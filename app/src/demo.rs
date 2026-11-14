//! `DemoBackend`: three accounts and a few weeks of mail, kept in memory,
//! for the UI tests (debug builds only). Sending, flags, search and adding
//! accounts behave as the real backends do; refreshing delivers a new
//! message, and so does a timer while the app runs.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

use mail_core::types::*;
use nsh_core::{prelude::*, time::unix_ms};

use crate::backend::{Backend, Events, File, MailFuture, Progress, Staged, invalid, rename_upload};
use crate::notify::{
    ApiKey, CreatedApiKey, CreatedSource, FeedPage, FeedQuery, NewApiKey, NewSource, Notification,
    Source, SourceKind,
};

const MINUTE: f64 = 60_000.0;
const HOUR: f64 = 60.0 * MINUTE;
const DAY: f64 = 24.0 * HOUR;
/// How often a message arrives on its own.
const ARRIVAL: Duration = Duration::from_secs(90);

#[derive(Default)]
struct Data {
    accounts: Vec<Account>,
    messages: Vec<Message>,
    sources: Vec<Source>,
    feed: Vec<Notification>,
    keys: Vec<ApiKey>,
    next_id: u64,
    next_source: u64,
    next_key: u64,
    arrivals: usize,
    events: Option<Events>,
    outbox: Vec<(Outgoing, Vec<(String, Rc<File>)>)>,
}

impl Data {
    fn id(&mut self) -> String {
        self.next_id += 1;
        format!("m{}", self.next_id)
    }

    fn emit(&self, event: MailEvent) {
        if let Some(events) = &self.events {
            events(event);
        }
    }

    fn account(&self, id: &str) -> Result<&Account> {
        self.accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| not_found("account"))
    }

    /// The mail source of an account (`src-mail-…`), added with the account.
    fn mail_source(&self, account: &str) -> Option<&str> {
        self.sources
            .iter()
            .find(|s| s.kind == SourceKind::Mail && s.id == format!("src-mail-{account}"))
            .map(|s| s.id.as_str())
    }

    /// The feed's view of a mail message: mail keeps its own rows, and the
    /// feed reads a projection of the inbox's (one row per message).
    fn project(&mut self, m: &Message) {
        let Some(source) = self.mail_source(&m.summary.account_id).map(str::to_owned) else {
            return;
        };
        if m.summary.folder != FolderKind::Inbox {
            return;
        }
        let from = m.summary.from.clone();
        let sender = from
            .as_ref()
            .map(|a| a.name.clone().unwrap_or_else(|| a.email.clone()))
            .unwrap_or_default();
        let n = Notification {
            id: m.summary.id.clone(),
            source_id: source,
            sender,
            title: m.summary.subject.clone(),
            body: m.text.clone().unwrap_or_else(|| m.summary.snippet.clone()),
            link: None,
            received_at: m.summary.date,
            priority: 4,
            tags: Vec::new(),
            read: m.summary.seen,
            archived: false,
            thread_id: Some(m.summary.thread_id.clone()),
        };
        self.feed.retain(|f| f.id != n.id);
        self.feed.push(n);
    }

    fn add_source(&mut self, source: Source) {
        self.sources.push(source);
    }

    /// A notification that arrives on its own, as a webhook or a bot's would.
    fn deliver_notification(&mut self) -> Option<Notification> {
        let (source, sender, title, body, priority, tags) = NOTICES[self.arrivals % NOTICES.len()];
        self.arrivals += 1;
        if self.sources.iter().all(|s| s.id != source) {
            return None;
        }
        let n = Notification {
            id: notif_id(self),
            source_id: source.to_owned(),
            sender: sender.to_owned(),
            title: title.to_owned(),
            body: body.to_owned(),
            link: None,
            received_at: unix_ms() as i64,
            priority,
            tags: tags.iter().map(|t| (*t).to_owned()).collect(),
            read: false,
            archived: false,
            thread_id: None,
        };
        self.feed.push(n.clone());
        Some(n)
    }

    /// Unread notifications per source, and all of them.
    fn unread_counts(&self) -> (HashMap<String, u32>, u32) {
        let mut counts = HashMap::new();
        for n in self.feed.iter().filter(|n| !n.read && !n.archived) {
            *counts.entry(n.source_id.clone()).or_default() += 1;
        }
        let total = counts.values().sum();
        (counts, total)
    }

    fn add_account(&mut self, mut account: Account) -> Result<Account> {
        account.email = account.email.trim().to_lowercase();
        if self.accounts.iter().any(|a| a.email == account.email) {
            return Err(invalid(format!("{} is already added", account.email)));
        }
        account.id = format!("a-{}", account.email.replace(['@', '.'], "-"));
        self.accounts.push(account.clone());
        let id = self.id();
        let text = format!(
            "{} is now part of your unified inbox.\n\nNew mail appears here as it arrives. \
             Pull down (or press Refresh) to check right away.",
            account.email
        );
        let m = message(
            id,
            &account,
            FolderKind::Inbox,
            unix_ms(),
            ("nsh-mail", "hello@nsh-mail.app"),
            "Your account is connected",
            &text,
        );
        let source = Source {
            id: format!("src-mail-{}", account.id),
            kind: SourceKind::Mail,
            name: account
                .provider
                .as_deref()
                .map(|p| match p {
                    "gmail" => "Gmail",
                    "outlook" => "Outlook",
                    "fastmail" => "Fastmail",
                    _ => "Mail",
                })
                .unwrap_or("Mail")
                .to_owned(),
            icon: None,
            color: Some("chart-3".to_owned()),
            enabled: true,
            min_priority: 0,
            created_at: unix_ms() as i64,
        };
        self.add_source(source);
        self.project(&m);
        self.messages.push(m);
        Ok(account)
    }

    fn deliver(&mut self) -> Option<Summary> {
        if self.accounts.is_empty() {
            return None;
        }
        let (from, subject, text) = ARRIVALS[self.arrivals % ARRIVALS.len()];
        let account = self.accounts[self.arrivals % self.accounts.len()].clone();
        self.arrivals += 1;
        let id = self.id();
        let m = message(
            id,
            &account,
            FolderKind::Inbox,
            unix_ms(),
            from,
            subject,
            text,
        );
        let summary = m.summary.clone();
        self.project(&m);
        self.messages.push(m);
        Some(summary)
    }

    /// Conversations with a message in the query's folder, newest first ([`in_view`]).
    fn threads(&self, query: &ListQuery) -> Vec<Thread> {
        let of_account = |m: &Message| {
            query
                .account
                .as_ref()
                .is_none_or(|a| &m.summary.account_id == a)
        };
        // A large mailbox lists thousands of conversations: group in one pass, by id.
        let listed: HashSet<&str> = self
            .messages
            .iter()
            .filter(|m| m.summary.folder == query.folder && of_account(m))
            .map(|m| m.summary.thread_id.as_str())
            .collect();
        let mut groups: HashMap<&str, Vec<(Summary, Option<String>)>> = HashMap::new();
        for m in self.messages.iter().filter(|m| {
            listed.contains(m.summary.thread_id.as_str())
                && in_view(m.summary.folder, query.folder)
                && of_account(m)
        }) {
            groups
                .entry(m.summary.thread_id.as_str())
                .or_default()
                .push((m.summary.clone(), m.message_id.clone()));
        }
        let mut threads: Vec<Thread> = groups
            .into_iter()
            .filter_map(|(id, v)| Thread::of(id.to_owned(), &conversation(v)))
            .collect();
        threads.sort_by(|a, b| {
            b.latest
                .date
                .cmp(&a.latest.date)
                .then_with(|| b.id.cmp(&a.id))
        });
        threads
    }

    fn thread(&self, id: &str) -> Vec<(Summary, Option<String>)> {
        self.messages
            .iter()
            .filter(|m| m.summary.thread_id == id)
            .map(|m| (m.summary.clone(), m.message_id.clone()))
            .collect()
    }

    /// Newest first; equal dates by id, so a page boundary is stable.
    fn sorted(&self, keep: impl Fn(&Message) -> bool) -> Vec<&Message> {
        let mut found: Vec<&Message> = self.messages.iter().filter(|m| keep(m)).collect();
        found.sort_by(|a, b| {
            let (a, b) = (&a.summary, &b.summary);
            b.date.cmp(&a.date).then_with(|| b.id.cmp(&a.id))
        });
        found
    }

    /// A page of the feed: the source and text filters, newest first, and the
    /// unread counts the filter chips show.
    fn feed_page(&self, query: &FeedQuery) -> FeedPage {
        let (counts, unread) = self.unread_counts();
        let words: Vec<String> = query
            .text
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let mut found: Vec<&Notification> = self
            .feed
            .iter()
            .filter(|n| {
                !n.archived
                    && query.source.as_ref().is_none_or(|s| &n.source_id == s)
                    && (!query.unread || !n.read)
                    && (words.is_empty() || {
                        let hay =
                            format!("{} {} {} {}", n.sender, n.title, n.body, n.tags.join(" "))
                                .to_lowercase();
                        words.iter().all(|w| hay.contains(w))
                    })
            })
            .collect();
        found.sort_by(|a, b| {
            b.received_at
                .cmp(&a.received_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        let start = match &query.before {
            Some(before) => found
                .iter()
                .position(|n| &n.id == before)
                .map_or(found.len(), |at| at + 1),
            None => 0,
        };
        let page: Vec<Notification> = found
            .iter()
            .skip(start)
            .take(query.limit.max(1) as usize)
            .map(|n| (*n).clone())
            .collect();
        let more = found.len() > start + page.len();
        FeedPage {
            items: page,
            counts,
            unread,
            more,
        }
    }
}

/// In-memory mail (see the module's docs).
pub struct DemoBackend {
    data: Rc<RefCell<Data>>,
    staged: Staged,
}

impl Default for DemoBackend {
    fn default() -> Self {
        Self {
            data: Rc::new(RefCell::new(seed(unix_ms()))),
            staged: Staged::default(),
        }
    }
}

impl DemoBackend {
    /// The demo mail at the size of a large mailbox (`nshmail.mode` = `demo-large`): 10,000 messages
    /// in 2,000 conversations, for the performance tests and measurements.
    pub fn large() -> Self {
        Self {
            data: Rc::new(RefCell::new(seed_large(unix_ms()))),
            staged: Staged::default(),
        }
    }
}

fn not_found(what: &str) -> Error {
    Error::new(ErrorCode::NotFound, format!("{what} not found"))
}

/// An id for a notification that is not mail's projection of a message.
fn notif_id(data: &mut Data) -> String {
    data.next_id += 1;
    format!("n{}", data.next_id)
}

/// A source's ingest token: 48 hex characters, as the server's 32 random bytes hexed.
fn token(seed: u64) -> String {
    let mut h = seed ^ 0x243f_6a88_85a3_08d3;
    let mut out = String::new();
    for _ in 0..6 {
        h = h
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        out.push_str(&format!("{:08x}", (h >> 16) as u32));
    }
    out
}

fn now_hex(data: &Data) -> u64 {
    unix_ms() as u64 ^ (data.next_source as u64).wrapping_mul(0x9e37_79b9)
}

/// Where a source's token is sent: the design's three ingest shapes.
fn ingest_url(kind: SourceKind, token: &str) -> Option<String> {
    let base = "https://mail-api.bangwu.me";
    match kind {
        SourceKind::Webhook => Some(format!("{base}/hooks/{token}")),
        SourceKind::Gotify => Some(format!("{base}/message?token={token}")),
        SourceKind::Ntfy => Some(format!("{base}/{token}")),
        _ => None,
    }
}

/// Something arrived in the feed: the shell reloads it as it reloads the mailboxes.
fn notification_event() -> MailEvent {
    MailEvent::Changed {
        account_id: String::new(),
        folder: FolderKind::Inbox,
    }
}

fn ok<'a, T: 'a>(value: Result<T>) -> MailFuture<'a, T> {
    Box::pin(async move { value })
}

/// Waits as a server would, so loading states show.
async fn latency(ms: u64) {
    timer(Duration::from_millis(ms)).await;
}

fn endpoint(host: &str, port: u16, security: Security) -> Endpoint {
    Endpoint {
        host: host.to_owned(),
        port,
        security,
    }
}

fn oauth(provider: OAuthProvider) -> Auth {
    Auth::OAuth { provider }
}

impl Backend for DemoBackend {
    fn start(&self, events: Events) -> MailFuture<'_, ()> {
        self.data.borrow_mut().events = Some(events);
        // The arrivals outlive this call: they stop when the backend goes.
        let data = Rc::downgrade(&self.data);
        Owner::detached().run(|| {
            spawn(async move {
                loop {
                    timer(ARRIVAL).await;
                    let Some(data) = data.upgrade() else { return };
                    let mut data = data.borrow_mut();
                    if let Some(summary) = data.deliver() {
                        let account_id = summary.account_id.clone();
                        data.emit(MailEvent::NewMail { summary });
                        data.emit(MailEvent::Changed {
                            account_id,
                            folder: FolderKind::Inbox,
                        });
                    }
                    if data.deliver_notification().is_some() {
                        data.emit(notification_event());
                    }
                }
            })
        });
        ok(Ok(()))
    }

    fn sign_out(&self) -> MailFuture<'_, ()> {
        *self.data.borrow_mut() = Data::default();
        ok(Ok(()))
    }

    fn accounts(&self) -> MailFuture<'_, Vec<Account>> {
        ok(Ok(self.data.borrow().accounts.clone()))
    }

    fn detect(&self, email: String) -> MailFuture<'_, Option<ServerSettings>> {
        Box::pin(async move {
            latency(300).await;
            let email = email.trim().to_lowercase();
            let domain = email.rsplit_once('@').map(|(_, d)| d.to_owned());
            let Some(domain) = domain.filter(|d| d.contains('.')) else {
                return Err(invalid("enter an email address"));
            };
            Ok(preset(&domain).map(|s| ServerSettings {
                username: email,
                ..s
            }))
        })
    }

    fn add_imap(&self, input: ImapInput) -> MailFuture<'_, Account> {
        Box::pin(async move {
            if parse_address(&input.email).is_none() {
                return Err(invalid("enter an email address"));
            }
            if input.imap.host.trim().is_empty() || input.smtp.host.trim().is_empty() {
                return Err(invalid("enter the IMAP and SMTP servers"));
            }
            if input.imap.port == 0 || input.smtp.port == 0 {
                return Err(invalid("enter the IMAP and SMTP ports"));
            }
            if input.password.is_empty() {
                return Err(invalid("enter the app password"));
            }
            latency(700).await;
            let username = input
                .username
                .clone()
                .filter(|u| !u.trim().is_empty())
                .unwrap_or_else(|| input.email.clone());
            self.data.borrow_mut().add_account(Account {
                email: input.email,
                name: input.name.filter(|n| !n.trim().is_empty()),
                provider: input.provider,
                username,
                auth: Auth::Password,
                imap: input.imap,
                smtp: input.smtp,
                id: String::new(),
                signature: Signature::default(),
            })
        })
    }

    fn add_oauth(&self, provider: OAuthProvider) -> MailFuture<'_, Account> {
        Box::pin(async move {
            latency(900).await;
            let (email, id) = match provider {
                OAuthProvider::Google => ("demo.reader@gmail.com", "gmail"),
                OAuthProvider::Microsoft => ("demo.reader@outlook.com", "outlook"),
            };
            let settings = preset(email.rsplit_once('@').map_or("", |(_, d)| d))
                .ok_or_else(|| not_found("preset"))?;
            self.data.borrow_mut().add_account(Account {
                id: String::new(),
                email: email.to_owned(),
                name: Some("Demo Reader".to_owned()),
                provider: Some(id.to_owned()),
                username: email.to_owned(),
                auth: oauth(provider),
                imap: settings.imap,
                smtp: settings.smtp,
                signature: Signature::default(),
            })
        })
    }

    fn remove_account(&self, id: String) -> MailFuture<'_, ()> {
        let mut data = self.data.borrow_mut();
        let result = data.account(&id).map(|_| ());
        if result.is_ok() {
            data.accounts.retain(|a| a.id != id);
            data.messages.retain(|m| m.summary.account_id != id);
        }
        ok(result)
    }

    fn set_signature(&self, id: String, signature: Signature) -> MailFuture<'_, Account> {
        let mut data = self.data.borrow_mut();
        let found = data.accounts.iter_mut().find(|a| a.id == id);
        ok(found.ok_or_else(|| not_found("account")).map(|a| {
            a.signature = signature;
            a.clone()
        }))
    }

    fn list(&self, query: ListQuery) -> MailFuture<'_, Vec<Summary>> {
        Box::pin(async move {
            latency(120).await;
            let data = self.data.borrow();
            let found = data.sorted(|m| {
                m.summary.folder == query.folder
                    && query
                        .account
                        .as_ref()
                        .is_none_or(|a| &m.summary.account_id == a)
            });
            let start = match &query.before {
                Some(before) => found
                    .iter()
                    .position(|m| &m.summary.id == before)
                    .map_or(found.len(), |at| at + 1),
                None => 0,
            };
            Ok(found
                .into_iter()
                .skip(start)
                .take(query.limit.max(1) as usize)
                .map(|m| m.summary.clone())
                .collect())
        })
    }

    fn older(&self, folder: FolderKind, account: Option<String>) -> MailFuture<'_, Backfill> {
        let _ = (folder, account);
        ok(Ok(Backfill::default()))
    }

    fn threads(&self, query: ListQuery) -> MailFuture<'_, Vec<Thread>> {
        Box::pin(async move {
            latency(120).await;
            let found = self.data.borrow().threads(&query);
            let start = match &query.before {
                Some(before) => found
                    .iter()
                    .position(|t| &t.id == before)
                    .map_or(found.len(), |at| at + 1),
                None => 0,
            };
            Ok(found
                .into_iter()
                .skip(start)
                .take(query.limit.max(1) as usize)
                .collect())
        })
    }

    fn thread(&self, id: String) -> MailFuture<'_, Vec<Summary>> {
        Box::pin(async move {
            latency(60).await;
            let messages = conversation(self.data.borrow().thread(&id));
            if messages.is_empty() {
                return Err(not_found("conversation"));
            }
            Ok(messages)
        })
    }

    fn set_thread_flags(&self, id: String, flags: Flags) -> MailFuture<'_, ()> {
        let mut data = self.data.borrow_mut();
        let latest = data
            .messages
            .iter()
            .filter(|m| m.summary.thread_id == id && m.summary.folder != FolderKind::Trash)
            .max_by_key(|m| m.summary.date)
            .map(|m| m.summary.id.clone());
        let mut changed = Vec::new();
        for Message { summary: m, .. } in data
            .messages
            .iter_mut()
            .filter(|m| m.summary.thread_id == id)
        {
            m.seen = flags.seen.unwrap_or(m.seen);
            match flags.flagged {
                Some(true) if latest.as_ref() == Some(&m.id) => m.flagged = true,
                Some(false) => m.flagged = false,
                _ => {}
            }
            changed.push((m.account_id.clone(), m.folder));
        }
        if changed.is_empty() {
            return ok(Err(not_found("conversation")));
        }
        changed.dedup();
        for (account_id, folder) in changed {
            data.emit(MailEvent::Changed { account_id, folder });
        }
        ok(Ok(()))
    }

    fn move_thread(&self, id: String, folder: FolderKind) -> MailFuture<'_, ()> {
        let mut data = self.data.borrow_mut();
        let moves = |m: &Message| {
            m.summary.thread_id == id
                && match folder {
                    FolderKind::Archive => m.summary.folder == FolderKind::Inbox,
                    _ => m.summary.folder != FolderKind::Trash,
                }
        };
        if !matches!(folder, FolderKind::Archive | FolderKind::Trash) {
            return ok(Err(invalid("a conversation moves to archive or trash")));
        }
        let mut changed = Vec::new();
        for Message { summary: m, .. } in data.messages.iter_mut().filter(|m| moves(m)) {
            changed.push((
                m.account_id.clone(),
                std::mem::replace(&mut m.folder, folder),
            ));
            changed.push((m.account_id.clone(), folder));
        }
        changed.dedup();
        for (account_id, folder) in changed {
            data.emit(MailEvent::Changed { account_id, folder });
        }
        ok(Ok(()))
    }

    fn get(&self, id: String) -> MailFuture<'_, Message> {
        Box::pin(async move {
            latency(80).await;
            let data = self.data.borrow();
            data.messages
                .iter()
                .find(|m| m.summary.id == id)
                .cloned()
                .ok_or_else(|| not_found("message"))
        })
    }

    fn attachment(&self, id: String, index: u32) -> MailFuture<'_, Vec<u8>> {
        Box::pin(async move {
            latency(80).await;
            let data = self.data.borrow();
            let message = data.messages.iter().find(|m| m.summary.id == id);
            message
                .and_then(|m| m.attachments.get(index as usize))
                .map(|a| {
                    format!("{} (demo)", a.filename.as_deref().unwrap_or_default()).into_bytes()
                })
                .ok_or_else(|| not_found("attachment"))
        })
    }

    fn set_flags(&self, id: String, flags: Flags) -> MailFuture<'_, ()> {
        let mut data = self.data.borrow_mut();
        let result = match data.messages.iter_mut().find(|m| m.summary.id == id) {
            Some(Message { summary: m, .. }) => {
                m.seen = flags.seen.unwrap_or(m.seen);
                m.flagged = flags.flagged.unwrap_or(m.flagged);
                Ok((m.account_id.clone(), m.folder))
            }
            None => Err(not_found("message")),
        };
        ok(result.map(|(account_id, folder)| {
            data.emit(MailEvent::Changed { account_id, folder });
        }))
    }

    fn move_to(&self, id: String, folder: FolderKind) -> MailFuture<'_, ()> {
        let mut data = self.data.borrow_mut();
        let result = match data.messages.iter_mut().find(|m| m.summary.id == id) {
            Some(Message { summary: m, .. }) => Ok((
                m.account_id.clone(),
                std::mem::replace(&mut m.folder, folder),
            )),
            None => Err(not_found("message")),
        };
        ok(result.map(|(account_id, from)| {
            data.emit(MailEvent::Changed {
                account_id: account_id.clone(),
                folder: from,
            });
            data.emit(MailEvent::Changed { account_id, folder });
        }))
    }

    fn send(&self, draft: Draft) -> MailFuture<'_, ()> {
        Box::pin(async move {
            check_recipients(&draft)?;
            let staged = self.staged.of(&draft)?;
            latency(600).await;
            post(&mut self.data.borrow_mut(), &draft, &staged)?;
            self.staged.sent(&draft);
            Ok(())
        })
    }

    fn queue(&self, mut draft: Draft, when: SendAt) -> MailFuture<'_, Outgoing> {
        Box::pin(async move {
            check_recipients(&draft)?;
            let staged = self.staged.of(&draft)?;
            latency(150).await;
            let now = unix_ms() as i64;
            let (send_at, scheduled) = match (when.delay, when.send_at) {
                (Some(delay), None) => (now + i64::from(delay) * 1000, false),
                (None, Some(at)) if at >= now - 60_000 => (at.max(now), true),
                (None, Some(_)) => return Err(invalid("pick a time in the future")),
                _ => return Err(invalid("give either a delay or a time to send at")),
            };
            let mut data = self.data.borrow_mut();
            // As the engine does: forwarded files are copied, every file becomes an upload.
            let mut files = Vec::new();
            for a in &draft.attachments {
                match a {
                    DraftAttachment::Upload { id } => {
                        let (_, f) = staged
                            .iter()
                            .find(|(s, _)| s == id)
                            .ok_or_else(|| not_found("file"))?;
                        files.push((id.clone(), f.clone()));
                    }
                    DraftAttachment::Forward { message_id, index } => {
                        let a = data
                            .messages
                            .iter()
                            .find(|m| &m.summary.id == message_id)
                            .and_then(|m| m.attachments.iter().find(|a| a.index == *index))
                            .ok_or_else(|| not_found("attachment"))?;
                        let file = File {
                            name: a.filename.clone().unwrap_or_default(),
                            mime_type: a.mime_type.clone(),
                            data: vec![0; a.size as usize],
                        };
                        let id = format!("o{}", data.id());
                        files.push((id, Rc::new(file)));
                    }
                }
            }
            draft.attachments = files
                .iter()
                .map(|(id, _)| DraftAttachment::Upload { id: id.clone() })
                .collect();
            let outgoing = Outgoing {
                id: format!("out-{}", data.id()),
                files: files
                    .iter()
                    .map(|(id, f)| Upload {
                        id: id.clone(),
                        filename: f.name.clone(),
                        mime_type: f.mime_type.clone(),
                        size: f.data.len() as u64,
                    })
                    .collect(),
                draft,
                send_at,
                scheduled,
                error: None,
            };
            for (id, _) in &files {
                self.staged.remove(id);
            }
            data.outbox.push((outgoing.clone(), files));
            data.emit(outbox_event(&outgoing, OutboxState::Queued));
            drop(data);
            let (weak, id) = (Rc::downgrade(&self.data), outgoing.id.clone());
            let wait = Duration::from_millis((send_at - now).max(0) as u64);
            Owner::detached().run(|| {
                spawn(async move {
                    timer(wait).await;
                    let Some(data) = weak.upgrade() else { return };
                    let mut data = data.borrow_mut();
                    let Some(at) = data.outbox.iter().position(|(o, _)| o.id == id) else {
                        return;
                    };
                    let (outgoing, files) = data.outbox.remove(at);
                    match post(&mut data, &outgoing.draft, &files) {
                        Ok(()) => {
                            data.emit(outbox_event(&outgoing, OutboxState::Sent));
                            data.emit(MailEvent::Changed {
                                account_id: outgoing.draft.account_id.clone(),
                                folder: FolderKind::Sent,
                            });
                        }
                        Err(error) => {
                            let mut failed = outgoing.clone();
                            failed.error = Some(error.clone());
                            data.outbox.push((failed, files));
                            let mut event = outbox_event(&outgoing, OutboxState::Failed);
                            if let MailEvent::Outbox { error: e, .. } = &mut event {
                                *e = Some(error);
                            }
                            data.emit(event);
                        }
                    }
                })
            });
            Ok(outgoing)
        })
    }

    fn outbox(&self) -> MailFuture<'_, Vec<Outgoing>> {
        let mut list: Vec<Outgoing> = self
            .data
            .borrow()
            .outbox
            .iter()
            .map(|(o, _)| o.clone())
            .collect();
        list.sort_by_key(|o| o.send_at);
        ok(Ok(list))
    }

    fn cancel(&self, id: String) -> MailFuture<'_, Outgoing> {
        let mut data = self.data.borrow_mut();
        let Some(at) = data.outbox.iter().position(|(o, _)| o.id == id) else {
            return ok(Err(Error::new(
                ErrorCode::NotFound,
                "this message is no longer waiting: it was sent or cancelled",
            )));
        };
        let (mut outgoing, files) = data.outbox.remove(at);
        for (id, f) in files {
            let file = File {
                name: f.name.clone(),
                mime_type: f.mime_type.clone(),
                data: f.data.clone(),
            };
            let staged = self.staged.restore(id.clone(), file);
            if staged != id {
                rename_upload(&mut outgoing, &id, &staged);
            }
        }
        data.emit(outbox_event(&outgoing, OutboxState::Cancelled));
        ok(Ok(outgoing))
    }

    fn stage(&self, file: File, progress: Progress) -> MailFuture<'_, Upload> {
        Box::pin(async move {
            // In steps, so the composer's progress shows as it would against a server.
            let size = file.data.len() as u64;
            for step in 1..=4 {
                latency(120).await;
                progress(size * step / 4);
            }
            Ok(self.staged.add(file))
        })
    }

    fn unstage(&self, id: String) -> MailFuture<'_, ()> {
        self.staged.remove(&id);
        ok(Ok(()))
    }

    fn search(&self, text: String, limit: u32) -> MailFuture<'_, Vec<Summary>> {
        Box::pin(async move {
            latency(150).await;
            let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
            if words.is_empty() {
                return Ok(Vec::new());
            }
            let data = self.data.borrow();
            let found = data.sorted(
                |Message {
                     summary: s, text, ..
                 }| {
                    let from = s
                        .from
                        .as_ref()
                        .map(|a| format!("{} {}", a.name.as_deref().unwrap_or(""), a.email))
                        .unwrap_or_default();
                    let to: Vec<&str> = s.to.iter().map(|a| a.email.as_str()).collect();
                    let hay = format!(
                        "{} {from} {} {}",
                        s.subject,
                        to.join(" "),
                        text.as_deref().unwrap_or("")
                    )
                    .to_lowercase();
                    words.iter().all(|w| hay.contains(w))
                },
            );
            Ok(found
                .into_iter()
                .take(limit.max(1) as usize)
                .map(|m| m.summary.clone())
                .collect())
        })
    }

    fn unread(&self) -> MailFuture<'_, HashMap<AccountId, u32>> {
        Box::pin(async move {
            let mut counts = HashMap::new();
            for m in &self.data.borrow().messages {
                if m.summary.folder == FolderKind::Inbox && !m.summary.seen {
                    *counts.entry(m.summary.account_id.clone()).or_default() += 1;
                }
            }
            Ok(counts)
        })
    }

    fn refresh(&self) -> MailFuture<'_, ()> {
        Box::pin(async move {
            let ids: Vec<String> = self
                .data
                .borrow()
                .accounts
                .iter()
                .map(|a| a.id.clone())
                .collect();
            let state = |account_id: &String, syncing| MailEvent::SyncState {
                account_id: account_id.clone(),
                syncing,
                error: None,
            };
            for id in &ids {
                self.data.borrow().emit(state(id, true));
            }
            latency(800).await;
            let mut data = self.data.borrow_mut();
            if let Some(summary) = data.deliver() {
                data.emit(MailEvent::NewMail { summary });
            }
            for id in ids {
                data.emit(state(&id, false));
                data.emit(MailEvent::Changed {
                    account_id: id,
                    folder: FolderKind::Inbox,
                });
            }
            // A webhook or a bot wrote to the feed too.
            if data.deliver_notification().is_some() {
                data.emit(notification_event());
            }
            Ok(())
        })
    }

    fn sources(&self) -> MailFuture<'_, Vec<Source>> {
        ok(Ok(self.data.borrow().sources.clone()))
    }

    fn feed(&self, query: FeedQuery) -> MailFuture<'_, FeedPage> {
        Box::pin(async move {
            latency(120).await;
            Ok(self.data.borrow().feed_page(&query))
        })
    }

    fn mark_read(&self, ids: Vec<String>, read: bool) -> MailFuture<'_, ()> {
        let mut data = self.data.borrow_mut();
        for n in data.feed.iter_mut().filter(|n| ids.contains(&n.id)) {
            n.read = read;
        }
        // Mail keeps its own flags: the projection follows them.
        for m in data
            .messages
            .iter_mut()
            .filter(|m| ids.contains(&m.summary.id))
        {
            m.summary.seen = read;
        }
        ok(Ok(()))
    }

    fn archive(&self, ids: Vec<String>) -> MailFuture<'_, ()> {
        let mut data = self.data.borrow_mut();
        for n in data.feed.iter_mut().filter(|n| ids.contains(&n.id)) {
            n.archived = true;
            n.read = true;
        }
        // Archiving mail's notification archives the message, as the server does in one transaction.
        for m in data
            .messages
            .iter_mut()
            .filter(|m| ids.contains(&m.summary.id) && m.summary.folder == FolderKind::Inbox)
        {
            m.summary.folder = FolderKind::Archive;
            m.summary.seen = true;
        }
        ok(Ok(()))
    }

    fn create_source(&self, input: NewSource) -> MailFuture<'_, CreatedSource> {
        Box::pin(async move {
            if input.name.trim().is_empty() {
                return Err(invalid("name the source"));
            }
            latency(300).await;
            let mut data = self.data.borrow_mut();
            data.next_source += 1;
            let source = Source {
                id: format!("src-new-{}", data.next_source),
                kind: input.kind,
                name: input.name.trim().to_owned(),
                icon: input.icon.filter(|i| !i.trim().is_empty()),
                color: Some(input.color.unwrap_or_else(|| "chart-1".to_owned())),
                enabled: true,
                min_priority: input.min_priority.min(10),
                created_at: unix_ms() as i64,
            };
            let url = ingest_url(source.kind, &token(now_hex(&data)));
            data.sources.push(source.clone());
            drop(data);
            Ok(CreatedSource {
                source,
                ingest_url: url,
            })
        })
    }

    fn update_source(&self, source: Source) -> MailFuture<'_, Source> {
        let mut data = self.data.borrow_mut();
        let found = data.sources.iter_mut().find(|s| s.id == source.id);
        ok(found.ok_or_else(|| not_found("source")).map(|s| {
            *s = source;
            s.clone()
        }))
    }

    fn delete_source(&self, id: String) -> MailFuture<'_, ()> {
        let mut data = self.data.borrow_mut();
        let before = data.sources.len();
        data.sources.retain(|s| s.id != id);
        data.feed.retain(|n| n.source_id != id);
        ok(if data.sources.len() == before {
            Err(not_found("source"))
        } else {
            Ok(())
        })
    }

    fn rotate_source_token(&self, id: String) -> MailFuture<'_, CreatedSource> {
        let mut data = self.data.borrow_mut();
        let Some(source) = data.sources.iter().find(|s| s.id == id).cloned() else {
            return ok(Err(not_found("source")));
        };
        if !source.kind.ingest() {
            return ok(Err(invalid("this source has no ingest URL")));
        }
        data.next_source += 1;
        let url = ingest_url(source.kind, &token(now_hex(&data)));
        ok(Ok(CreatedSource {
            source,
            ingest_url: url,
        }))
    }

    fn test_source(&self, id: String) -> MailFuture<'_, ()> {
        let mut data = self.data.borrow_mut();
        let Some(source) = data.sources.iter().find(|s| s.id == id).cloned() else {
            return ok(Err(not_found("source")));
        };
        let n = Notification {
            id: notif_id(&mut data),
            source_id: source.id.clone(),
            sender: source.name.clone(),
            title: "Test message".to_owned(),
            body: format!(
                "If this is in the feed, {} is connected. Messages sent to it land here.",
                source.name
            ),
            link: None,
            received_at: unix_ms() as i64,
            priority: 5,
            tags: vec!["test".to_owned()],
            read: false,
            archived: false,
            thread_id: None,
        };
        data.feed.push(n.clone());
        drop(data);
        ok(Ok(()))
    }

    fn api_keys(&self) -> MailFuture<'_, Vec<ApiKey>> {
        ok(Ok(self.data.borrow().keys.clone()))
    }

    fn create_api_key(&self, input: NewApiKey) -> MailFuture<'_, CreatedApiKey> {
        Box::pin(async move {
            if input.name.trim().is_empty() {
                return Err(invalid("name the key"));
            }
            latency(200).await;
            let mut data = self.data.borrow_mut();
            data.next_key += 1;
            let raw = format!("nb_{:032x}", data.next_key as u128 * 0x9e3779b97f4a7c15u128);
            let now = unix_ms() as i64;
            let key = ApiKey {
                id: format!("key-{}", data.next_key),
                name: input.name.trim().to_owned(),
                prefix: raw.chars().take(11).collect(),
                read: true,
                write: input.write,
                source_ids: input.source_ids,
                created_at: now,
                last_used_at: None,
                expires_at: input
                    .expires_days
                    .map(|days| now + i64::from(days) * 86_400_000),
            };
            data.keys.push(key.clone());
            Ok(CreatedApiKey { key, token: raw })
        })
    }

    fn revoke_api_key(&self, id: String) -> MailFuture<'_, ()> {
        let mut data = self.data.borrow_mut();
        let before = data.keys.len();
        data.keys.retain(|k| k.id != id);
        ok(if data.keys.len() == before {
            Err(not_found("key"))
        } else {
            Ok(())
        })
    }
}

fn check_recipients(draft: &Draft) -> Result<()> {
    if draft.to.is_empty() {
        return Err(invalid("add a recipient"));
    }
    let recipients = draft.to.iter().chain(&draft.cc).chain(&draft.bcc);
    if let Some(bad) = recipients
        .into_iter()
        .find(|r| parse_address(&r.email).is_none())
    {
        return Err(invalid(format!("{} is not an email address", bad.email)));
    }
    Ok(())
}

fn outbox_event(o: &Outgoing, state: OutboxState) -> MailEvent {
    MailEvent::Outbox {
        account_id: o.draft.account_id.clone(),
        id: o.id.clone(),
        subject: o.draft.subject.clone(),
        state,
        late: false,
        error: None,
    }
}

/// Sends `draft` from the demo's mailboxes: a copy in Sent, and in the inbox
/// of any own account it is addressed to.
fn post(data: &mut Data, draft: &Draft, staged: &[(String, Rc<File>)]) -> Result<()> {
    let recipients: Vec<&Address> = draft.to.iter().chain(&draft.cc).chain(&draft.bcc).collect();
    let mut attachments = Vec::new();
    for a in &draft.attachments {
        let (filename, mime_type, size) = match a {
            DraftAttachment::Upload { id } => {
                let (_, f) = staged
                    .iter()
                    .find(|(s, _)| s == id)
                    .ok_or_else(|| not_found("file"))?;
                (f.name.clone(), f.mime_type.clone(), f.data.len() as u64)
            }
            DraftAttachment::Forward { message_id, index } => {
                let source = data
                    .messages
                    .iter()
                    .find(|m| &m.summary.id == message_id)
                    .ok_or_else(|| not_found("message"))?;
                let a = source
                    .attachments
                    .iter()
                    .find(|a| a.index == *index)
                    .ok_or_else(|| not_found("attachment"))?;
                (
                    a.filename.clone().unwrap_or_default(),
                    a.mime_type.clone(),
                    a.size,
                )
            }
        };
        attachments.push(Attachment {
            index: attachments.len() as u32,
            filename: Some(filename),
            mime_type,
            size,
            inline: false,
            content_id: None,
        });
    }
    if attachments.iter().map(|a| a.size).sum::<u64>() > MAX_ATTACHMENTS {
        return Err(Error::new(
            ErrorCode::TooLarge,
            "the attachments are too large",
        ));
    }
    let from = data.account(&draft.account_id)?.clone();
    let id = data.id();
    let sender = (
        from.name.as_deref().unwrap_or(&from.email),
        from.email.as_str(),
    );
    let mut sent = message(
        id,
        &from,
        FolderKind::Sent,
        unix_ms(),
        sender,
        &draft.subject,
        &draft.text,
    );
    sent.summary.to = draft.to.clone();
    sent.cc = draft.cc.clone();
    sent.summary.seen = true;
    sent.in_reply_to = draft.in_reply_to.clone();
    sent.references = draft.references.clone();
    if let Some(parent) = data.messages.iter().find(|m| {
        m.message_id.is_some()
            && (m.message_id == draft.in_reply_to
                || draft.references.contains(m.message_id.as_ref().unwrap()))
    }) {
        sent.summary.thread_id = parent.summary.thread_id.clone();
    }
    sent.summary.has_attachments = !attachments.is_empty();
    sent.attachments = attachments;
    data.messages.push(sent.clone());
    // Mail to one's own accounts arrives in their inboxes too.
    let own: Vec<Account> = data
        .accounts
        .iter()
        .filter(|a| {
            recipients
                .iter()
                .any(|r| r.email.eq_ignore_ascii_case(&a.email))
        })
        .cloned()
        .collect();
    for account in own {
        let mut copy = sent.clone();
        copy.summary.id = data.id();
        copy.summary.account_id = account.id.clone();
        copy.summary.folder = FolderKind::Inbox;
        copy.summary.seen = false;
        let summary = copy.summary.clone();
        data.messages.push(copy);
        data.emit(MailEvent::NewMail { summary });
    }
    Ok(())
}

fn parse_address(raw: &str) -> Option<Address> {
    let raw = raw.trim();
    let (name, email) = match (raw.rfind('<'), raw.ends_with('>')) {
        (Some(open), true) => (
            Some(raw[..open].trim().trim_matches('"').to_owned()),
            raw[open + 1..raw.len() - 1].trim(),
        ),
        _ => (None, raw),
    };
    let (local, domain) = email.split_once('@')?;
    if local.is_empty() || !domain.contains('.') || email.contains(char::is_whitespace) {
        return None;
    }
    Some(Address {
        name: name.filter(|n| !n.is_empty()),
        email: email.to_owned(),
    })
}

fn message(
    id: String,
    account: &Account,
    folder: FolderKind,
    date: f64,
    from: (&str, &str),
    subject: &str,
    text: &str,
) -> Message {
    let snippet: String = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(140)
        .collect();
    Message {
        message_id: Some(format!("<{id}@demo.nsh-mail>")),
        summary: Summary {
            id: id.clone(),
            account_id: account.id.clone(),
            folder,
            from: Some(Address {
                name: Some(from.0.to_owned()),
                email: from.1.to_owned(),
            }),
            to: vec![Address {
                name: account.name.clone(),
                email: account.email.clone(),
            }],
            subject: subject.to_owned(),
            snippet,
            date: date as i64,
            seen: false,
            flagged: false,
            has_attachments: false,
            thread_id: format!("t-{id}"),
        },
        cc: Vec::new(),
        reply_to: Vec::new(),
        in_reply_to: None,
        references: Vec::new(),
        text: Some(text.to_owned()),
        html: None,
        remote_images: Vec::new(),
        attachments: Vec::new(),
    }
}

fn preset(domain: &str) -> Option<ServerSettings> {
    let tls = |id: &str, imap: &str, smtp: &str, name: &str, help: Option<&str>| ServerSettings {
        provider: Some(id.to_owned()),
        display_name: Some(name.to_owned()),
        username: String::new(),
        imap: endpoint(imap, 993, Security::Tls),
        smtp: endpoint(smtp, 465, Security::Tls),
        auth: Auth::Password,
        app_password: true,
        help_url: help.map(str::to_owned),
    };
    Some(match domain {
        "gmail.com" | "googlemail.com" => ServerSettings {
            auth: oauth(OAuthProvider::Google),
            app_password: false,
            ..tls("gmail", "imap.gmail.com", "smtp.gmail.com", "Gmail", None)
        },
        "outlook.com" | "hotmail.com" | "live.com" => ServerSettings {
            smtp: endpoint("smtp.office365.com", 587, Security::StartTls),
            auth: oauth(OAuthProvider::Microsoft),
            app_password: false,
            ..tls("outlook", "outlook.office365.com", "", "Outlook", None)
        },
        "qq.com" | "foxmail.com" => tls(
            "qq",
            "imap.qq.com",
            "smtp.qq.com",
            "QQ Mail",
            Some("https://service.mail.qq.com/detail/0/75"),
        ),
        "163.com" => tls(
            "163",
            "imap.163.com",
            "smtp.163.com",
            "NetEase 163",
            Some("https://help.mail.163.com/faq.do?m=list&categoryID=90"),
        ),
        "126.com" => tls(
            "126",
            "imap.126.com",
            "smtp.126.com",
            "NetEase 126",
            Some("https://help.mail.163.com/faq.do?m=list&categoryID=90"),
        ),
        "icloud.com" | "me.com" | "mac.com" => ServerSettings {
            smtp: endpoint("smtp.mail.me.com", 587, Security::StartTls),
            ..tls(
                "icloud",
                "imap.mail.me.com",
                "",
                "iCloud Mail",
                Some("https://support.apple.com/en-us/102654"),
            )
        },
        "fastmail.com" | "fastmail.fm" => tls(
            "fastmail",
            "imap.fastmail.com",
            "smtp.fastmail.com",
            "Fastmail",
            Some("https://www.fastmail.help/hc/en-us/articles/360058752854"),
        ),
        "yahoo.com" => tls(
            "yahoo",
            "imap.mail.yahoo.com",
            "smtp.mail.yahoo.com",
            "Yahoo Mail",
            Some("https://help.yahoo.com/kb/SLN15241.html"),
        ),
        _ => return None,
    })
}

/// What arrives on refresh and on the timer, in turn.
const ARRIVALS: &[((&str, &str), &str, &str)] = &[
    (
        ("Maya Patel", "maya@northwind.dev"),
        "Quick check on the release notes",
        "Hi Lin,\n\nCould you take one more look at the release notes before 4pm? I moved the migration section up \
         and trimmed the intro.\n\nThanks,\nMaya",
    ),
    (
        ("GitHub", "notifications@github.com"),
        "[northwind/atlas] CI passed on main",
        "All checks have passed on main.\n\n5 successful checks: core, server, linux, windows, web.",
    ),
    (
        ("顺丰速运", "service@sf-express.com"),
        "您的快件已到达自提点",
        "您好，您的快件 SF1428 5521 0937 已到达【科技园北区丰巢柜】，取件码 6127，请于 48 小时内取件。",
    ),
    (
        ("Daniel Okafor", "daniel.okafor@gmail.com"),
        "Saturday hike?",
        "Weather looks great for Saturday. Thinking the coastal trail, start around 8. You in?\n\nD.",
    ),
];

struct Seed {
    account: usize,
    folder: FolderKind,
    ago: f64,
    from: (&'static str, &'static str),
    subject: &'static str,
    text: &'static str,
    html: Option<&'static str>,
    seen: bool,
    flagged: bool,
    attachments: &'static [(&'static str, &'static str, f64)],
    remote_images: &'static [&'static str],
    /// Seeds with the same tag are one conversation, each replying to the one before.
    thread: Option<&'static str>,
}

const fn seed_msg(
    account: usize,
    ago: f64,
    from: (&'static str, &'static str),
    subject: &'static str,
    text: &'static str,
) -> Seed {
    Seed {
        account,
        folder: FolderKind::Inbox,
        ago,
        from,
        subject,
        text,
        html: None,
        seen: true,
        flagged: false,
        attachments: &[],
        remote_images: &[],
        thread: None,
    }
}

const WORK: usize = 0;
const GMAIL: usize = 1;
const OUTLOOK: usize = 2;

const DESIGN_REVIEW_HTML: &str = r#"<h2>Design review: unified inbox</h2>
<p>Hi team,</p>
<p>Thanks for the comments on the <b>unified inbox</b> proposal. Summary of where we landed:</p>
<ul>
<li>Account colors stay as small marks next to the sender, never as row backgrounds.</li>
<li>Unread is shown by weight and a dot; flagged by an icon in its own lane.</li>
<li>Remote images are <i>blocked by default</i>; the reader offers to load them.</li>
</ul>
<p><img src="https://images.example.com/mockup-inbox.png" alt="Inbox mockup"></p>
<p>The full notes are in the <a href="https://github.com/markbang/nsh-mail/blob/main/docs/design.md">design doc</a>.</p>
<p>— Maya</p>"#;

const NEWSLETTER_HTML: &str = r#"<h1>This Week in Rust 571</h1>
<p>Hello and welcome to another issue of <i>This Week in Rust</i>!</p>
<h3>Updates from the Rust project</h3>
<ul>
<li><a href="https://blog.rust-lang.org/">Announcing Rust 1.91.0</a></li>
<li>Async closures are now stable in more positions</li>
</ul>
<h3>Crate of the week</h3>
<p>This week's crate is <code>ammonia</code>, a whitelist-based HTML sanitizer.</p>
<blockquote>Sanitize first, render second.</blockquote>
<p><img src="https://this-week-in-rust.org/tracking-pixel.gif" alt=""></p>"#;

// Laid out for 600px, as most commercial mail is: the reader scales it to a narrow pane.
const RECEIPT_HTML: &str = r#"<table width="100%" cellpadding="0" cellspacing="0" style="background:#f3f4f6"><tr><td align="center" style="padding:24px">
<table width="600" cellpadding="0" cellspacing="0" style="background:#ffffff;border:1px solid #e5e7eb">
<tr><td style="padding:20px 32px;background:#0b3d91;color:#ffffff;font-size:20px;font-weight:bold">Fastmail</td></tr>
<tr><td style="padding:32px"><h2 style="margin:0 0 16px">Your receipt from Fastmail</h2>
<table width="100%" cellpadding="8" cellspacing="0" style="border-collapse:collapse;font-size:14px">
<tr style="border-bottom:1px solid #e5e7eb"><th align="left">Item</th><th align="right">Amount</th></tr>
<tr><td>Standard plan, 12 months</td><td align="right">$60.00</td></tr>
<tr><td>Tax</td><td align="right">$0.00</td></tr>
<tr style="border-top:2px solid #111827"><td><b>Total</b></td><td align="right"><b>$60.00</b></td></tr>
</table>
<p style="margin:24px 0 0">Paid with Visa ending 4242 on 3 November.</p></td></tr>
<tr><td style="padding:16px 32px;font-size:12px;color:#6b7280">Fastmail Pty Ltd, Melbourne, Australia</td></tr>
</table></td></tr></table>"#;

const SEEDS: &[Seed] = &[
    Seed {
        seen: false,
        flagged: true,
        html: Some(DESIGN_REVIEW_HTML),
        remote_images: &["https://images.example.com/mockup-inbox.png"],
        ..seed_msg(
            WORK,
            12.0 * MINUTE,
            ("Maya Patel", "maya@northwind.dev"),
            "Design review notes: unified inbox",
            "Hi team,\n\nThanks for the comments on the unified inbox proposal. Summary of where we landed:\n\n- Account colors stay as small marks next to the sender, never as row backgrounds.\n- Unread is shown by weight and a dot; flagged by an icon in its own lane.\n- Remote images are blocked by default; the reader offers to load them.\n\nThe full notes are in the design doc.\n\n— Maya",
        )
    },
    Seed {
        seen: false,
        ..seed_msg(
            GMAIL,
            38.0 * MINUTE,
            ("Daniel Okafor", "daniel.okafor@gmail.com"),
            "Photos from the weekend",
            "Finally sorted the photos from Saturday. The one at the lighthouse came out great.\n\nI'll send the full album link tonight.",
        )
    },
    Seed {
        seen: false,
        attachments: &[("Q4-roadmap.pdf", "application/pdf", 482_113.0)],
        ..seed_msg(
            WORK,
            1.6 * HOUR,
            ("Jonas Weber", "jonas@northwind.dev"),
            "Q4 roadmap draft",
            "Attached is the first draft of the Q4 roadmap. The big items are offline search, push on Android and the server beta.\n\nComments by Friday please.\n\nJonas",
        )
    },
    Seed {
        seen: false,
        ..seed_msg(
            OUTLOOK,
            2.5 * HOUR,
            ("王芳", "wangfang@outlook.com"),
            "周五聚餐改到七点",
            "大家好，\n\n周五的聚餐改到晚上七点，地点不变，还是园区西门那家。到时候见！\n\n王芳",
        )
    },
    Seed {
        html: Some(NEWSLETTER_HTML),
        remote_images: &["https://this-week-in-rust.org/tracking-pixel.gif"],
        ..seed_msg(
            GMAIL,
            5.0 * HOUR,
            ("This Week in Rust", "newsletter@this-week-in-rust.org"),
            "This Week in Rust 571",
            "Hello and welcome to another issue of This Week in Rust!\n\nUpdates from the Rust project\n- Announcing Rust 1.91.0\n- Async closures are now stable in more positions\n\nCrate of the week: ammonia, a whitelist-based HTML sanitizer.",
        )
    },
    Seed {
        seen: false,
        ..seed_msg(
            WORK,
            7.0 * HOUR,
            ("GitHub", "notifications@github.com"),
            "[northwind/atlas] Review requested: feat(app): client UI",
            "@maya-patel requested your review on #3: feat(app): client UI.\n\n+2,140 −12 across 41 files.",
        )
    },
    Seed {
        flagged: true,
        ..seed_msg(
            OUTLOOK,
            22.0 * HOUR,
            ("China Merchants Bank", "creditcard@cmbchina.com"),
            "信用卡电子账单（11月）",
            "尊敬的陈琳女士：\n\n您 11 月的信用卡账单已出，本期应还金额 ¥3,284.50，最后还款日 12 月 5 日。",
        )
    },
    Seed {
        html: Some(RECEIPT_HTML),
        ..seed_msg(
            WORK,
            1.1 * DAY,
            ("Fastmail", "billing@fastmail.com"),
            "Your receipt from Fastmail",
            "Your receipt from Fastmail\n\nStandard plan, 12 months: $60.00\nTax: $0.00\nTotal: $60.00\n\nPaid with Visa ending 4242 on 3 November.",
        )
    },
    Seed {
        thread: Some("thanksgiving"),
        ..seed_msg(
            GMAIL,
            3.0 * DAY,
            ("Mom", "chen.meiling@163.com"),
            "Thanksgiving plans",
            "Are you two coming home for Thanksgiving this year? Your aunt is flying in on Wednesday.\n\nLove, Mom",
        )
    },
    Seed {
        thread: Some("thanksgiving"),
        ..seed_msg(
            GMAIL,
            1.4 * DAY,
            ("Mom", "chen.meiling@163.com"),
            "Re: Thanksgiving plans",
            "We'd love to have you both. Dad is already planning the menu. Let me know your train times when you book.\n\nLove, Mom",
        )
    },
    seed_msg(
        WORK,
        1.8 * DAY,
        ("Priya Raman", "priya@northwind.dev"),
        "Retro moved to Thursday",
        "Heads up: the sprint retro moves to Thursday 3pm because of the all-hands. Same room.",
    ),
    Seed {
        attachments: &[(
            "boarding-pass.pkpass",
            "application/vnd.apple.pkpass",
            61_440.0,
        )],
        ..seed_msg(
            GMAIL,
            2.2 * DAY,
            ("United Airlines", "receipts@united.com"),
            "Your trip to Chicago: check in now",
            "Your flight UA 1872 from SFO to ORD departs Thursday at 07:15. Check in now to get your boarding pass.",
        )
    },
    seed_msg(
        OUTLOOK,
        2.9 * DAY,
        ("LinkedIn", "messages-noreply@linkedin.com"),
        "You appeared in 14 searches this week",
        "See who's looking at your profile and what they searched for.",
    ),
    seed_msg(
        WORK,
        3.4 * DAY,
        ("Sentry", "noreply@sentry.io"),
        "New issue: TypeError in reader.tsx",
        "TypeError: Cannot read properties of undefined (reading 'html')\n\nreader.tsx in renderBody at line 88\n\n12 events, 3 users",
    ),
    seed_msg(
        GMAIL,
        4.1 * DAY,
        ("Daniel Okafor", "daniel.okafor@gmail.com"),
        "Book recommendation",
        "You have to read 'The Overstory'. Took me a month but worth every page.",
    ),
    seed_msg(
        OUTLOOK,
        5.0 * DAY,
        (
            "Microsoft account team",
            "account-security-noreply@accountprotection.microsoft.com",
        ),
        "New sign-in to your account",
        "We noticed a new sign-in to your Microsoft account from Linux, Chrome.\n\nIf this was you, you can ignore this message.",
    ),
    seed_msg(
        WORK,
        6.2 * DAY,
        ("Maya Patel", "maya@northwind.dev"),
        "Offsite agenda",
        "Day 1: roadmap and planning.\nDay 2: hack day, demos at 4pm.\n\nBring a laptop charger, the venue has few outlets.",
    ),
    seed_msg(
        GMAIL,
        7.5 * DAY,
        ("Spotify", "no-reply@spotify.com"),
        "Your weekly mixtape is ready",
        "Discover Weekly: 30 new songs picked for you.",
    ),
    seed_msg(
        OUTLOOK,
        9.0 * DAY,
        ("知乎", "noreply@zhihu.com"),
        "你关注的问题有了新回答",
        "「Rust 适合写桌面应用吗？」有 3 个新回答。",
    ),
    // Older mail, so the list has more than a page.
    seed_msg(
        WORK,
        10.0 * DAY,
        ("Jonas Weber", "jonas@northwind.dev"),
        "Weekly sync notes",
        "Notes from this week's sync are in the shared folder.",
    ),
    seed_msg(
        GMAIL,
        11.0 * DAY,
        ("Airbnb", "automated@airbnb.com"),
        "Reminder: leave a review",
        "How was your stay in Lisbon? Your review helps other travellers.",
    ),
    seed_msg(
        WORK,
        12.0 * DAY,
        ("GitHub", "notifications@github.com"),
        "[northwind/atlas] Issue #1 opened: design doc",
        "jonas-weber opened issue #1: design doc.",
    ),
    seed_msg(
        OUTLOOK,
        13.0 * DAY,
        ("王芳", "wangfang@outlook.com"),
        "下周的项目会议",
        "下周二上午十点开项目会议，请提前看一下需求文档。",
    ),
    seed_msg(
        GMAIL,
        14.0 * DAY,
        ("Mom", "chen.meiling@163.com"),
        "Recipe you asked for",
        "Here's grandma's dumpling recipe. The secret is the ginger water.",
    ),
    seed_msg(
        WORK,
        15.0 * DAY,
        ("Priya Raman", "priya@northwind.dev"),
        "Welcome to the team!",
        "Welcome aboard, Lin! Your laptop arrives Monday. Let me know if you need anything.",
    ),
    seed_msg(
        GMAIL,
        16.0 * DAY,
        ("Google", "no-reply@accounts.google.com"),
        "Security alert",
        "A new app password was created for your Google Account.",
    ),
    seed_msg(
        OUTLOOK,
        17.0 * DAY,
        ("Booking.com", "noreply@booking.com"),
        "Confirmation: Hotel Lux Lisboa",
        "Your booking is confirmed: 3 nights, 12–15 October.",
    ),
    seed_msg(
        WORK,
        18.0 * DAY,
        ("Sentry", "noreply@sentry.io"),
        "Weekly report",
        "Errors this week: 41 (−23%). Top issue: timeout in sync worker.",
    ),
    seed_msg(
        GMAIL,
        19.0 * DAY,
        ("Daniel Okafor", "daniel.okafor@gmail.com"),
        "Climbing gym",
        "New routes went up at the gym. Tuesday?",
    ),
    seed_msg(
        OUTLOOK,
        20.0 * DAY,
        ("Apple", "no_reply@email.apple.com"),
        "Your receipt from Apple",
        "iCloud+ 50 GB: ¥6.00",
    ),
    seed_msg(
        WORK,
        21.0 * DAY,
        ("Jonas Weber", "jonas@northwind.dev"),
        "Interview loop",
        "Can you join the interview loop on Wednesday for the backend role?",
    ),
    seed_msg(
        GMAIL,
        22.0 * DAY,
        ("Strava", "no-reply@strava.com"),
        "Your October recap",
        "You ran 84 km in October. Nice work!",
    ),
    seed_msg(
        OUTLOOK,
        23.0 * DAY,
        ("王芳", "wangfang@outlook.com"),
        "照片",
        "上次团建的照片我放到群相册了。",
    ),
    seed_msg(
        WORK,
        24.0 * DAY,
        ("Maya Patel", "maya@northwind.dev"),
        "Docs style guide",
        "Sharing the docs style guide we agreed on. Sentence case everywhere, please.",
    ),
    seed_msg(
        GMAIL,
        25.0 * DAY,
        ("Kickstarter", "no-reply@kickstarter.com"),
        "A project you backed has an update",
        "Update #12: Shipping starts next month.",
    ),
    seed_msg(
        WORK,
        26.0 * DAY,
        ("Priya Raman", "priya@northwind.dev"),
        "Laptop setup",
        "Your laptop is ready for pickup at the front desk.",
    ),
    // A conversation: Priya asks, Lin answers from Sent, Jonas and Maya follow.
    Seed {
        thread: Some("offsite"),
        ..seed_msg(
            WORK,
            2.0 * DAY,
            ("Priya Raman", "priya@northwind.dev"),
            "Offsite in March: Lisbon or Porto?",
            "Hi all,\n\nWe have budget for a three-day offsite in March. Lisbon or Porto? Vote by Friday and I'll book.\n\nPriya",
        )
    },
    Seed {
        folder: FolderKind::Sent,
        thread: Some("offsite"),
        ..seed_msg(
            WORK,
            1.9 * DAY,
            ("Lin Chen", "lin.chen@northwind.dev"),
            "Re: Offsite in March: Lisbon or Porto?",
            "Porto gets my vote: easier flights for the Berlin folks, and the venue near the river has a big room for the hack day.\n\nLin",
        )
    },
    Seed {
        thread: Some("offsite"),
        ..seed_msg(
            WORK,
            1.2 * DAY,
            ("Jonas Weber", "jonas@northwind.dev"),
            "Re: Offsite in March: Lisbon or Porto?",
            "Porto works for me too. I can look at train times from Lisbon airport for anyone flying in there.\n\nJonas",
        )
    },
    Seed {
        seen: false,
        thread: Some("offsite"),
        attachments: &[("offsite-agenda.pdf", "application/pdf", 96_512.0)],
        ..seed_msg(
            WORK,
            4.0 * HOUR,
            ("Maya Patel", "maya@northwind.dev"),
            "Re: Offsite in March: Lisbon or Porto?",
            "Porto it is. I booked the river venue for 12–14 March; the agenda draft is attached. Add your session ideas by next Wednesday.\n\nMaya",
        )
    },
    // Other folders.
    Seed {
        folder: FolderKind::Sent,
        ..seed_msg(
            WORK,
            3.0 * HOUR,
            ("Lin Chen", "lin.chen@northwind.dev"),
            "Re: Q4 roadmap draft",
            "Looks good overall. I'd move offline search before the server beta: it helps both modes.\n\nLin",
        )
    },
    Seed {
        folder: FolderKind::Sent,
        thread: Some("thanksgiving"),
        ..seed_msg(
            GMAIL,
            1.5 * DAY,
            ("Lin Chen", "linchen.mail@gmail.com"),
            "Re: Thanksgiving plans",
            "We'll take the 10:05 train and arrive around 2. Can't wait!",
        )
    },
    Seed {
        folder: FolderKind::Drafts,
        ..seed_msg(
            WORK,
            4.0 * HOUR,
            ("Lin Chen", "lin.chen@northwind.dev"),
            "Notes for the offsite",
            "Ideas for the hack day:\n- Offline search\n- Keyboard shortcuts help",
        )
    },
    Seed {
        folder: FolderKind::Archive,
        ..seed_msg(
            WORK,
            30.0 * DAY,
            ("HR", "hr@northwind.dev"),
            "Your benefits enrollment",
            "Your benefits enrollment is complete.",
        )
    },
    Seed {
        folder: FolderKind::Archive,
        ..seed_msg(
            GMAIL,
            40.0 * DAY,
            ("Daniel Okafor", "daniel.okafor@gmail.com"),
            "Trip photos",
            "Here are the photos from Kyoto.",
        )
    },
    Seed {
        folder: FolderKind::Trash,
        ..seed_msg(
            OUTLOOK,
            6.0 * DAY,
            ("Promo", "deals@shop.example"),
            "48 hours only: 60% off",
            "Our biggest sale of the year ends soon.",
        )
    },
];

/// The sources the demo's feed comes from, besides the mail accounts.
const SOURCE_SEEDS: &[(&str, SourceKind, &str, &str, Option<&str>, bool, u8)] = &[
    (
        "src-github",
        SourceKind::Webhook,
        "GitHub",
        "chart-1",
        Some("bolt"),
        true,
        0,
    ),
    (
        "src-kuma",
        SourceKind::Webhook,
        "Uptime Kuma",
        "chart-2",
        Some("activity"),
        false,
        0,
    ),
    (
        "src-sonarr",
        SourceKind::Gotify,
        "Sonarr",
        "chart-5",
        None,
        true,
        2,
    ),
    (
        "src-backup",
        SourceKind::Ntfy,
        "Backup script",
        "chart-4",
        Some("terminal"),
        true,
        0,
    ),
    (
        "src-telegram",
        SourceKind::Telegram,
        "Team bot",
        "chart-3",
        None,
        true,
        0,
    ),
    (
        "src-slack",
        SourceKind::Slack,
        "Northwind HQ",
        "chart-1",
        None,
        true,
        3,
    ),
    (
        "src-discord",
        SourceKind::Discord,
        "Game night",
        "chart-2",
        None,
        true,
        0,
    ),
];

/// Notifications in the feed besides mail's.
const NOTICE_SEEDS: &[(&str, f64, &str, &str, &str, u8, &[&str], bool, Option<&str>)] = &[
    (
        "src-github",
        4.0 * MINUTE,
        "northwind/atlas",
        "CI failed on main",
        "Workflow Linux: 3 checks failed.\n\nerror[E0433]: failed to resolve: use of undeclared crate or module `nsh`\nin app/src/notify.rs:12:5",
        7,
        &["ci", "main"],
        false,
        Some("https://github.com/northwind/atlas/actions"),
    ),
    (
        "src-github",
        23.0 * MINUTE,
        "maya-patel",
        "Review requested: #56 feat(app): notification feed",
        "@maya-patel requested your review on #56: notification feed, sources and API keys.\n\n+2,140 −12 across 41 files.",
        5,
        &["review"],
        false,
        Some("https://github.com/northwind/atlas/pull/56"),
    ),
    (
        "src-sonarr",
        52.0 * MINUTE,
        "Sonarr",
        "Downloaded: Slow Horses S04E02",
        "1080p WEB-DL · 2.1 GB · ready in Plex.",
        3,
        &["download"],
        true,
        None,
    ),
    (
        "src-slack",
        1.1 * HOUR,
        "Priya Raman",
        "Priya mentioned you in #app",
        "can you look at the FTS index before the release? the second table is missing from the migration",
        5,
        &["mention"],
        false,
        None,
    ),
    (
        "src-telegram",
        1.4 * HOUR,
        "Maya Patel",
        "The venue confirmed",
        "The river venue confirmed 12–14 March. I'll send the contract tomorrow morning.",
        4,
        &["team"],
        false,
        None,
    ),
    (
        "src-backup",
        3.2 * HOUR,
        "Backup script",
        "Backup finished",
        "snapshot-2026-11-14 (42 GB) uploaded to B2 in 6m 12s. 30 snapshots kept.",
        2,
        &["backup", "nightly"],
        true,
        None,
    ),
    (
        "src-github",
        5.0 * HOUR,
        "northwind/atlas",
        "Deploy failed: production",
        "Deploy nsh-mail-server to mail-api.bangwu.me failed at the health check.\n\nThe container restarted 3 times in 90s.",
        9,
        &["deploy", "production"],
        false,
        Some("https://github.com/northwind/atlas/actions"),
    ),
    (
        "src-discord",
        6.5 * HOUR,
        "Daniel Okafor",
        "Saturday raid",
        "We're four short for Saturday, still in? Same time as last week.",
        2,
        &["game-night"],
        true,
        None,
    ),
    (
        "src-github",
        1.4 * DAY,
        "northwind/atlas",
        "Release v0.0.3 published",
        "Notes: keyboard and screen-reader pass, empty and failure states, paged sync.",
        4,
        &["release"],
        true,
        Some("https://github.com/northwind/atlas/releases/tag/v0.0.3"),
    ),
    (
        "src-backup",
        2.4 * DAY,
        "nas",
        "Disk almost full",
        "/volume1 is 91% full: 8.2 TB of 9 TB. The oldest snapshots go first, then the mirror waits.",
        8,
        &["warning"],
        false,
        None,
    ),
    (
        "src-kuma",
        2.6 * DAY,
        "Uptime Kuma",
        "api.bangwu.me is up",
        "Response 200 in 214 ms, after 3 minutes down.",
        1,
        &["uptime"],
        true,
        None,
    ),
    (
        "src-sonarr",
        3.1 * DAY,
        "Sonarr",
        "Downloaded: Dune Part Two",
        "2160p HDR · 24.8 GB · ready in Plex.",
        2,
        &["download"],
        true,
        None,
    ),
    (
        "src-telegram",
        3.4 * DAY,
        "Jonas Weber",
        "Standup notes",
        "Porto booked, the agenda is in the shared folder. Add your session ideas by Wednesday.",
        3,
        &["team"],
        true,
        None,
    ),
    (
        "src-slack",
        4.2 * DAY,
        "Northwind HQ",
        "Weekly digest: 12 threads you follow",
        "#app: 404 is retried twice before failing; #design: the empty states shipped.",
        1,
        &["digest"],
        true,
        None,
    ),
];

/// What refresh delivers after the first few: a source writing to the feed.
const NOTICES: &[(&str, &str, &str, &str, u8, &[&str])] = &[
    (
        "src-github",
        "northwind/atlas",
        "CI passed on main",
        "All checks have passed on main: core, server, linux, windows, web.",
        3,
        &["ci"],
    ),
    (
        "src-backup",
        "Backup script",
        "Backup started",
        "snapshot-2026-11-14 at 03:00, 41 GB so far.",
        2,
        &["backup"],
    ),
    (
        "src-slack",
        "Maya Patel",
        "Maya mentioned you in #design",
        "the new source colours look right in dark mode, ship it",
        5,
        &["mention"],
    ),
    (
        "src-telegram",
        "Team bot",
        "Deploy finished",
        "mail-api.bangwu.me runs v0.0.4, health check green.",
        4,
        &["deploy"],
    ),
];

/// The three demo accounts.
fn demo_accounts() -> Vec<Account> {
    let account = |id: &str,
                   email: &str,
                   name: &str,
                   provider: &str,
                   auth: Auth,
                   imap: &str,
                   smtp: (&str, u16, Security)| Account {
        id: id.into(),
        email: email.into(),
        name: Some(name.into()),
        provider: Some(provider.into()),
        username: email.into(),
        auth,
        imap: endpoint(imap, 993, Security::Tls),
        smtp: endpoint(smtp.0, smtp.1, smtp.2),
        signature: Signature::default(),
    };
    vec![
        account(
            "a-work",
            "lin.chen@northwind.dev",
            "Lin Chen",
            "fastmail",
            Auth::Password,
            "imap.fastmail.com",
            ("smtp.fastmail.com", 465, Security::Tls),
        ),
        account(
            "a-gmail",
            "linchen.mail@gmail.com",
            "Lin Chen",
            "gmail",
            oauth(OAuthProvider::Google),
            "imap.gmail.com",
            ("smtp.gmail.com", 465, Security::Tls),
        ),
        account(
            "a-outlook",
            "chenlin@outlook.com",
            "陈琳",
            "outlook",
            oauth(OAuthProvider::Microsoft),
            "outlook.office365.com",
            ("smtp.office365.com", 587, Security::StartTls),
        ),
    ]
}

/// A source for each demo account: the feed's mail side.
fn seed_mail_sources(data: &mut Data, now: f64) {
    for (account, name, color) in [
        ("a-work", "Fastmail", "chart-1"),
        ("a-gmail", "Gmail", "chart-2"),
        ("a-outlook", "Outlook", "chart-3"),
    ] {
        data.sources.push(Source {
            id: format!("src-mail-{account}"),
            kind: SourceKind::Mail,
            name: name.to_owned(),
            icon: None,
            color: Some(color.to_owned()),
            enabled: true,
            min_priority: 0,
            created_at: (now - 60.0 * DAY) as i64,
        });
    }
}

fn seed(now: f64) -> Data {
    let accounts = demo_accounts();
    let mut data = Data {
        accounts,
        ..Data::default()
    };
    for s in SEEDS {
        let account = data.accounts[s.account].clone();
        let id = data.id();
        let mut m = message(
            id,
            &account,
            s.folder,
            now - s.ago,
            s.from,
            s.subject,
            s.text,
        );
        if matches!(s.folder, FolderKind::Sent | FolderKind::Drafts) {
            m.summary.to = vec![Address {
                name: None,
                email: "team@northwind.dev".into(),
            }];
        }
        m.html = s.html.map(str::to_owned);
        m.remote_images = s.remote_images.iter().map(|u| (*u).to_owned()).collect();
        m.summary.seen = s.seen;
        m.summary.flagged = s.flagged;
        m.attachments = s
            .attachments
            .iter()
            .enumerate()
            .map(|(index, (name, mime, size))| Attachment {
                index: index as u32,
                filename: Some((*name).into()),
                mime_type: (*mime).into(),
                size: *size as u64,
                inline: false,
                content_id: None,
            })
            .collect();
        m.summary.has_attachments = !m.attachments.is_empty();
        if let Some(tag) = s.thread {
            m.summary.thread_id = format!("t-{tag}");
            let parent = data
                .messages
                .iter()
                .filter(|p| {
                    p.summary.thread_id == m.summary.thread_id && p.summary.date < m.summary.date
                })
                .max_by_key(|p| p.summary.date);
            if let Some(parent) = parent {
                m.in_reply_to = parent.message_id.clone();
                m.references = parent.references.clone();
                m.references.extend(parent.message_id.clone());
            }
        }
        data.messages.push(m);
    }

    seed_mail_sources(&mut data, now);
    for (n, (id, kind, name, color, icon, enabled, min_priority)) in SOURCE_SEEDS.iter().enumerate()
    {
        data.sources.push(Source {
            id: (*id).to_owned(),
            kind: *kind,
            name: (*name).to_owned(),
            icon: icon.map(str::to_owned),
            color: Some((*color).to_owned()),
            enabled: *enabled,
            min_priority: *min_priority,
            created_at: (now - (30.0 - n as f64) * DAY) as i64,
        });
    }

    let projected: Vec<Message> = data.messages.clone();
    for m in &projected {
        data.project(m);
    }

    for (source, ago, sender, title, body, priority, tags, read, link) in NOTICE_SEEDS {
        let n = Notification {
            id: notif_id(&mut data),
            source_id: (*source).to_owned(),
            sender: (*sender).to_owned(),
            title: (*title).to_owned(),
            body: (*body).to_owned(),
            link: link.map(str::to_owned),
            received_at: (now - ago) as i64,
            priority: *priority,
            tags: tags.iter().map(|t| (*t).to_owned()).collect(),
            read: *read,
            archived: false,
            thread_id: None,
        };
        data.feed.push(n);
    }

    data.keys = vec![
        ApiKey {
            id: "key-1".to_owned(),
            name: "Claude Code".to_owned(),
            prefix: "nb_7c41a9e2".to_owned(),
            read: true,
            write: true,
            source_ids: Vec::new(),
            created_at: (now - 22.0 * DAY) as i64,
            last_used_at: Some((now - 12.0 * MINUTE) as i64),
            expires_at: None,
        },
        ApiKey {
            id: "key-2".to_owned(),
            name: "Home assistant".to_owned(),
            prefix: "nb_1f8b30dd".to_owned(),
            read: true,
            write: false,
            source_ids: vec!["src-backup".to_owned(), "src-github".to_owned()],
            created_at: (now - 9.0 * DAY) as i64,
            last_used_at: Some((now - 2.0 * DAY) as i64),
            expires_at: Some((now + 21.0 * DAY) as i64),
        },
    ];
    data
}

// The large demo mailbox: a mailbox big enough that what a screen renders per update shows.
const LARGE_MESSAGES: usize = 9_600;
const LARGE_THREADS: usize = 2_000;
/// The conversation opened to check the reader: 200 messages.
const LARGE_THREAD: usize = 3;
const LARGE_THREAD_SIZE: usize = 200;

/// 10,000 messages in 2,000 conversations over the three accounts, one conversation of 200
/// messages, the rest of a few each. Deterministic, so measurements compare run to run.
fn seed_large(now: f64) -> Data {
    let mut data = Data {
        accounts: demo_accounts(),
        ..Data::default()
    };
    let people = LARGE_PEOPLE;
    let subjects = LARGE_SUBJECTS;
    let mut rng = Large(0x5eed_cafe);
    let mut counts: Vec<usize> = (0..LARGE_THREADS)
        .map(|i| {
            if i == LARGE_THREAD {
                LARGE_THREAD_SIZE
            } else {
                1 + rng.below(8) as usize
            }
        })
        .collect();
    let sum: usize = counts.iter().sum();
    if sum < LARGE_MESSAGES {
        let last = LARGE_THREADS - 1;
        counts[last] += LARGE_MESSAGES - sum;
    }
    for (i, count) in counts.iter().copied().enumerate() {
        // A conversation's latest message six minutes after the one below it.
        let latest = now - i as f64 * 6.0 * MINUTE;
        let subject = format!(
            "{} #{i}",
            subjects[rng.below(subjects.len() as u64) as usize]
        );
        let long = i == LARGE_THREAD;
        let mut parent: Option<String> = None;
        for j in 0..count {
            let account = data.accounts[(i + j) % data.accounts.len()].clone();
            let (name, email) = people[rng.below(people.len() as u64) as usize];
            let date = latest - (count - 1 - j) as f64 * 2.0 * MINUTE;
            let text = if long {
                format!(
                    "Message {} of 200 in this conversation.\n\nThe plan is in the attachment; the \
                     numbers moved a little since the last review, and the rest is unchanged.\n\n
                     Say if the order works for you and I will book the room.",
                    j + 1
                )
            } else {
                format!(
                    "{}
\nKind regards,\n{name}",
                    SUBJECT_BODY[rng.below(SUBJECT_BODY.len() as u64) as usize]
                )
            };
            let id = data.id();
            let subject = if j == 0 {
                subject.clone()
            } else {
                format!("Re: {subject}")
            };
            let mut m = message(
                id,
                &account,
                FolderKind::Inbox,
                date,
                (name, email),
                &subject,
                &text,
            );
            m.summary.thread_id = format!("t-large-{i}");
            m.summary.seen = long && j + 1 < count || rng.below(100) >= 18;
            m.summary.flagged = rng.below(100) < 2;
            if rng.below(100) < 5 {
                m.summary.has_attachments = true;
                m.attachments = vec![Attachment {
                    index: 0,
                    filename: Some("agenda.pdf".into()),
                    mime_type: "application/pdf".into(),
                    size: 184_320,
                    inline: false,
                    content_id: None,
                }];
            }
            if rng.below(100) < 10 {
                m.html = Some(format!("<p>{text}</p>"));
            }
            if let Some(parent) = &parent {
                m.in_reply_to = Some(parent.clone());
                m.references = vec![parent.clone()];
            }
            parent = m.message_id.clone();
            data.messages.push(m);
        }
    }
    // The feed has its sources; this demo's thousands of messages are not projected into it.
    seed_mail_sources(&mut data, now);
    // Sent, Archive, Drafts and Trash hold the rest: 10,000 messages in all.
    for (folder, count) in [
        (FolderKind::Sent, 200),
        (FolderKind::Archive, 150),
        (FolderKind::Drafts, 20),
        (FolderKind::Trash, 30),
    ] {
        for i in 0..count {
            let account = data.accounts[i % data.accounts.len()].clone();
            let (name, email) = people[i % people.len()];
            let id = data.id();
            let mut m = message(
                id,
                &account,
                folder,
                now - (LARGE_THREADS + i) as f64 * 3.0 * MINUTE,
                (name, email),
                &format!("{} #sent{i}", subjects[i % subjects.len()]),
                SUBJECT_BODY[i % SUBJECT_BODY.len()],
            );
            m.summary.seen = true;
            if matches!(folder, FolderKind::Sent | FolderKind::Drafts) {
                m.summary.to = vec![Address {
                    name: Some(name.into()),
                    email: email.into(),
                }];
            }
            data.messages.push(m);
        }
    }
    data
}

/// A tiny xorshift: the large mailbox repeats from run to run.
struct Large(u64);

impl Large {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n.max(1)
    }
}

const LARGE_PEOPLE: [(&str, &str); 24] = [
    ("Priya Raman", "priya@northwind.dev"),
    ("Jonas Weber", "jonas@northwind.dev"),
    ("Maya Patel", "maya@northwind.dev"),
    ("Tobias Falk", "tobias@northwind.dev"),
    ("Ana Albers", "ana@lumen.example"),
    ("Bo Brandt", "bo@lumen.example"),
    ("Chen Yu", "chen.yu@lumen.example"),
    ("Dana Dorn", "dana@lumen.example"),
    ("Emil Ebert", "emil@lumen.example"),
    ("Fatima Gross", "fatima@lumen.example"),
    ("Gao Lei", "gao.lei@lumen.example"),
    ("Hana Heller", "hana@lumen.example"),
    ("Ivo Ibarra", "ivo@lumen.example"),
    ("Jun Jansen", "jun@lumen.example"),
    ("Kai Keller", "kai@lumen.example"),
    ("Lena Lorenz", "lena@lumen.example"),
    ("Mira Mertens", "mira@lumen.example"),
    ("Nils Novak", "nils@lumen.example"),
    ("Omar Ortiz", "omar@lumen.example"),
    ("Pia Peters", "pia@lumen.example"),
    ("Ravi Quirin", "ravi@lumen.example"),
    ("Sofia Ritter", "sofia@lumen.example"),
    ("Tomas Sauer", "tomas@lumen.example"),
    ("Wen Zhou", "wen.zhou@lumen.example"),
];

const LARGE_SUBJECTS: [&str; 12] = [
    "Weekly sync notes",
    "Invoice for October",
    "Deployment window",
    "Lunch on Thursday?",
    "New design review",
    "Quarterly report draft",
    "Access request",
    "Release changelog",
    "Travel booking",
    "Reminder: timesheet",
    "项目周报",
    "会议纪要",
];

const SUBJECT_BODY: [&str; 8] = [
    "Short update: everything is on track, the numbers are in the doc.",
    "Attaching the notes from this morning; tell me what I missed.",
    "Can we move this to the afternoon? The morning is full.",
    "The invoice is paid, no action needed.",
    "Deployed to staging, the window is clear for tomorrow.",
    "Review is scheduled for Thursday; the deck is nearly done.",
    "Thanks for the quick turnaround, that closed it.",
    "预算已经确认，附件是明细。",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_parse_with_and_without_names() {
        assert_eq!(
            parse_address("Maya <maya@northwind.dev>")
                .unwrap()
                .name
                .as_deref(),
            Some("Maya")
        );
        assert_eq!(parse_address(" a@b.co ").unwrap().email, "a@b.co");
        assert!(parse_address("not an address").is_none());
        assert!(parse_address("a@localhost").is_none());
    }
}
