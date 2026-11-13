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
    Engine,
    secrets::{self, platform_store},
    types::*,
};
use nsh_core::prelude::*;
use tokio::sync::broadcast::error::RecvError;

use crate::backend::{Backend, Events, MailFuture};

type Slot = RefCell<Option<Engine>>;

#[derive(Default)]
pub struct LocalBackend {
    engine: Rc<Slot>,
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

    fn list(&self, query: ListQuery) -> MailFuture<'_, Vec<Summary>> {
        self.call(|e| async move { e.list(query).await })
    }

    fn get(&self, id: String) -> MailFuture<'_, Message> {
        self.call(|e| async move { e.get(&id).await })
    }

    fn set_flags(&self, id: String, flags: Flags) -> MailFuture<'_, ()> {
        self.call(|e| async move { e.set_flags(&id, flags).await })
    }

    fn send(&self, draft: Draft) -> MailFuture<'_, ()> {
        self.call(|e| async move { e.send(draft).await })
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
