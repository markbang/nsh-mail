//! The outbox: messages that wait before they go, for the undo window or until a scheduled
//! time. They live in the store with their files, so a restart (of the app or the server)
//! neither loses nor blocks them; one task per engine sends each when it is due.

use super::*;
use crate::store::Queued;

/// The longest undo window a client may ask for, in seconds.
const MAX_DELAY: u32 = 5 * 60;
/// A scheduled time this far in the past still counts as "now" (clocks differ a little).
const PAST_GRACE: i64 = 60_000;
/// Sent this long after its time, a message is reported as late.
const LATE_AFTER: i64 = 60_000;
/// The dispatcher looks at the wall clock at least this often: a device waking from sleep
/// should not wait out a long timer.
const MAX_WAIT: Duration = Duration::from_secs(30);
/// Network failures are retried this many times, a minute apart and doubling.
const RETRIES: u32 = 5;

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn invalid(message: &str) -> Error {
    err(ErrorCode::InvalidInput, message)
}

impl Engine {
    /// Puts a message in the outbox to go after the undo delay or at a scheduled time. Its
    /// files are kept with it: the `upload`s from `staged`, the `forward`s read now, so it no
    /// longer needs the message they came from. The returned draft names them all as uploads.
    pub async fn queue(
        &self,
        mut draft: Draft,
        mut staged: HashMap<String, StagedFile>,
        when: SendAt,
    ) -> Result<Outgoing> {
        let now = now_ms();
        let (send_at, scheduled) = match (when.delay, when.send_at) {
            (Some(d), None) if d <= MAX_DELAY => (now + i64::from(d) * 1000, false),
            (Some(_), None) => return Err(invalid("the undo delay is at most 5 minutes")),
            (None, Some(at)) if at >= now - PAST_GRACE => (at.max(now), true),
            (None, Some(_)) => return Err(invalid("pick a time in the future")),
            _ => return Err(invalid("give either a delay or a time to send at")),
        };
        if draft.to.is_empty() && draft.cc.is_empty() && draft.bcc.is_empty() {
            return Err(invalid("add at least one recipient"));
        }
        self.account(&draft.account_id).await?;
        let mut files = HashMap::new();
        let mut attachments = Vec::with_capacity(draft.attachments.len());
        let mut total = 0u64;
        for attachment in std::mem::take(&mut draft.attachments) {
            let (id, file) = match attachment {
                DraftAttachment::Upload { id } => {
                    let file = staged.remove(&id).ok_or_else(|| {
                        err(
                            ErrorCode::NotFound,
                            "an attached file is gone; attach it again",
                        )
                    })?;
                    (id, file)
                }
                DraftAttachment::Forward { message_id, index } => {
                    let (meta, data) = self.attachment(&message_id, index).await?;
                    let file = StagedFile {
                        filename: meta.filename.unwrap_or_else(|| "attachment".into()),
                        mime_type: meta.mime_type,
                        data,
                    };
                    (new_id(), file)
                }
            };
            total += file.data.len() as u64;
            if total > self.inner.max_attachments {
                return Err(too_large(self.inner.max_attachments));
            }
            attachments.push(DraftAttachment::Upload { id: id.clone() });
            files.insert(id, file);
        }
        draft.attachments = attachments;
        let uploads = draft
            .attachments
            .iter()
            .filter_map(|a| match a {
                DraftAttachment::Upload { id } => files.get(id).map(|f| Upload {
                    id: id.clone(),
                    filename: f.filename.clone(),
                    mime_type: f.mime_type.clone(),
                    size: f.data.len() as u64,
                }),
                DraftAttachment::Forward { .. } => None,
            })
            .collect();
        let outgoing = Outgoing {
            id: new_id(),
            draft,
            send_at,
            scheduled,
            files: uploads,
            error: None,
        };
        let row = outgoing.clone();
        self.store()
            .run(move |s| s.queue_outgoing(&row, &files))
            .await?;
        self.inner.outbox_wake.notify_one();
        self.outbox_event(&outgoing, OutboxState::Queued, false, None);
        Ok(outgoing)
    }

    /// The messages waiting to go (or that failed), the next first.
    pub async fn outbox(&self) -> Result<Vec<Outgoing>> {
        self.store().run(|s| s.outbox()).await
    }

    /// Takes a message out of the outbox before it goes, with its files (by the ids its
    /// draft names them by). Fails once it is being sent or gone.
    pub async fn cancel(&self, id: &str) -> Result<(Outgoing, HashMap<String, StagedFile>)> {
        let id = id.to_string();
        let q = self.store().run(move |s| s.take_outgoing(&id)).await?;
        self.outbox_event(&q.outgoing, OutboxState::Cancelled, false, None);
        Ok((q.outgoing, q.files))
    }

    fn outbox_event(&self, o: &Outgoing, state: OutboxState, late: bool, error: Option<Error>) {
        self.emit(MailEvent::Outbox {
            account_id: o.draft.account_id.clone(),
            id: o.id.clone(),
            subject: o.draft.subject.clone(),
            state,
            late,
            error,
        });
    }

    async fn send_due(&self) -> Result<()> {
        let now = now_ms();
        let due = self.store().run(move |s| s.claim_due(now)).await?;
        for q in due {
            self.send_queued(q).await;
        }
        Ok(())
    }

    async fn send_queued(&self, q: Queued) {
        let Queued {
            outgoing: o,
            files,
            attempts,
        } = q;
        let late = now_ms() - o.send_at > LATE_AFTER;
        let id = o.id.clone();
        let result = match self.send_with(o.draft.clone(), files).await {
            Ok(()) => {
                let done = self.store().run(move |s| s.outgoing_sent(&id)).await;
                self.outbox_event(&o, OutboxState::Sent, late, None);
                done
            }
            Err(e) if e.code == ErrorCode::Network && attempts < RETRIES => {
                let at = now_ms() + 60_000 * (1 << attempts);
                tracing::warn!(outbox = %o.id, "sending failed, retrying in {}s: {}", (at - now_ms()) / 1000, e.message);
                self.store()
                    .run(move |s| s.outgoing_retry(&id, at, &e))
                    .await
            }
            Err(e) => {
                let error = e.clone();
                let done = self
                    .store()
                    .run(move |s| s.outgoing_failed(&id, &error))
                    .await;
                self.outbox_event(&o, OutboxState::Failed, late, Some(e));
                done
            }
        };
        if let Err(e) = result {
            tracing::error!(outbox = %o.id, "cannot update the outbox: {}", e.message);
        }
    }
}

/// Sends what is due, then sleeps until the next is (or `queue` wakes it). Holds only a weak
/// reference while waiting, so dropping the last `Engine` ends it.
pub(super) async fn outbox_loop(inner: Weak<Inner>) {
    if let Some(engine) = upgrade(&inner) {
        let error = err(
            ErrorCode::Internal,
            "sending was interrupted; check Sent before sending it again",
        );
        match engine
            .store()
            .run(move |s| s.outgoing_interrupted(&error))
            .await
        {
            Ok(stuck) => {
                for o in stuck {
                    let e = o.error.clone();
                    engine.outbox_event(&o, OutboxState::Failed, false, e);
                }
            }
            Err(e) => tracing::error!("cannot read the outbox: {}", e.message),
        }
    }
    loop {
        let (wake, wait) = {
            let Some(engine) = upgrade(&inner) else {
                return;
            };
            let now = now_ms();
            let wait = match engine.store().run(|s| s.next_due()).await {
                Ok(Some(at)) if at <= now => {
                    if let Err(e) = engine.send_due().await {
                        tracing::error!("cannot send from the outbox: {}", e.message);
                    } else {
                        continue;
                    }
                    MAX_WAIT
                }
                Ok(Some(at)) => Duration::from_millis((at - now) as u64).min(MAX_WAIT),
                Ok(None) => MAX_WAIT,
                Err(e) => {
                    tracing::error!("cannot read the outbox: {}", e.message);
                    MAX_WAIT
                }
            };
            (engine.inner.outbox_wake.clone(), wait)
        };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = wake.notified() => {}
        }
    }
}
