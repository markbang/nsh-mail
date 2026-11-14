//! `LocalBackend`: mail-core on this device (docs/design.md §4 local mode).
//! The database is `data_dir()/mail.sqlite`; passwords and tokens go to the
//! OS keyring, or an encrypted file beside the database where there is none
//! (always on Android). The engine's work runs on the tasks' runtime
//! (`background`), off the engine thread; its events come back here.

use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    rc::{Rc, Weak},
};

use mail_core::{
    Engine, StagedFile,
    secrets::{self, platform_store},
    types::*,
};
use nsh_core::prelude::*;
use tokio::sync::broadcast::error::RecvError;

use crate::backend::{Backend, Events, File, MailFuture, Progress, Staged, rename_upload};

type Slot = RefCell<Option<Engine>>;

#[derive(Default)]
pub struct LocalBackend {
    engine: Rc<Slot>,
    staged: Staged,
}

/// Runs `work` on the tasks' runtime: the engine needs tokio's.
async fn run<T: Send + 'static>(
    work: impl Future<Output = Result<T>> + Send + 'static,
) -> Result<T> {
    background(work)
        .await
        .map_err(|e| Error::new(ErrorCode::Internal, format!("mail task failed: {e}")))?
}

impl LocalBackend {
    fn engine(&self) -> Result<Engine> {
        self.engine
            .borrow()
            .clone()
            .ok_or_else(|| Error::new(ErrorCode::Internal, "the mail engine is not running"))
    }

    /// `f` with the engine, on the tasks' runtime.
    fn call<T, F>(&self, f: impl FnOnce(Engine) -> F) -> MailFuture<'_, T>
    where
        T: Send + 'static,
        F: Future<Output = Result<T>> + Send + 'static,
    {
        let work = self.engine().map(f);
        Box::pin(async move { run(work?).await })
    }
}

async fn open() -> Result<(Engine, secrets::Backend)> {
    let dir = data_dir().ok_or_else(|| {
        Error::new(
            ErrorCode::Unsupported,
            "this device keeps no files for the app: use an nsh-mail server",
        )
    })?;
    blocking(move || {
        std::fs::create_dir_all(&dir).map_err(|e| {
            Error::new(
                ErrorCode::Storage,
                format!("cannot create {}: {e}", dir.display()),
            )
        })?;
        let (secrets, kind) = platform_store(&dir)?;
        Ok((Engine::open(dir.join("mail.sqlite"), secrets)?, kind))
    })
    .await
    .map_err(|e| Error::new(ErrorCode::Internal, format!("opening mail failed: {e}")))?
}

/// Hands the engine's events to `events` on the engine thread until the
/// engine goes.
fn forward(engine: &Engine, slot: Weak<Slot>, events: Events) {
    let mut rx = engine.subscribe();
    Owner::detached().run(|| {
        spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => events(event),
                    Err(RecvError::Lagged(_)) => {
                        // Events were lost: every list reloads.
                        let Some(engine) = slot.upgrade().and_then(|s| s.borrow().clone()) else {
                            return;
                        };
                        let accounts = run(async move { engine.accounts().await }).await;
                        for a in accounts.unwrap_or_default() {
                            events(MailEvent::Changed {
                                account_id: a.id,
                                folder: FolderKind::Inbox,
                            });
                        }
                    }
                    Err(RecvError::Closed) => return,
                }
            }
        })
    });
}

impl LocalBackend {
    /// Copies of the files a draft's `upload`s name, for the engine.
    fn staged_files(&self, draft: &Draft) -> Result<HashMap<String, StagedFile>> {
        Ok(self
            .staged
            .of(draft)?
            .into_iter()
            .map(|(id, f)| {
                let file = StagedFile {
                    filename: f.name.clone(),
                    mime_type: f.mime_type.clone(),
                    data: f.data.clone(),
                };
                (id, file)
            })
            .collect())
    }
}

impl Backend for LocalBackend {
    fn start(&self, events: Events) -> MailFuture<'_, ()> {
        Box::pin(async move {
            let (engine, secrets) = open().await?;
            forward(&engine, Rc::downgrade(&self.engine), events.clone());
            let background_engine = engine.clone();
            run(async move { background_engine.run().await }).await?;
            *self.engine.borrow_mut() = Some(engine);
            if secrets == secrets::Backend::EncryptedFile && cfg!(not(target_os = "android")) {
                events(MailEvent::Error {
                    account_id: None,
                    error: Error::new(
                        ErrorCode::Secrets,
                        "no system keyring: passwords are kept encrypted in the app's folder",
                    ),
                });
            }
            Ok(())
        })
    }

    fn sign_out(&self) -> MailFuture<'_, ()> {
        Box::pin(async move {
            let engine = self.engine()?;
            engine.stop();
            run(async move {
                for a in engine.accounts().await? {
                    engine.remove_account(&a.id).await?;
                }
                Ok(())
            })
            .await?;
            self.engine.borrow_mut().take();
            Ok(())
        })
    }

    fn accounts(&self) -> MailFuture<'_, Vec<Account>> {
        self.call(|e| async move { e.accounts().await })
    }

    fn detect(&self, email: String) -> MailFuture<'_, Option<ServerSettings>> {
        // Without autoconfig the form still takes the servers by hand.
        self.call(|e| async move {
            match e.detect(&email).await {
                Err(err) if err.code == ErrorCode::Network => Ok(None),
                found => found,
            }
        })
    }

    fn add_imap(&self, input: ImapInput) -> MailFuture<'_, Account> {
        self.call(|e| async move { e.add_imap_account(input).await })
    }

    fn add_oauth(&self, provider: OAuthProvider) -> MailFuture<'_, Account> {
        Box::pin(async move {
            let engine = self.engine()?;
            let e = engine.clone();
            let sign_in = run(async move { e.start_oauth(provider, None).await }).await?;
            if !open_url(sign_in.url.clone()).await {
                return Err(Error::new(
                    ErrorCode::OAuth,
                    format!(
                        "could not open the browser; open {} to sign in",
                        sign_in.url
                    ),
                ));
            }
            run(async move {
                let grant = sign_in.finish().await?;
                engine.add_oauth_account(grant).await
            })
            .await
        })
    }

    fn remove_account(&self, id: String) -> MailFuture<'_, ()> {
        self.call(|e| async move { e.remove_account(&id).await })
    }

    fn set_signature(&self, id: String, signature: Signature) -> MailFuture<'_, Account> {
        self.call(|e| async move { e.set_signature(&id, signature).await })
    }

    fn list(&self, query: ListQuery) -> MailFuture<'_, Vec<Summary>> {
        self.call(|e| async move { e.list(query).await })
    }

    fn older(&self, folder: FolderKind, account: Option<String>) -> MailFuture<'_, Backfill> {
        self.call(|e| async move { e.backfill(account.as_deref(), folder).await })
    }

    fn threads(&self, query: ListQuery) -> MailFuture<'_, Vec<Thread>> {
        self.call(|e| async move { e.threads(query).await })
    }

    fn thread(&self, id: String) -> MailFuture<'_, Vec<Summary>> {
        self.call(|e| async move { e.thread(&id).await })
    }

    fn set_thread_flags(&self, id: String, flags: Flags) -> MailFuture<'_, ()> {
        self.call(|e| async move { e.set_thread_flags(&id, flags).await })
    }

    fn move_thread(&self, id: String, folder: FolderKind) -> MailFuture<'_, ()> {
        self.call(move |e| async move { e.move_thread(&id, folder).await })
    }

    fn get(&self, id: String) -> MailFuture<'_, Message> {
        self.call(|e| async move { e.get(&id).await })
    }

    fn attachment(&self, id: String, index: u32) -> MailFuture<'_, Vec<u8>> {
        self.call(move |e| async move { Ok(e.attachment(&id, index).await?.1) })
    }

    fn set_flags(&self, id: String, flags: Flags) -> MailFuture<'_, ()> {
        self.call(|e| async move { e.set_flags(&id, flags).await })
    }

    fn move_to(&self, id: String, folder: FolderKind) -> MailFuture<'_, ()> {
        self.call(move |e| async move { e.move_to(&id, folder).await })
    }

    fn send(&self, draft: Draft) -> MailFuture<'_, ()> {
        Box::pin(async move {
            let staged = self.staged_files(&draft)?;
            let engine = self.engine()?;
            let sending = draft.clone();
            run(async move { engine.send_with(sending, staged).await }).await?;
            self.staged.sent(&draft);
            Ok(())
        })
    }

    fn queue(&self, draft: Draft, when: SendAt) -> MailFuture<'_, Outgoing> {
        Box::pin(async move {
            let staged = self.staged_files(&draft)?;
            let engine = self.engine()?;
            let queued = draft.clone();
            let outgoing = run(async move { engine.queue(queued, staged, when).await }).await?;
            // The outbox keeps its own copy, in the database.
            self.staged.sent(&draft);
            Ok(outgoing)
        })
    }

    fn outbox(&self) -> MailFuture<'_, Vec<Outgoing>> {
        self.call(|e| async move { e.outbox().await })
    }

    fn cancel(&self, id: String) -> MailFuture<'_, Outgoing> {
        Box::pin(async move {
            let engine = self.engine()?;
            let (mut outgoing, files) = run(async move { engine.cancel(&id).await }).await?;
            for (id, f) in files {
                let file = File {
                    name: f.filename,
                    mime_type: f.mime_type,
                    data: f.data,
                };
                let staged = self.staged.restore(id.clone(), file);
                if staged != id {
                    rename_upload(&mut outgoing, &id, &staged);
                }
            }
            Ok(outgoing)
        })
    }

    fn stage(&self, file: File, progress: Progress) -> MailFuture<'_, Upload> {
        let size = file.data.len() as u64;
        let upload = self.staged.add(file);
        progress(size);
        Box::pin(async move { Ok(upload) })
    }

    fn unstage(&self, id: String) -> MailFuture<'_, ()> {
        self.staged.remove(&id);
        Box::pin(async { Ok(()) })
    }

    fn search(&self, text: String, limit: u32) -> MailFuture<'_, Vec<Summary>> {
        self.call(|e| async move { e.search(&text, limit).await })
    }

    fn unread(&self) -> MailFuture<'_, HashMap<AccountId, u32>> {
        self.call(|e| async move { e.unread().await })
    }

    fn refresh(&self) -> MailFuture<'_, ()> {
        self.call(|e| async move { e.sync_all().await })
    }
}

impl Drop for LocalBackend {
    fn drop(&mut self) {
        if let Some(engine) = self.engine.borrow_mut().take() {
            engine.stop();
        }
    }
}
