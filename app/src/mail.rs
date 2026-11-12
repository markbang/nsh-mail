//! `Mail`, the one interface TSX uses (docs/design.md §3). It owns the
//! session: which backend runs, its status, and the listeners of its events.

use std::{cell::RefCell, rc::Rc};

use nsh_core::{prelude::*, storage::Storage};

use crate::{
    backend::{Backend, MailError},
    demo::DemoBackend,
    types::*,
};

const MODE_KEY: &str = "nshmail.mode";
const SERVER_KEY: &str = "nshmail.server";

#[derive(Default)]
struct Session {
    backend: Option<Rc<dyn Backend>>,
    status: Status,
    listeners: Vec<(u64, Callback<MailEvent>)>,
    next_listener: u64,
    started: bool,
}

thread_local! {
    // Calls from TSX and backend tasks all run on the engine thread.
    static SESSION: RefCell<Session> = RefCell::default();
}

fn with<R>(f: impl FnOnce(&mut Session) -> R) -> R {
    SESSION.with(|s| f(&mut s.borrow_mut()))
}

fn backend() -> Result<Rc<dyn Backend>, MailError> {
    with(|s| s.backend.clone())
        .ok_or_else(|| MailError::new(MailError::NO_SESSION, "choose local or cloud mode first"))
}

fn emit(event: MailEvent) {
    let listeners = with(|s| {
        match event.r#type {
            MailEventType::SyncState => {
                s.status.syncing = event.syncing.unwrap_or(false);
                s.status.error = event.error.clone();
            }
            MailEventType::Error => s.status.error = event.error.clone(),
            _ => {}
        }
        s.listeners
            .iter()
            .map(|(_, l)| l.clone())
            .collect::<Vec<_>>()
    });
    for listener in listeners {
        listener.post(event.clone());
    }
}

/// Opens a session for `mode`; the backend is the demo until the real
/// ones land (docs/design.md §3).
async fn open(mode: Mode, server: Option<String>) -> Result<(), MailError> {
    let backend: Rc<dyn Backend> = Rc::new(DemoBackend::default());
    with(|s| {
        s.backend = Some(backend.clone());
        s.status = Status {
            mode,
            server,
            syncing: false,
            error: None,
        };
    });
    backend.start(Rc::new(emit)).await?;
    emit(MailEvent::changed(None, None));
    Ok(())
}

fn remember(mode: Mode, server: Option<&str>) {
    let mode = match mode {
        Mode::Local => "local",
        Mode::Cloud => "cloud",
        Mode::None => {
            Storage::remove_item(MODE_KEY);
            Storage::remove_item(SERVER_KEY);
            return;
        }
    };
    // A full storage only costs the remembered mode: onboarding shows again.
    let _ = Storage::set_item(MODE_KEY, mode);
    match server {
        Some(server) => drop(Storage::set_item(SERVER_KEY, server)),
        None => Storage::remove_item(SERVER_KEY),
    }
}

fn valid_server(server: &str) -> Result<String, MailError> {
    let server = server.trim().trim_end_matches('/');
    let local = server.starts_with("http://localhost") || server.starts_with("http://127.0.0.1");
    if !(server.starts_with("https://") || local) || server.len() < "https://x".len() {
        return Err(MailError::invalid(
            "the server address starts with https://",
        ));
    }
    Ok(server.to_owned())
}

/// Mail from every account: the session, accounts, messages and their events.
#[module]
pub struct Mail;

#[module]
impl Mail {
    /// Restores the last session (if any) and starts syncing; resolves with
    /// the status. Calling it again returns the current status.
    pub async fn init() -> Result<Status, JsError> {
        if with(|s| std::mem::replace(&mut s.started, true)) {
            return Ok(Mail::status());
        }
        let mode = match Storage::get_item(MODE_KEY).as_deref() {
            Some("local") if cfg!(not(target_arch = "wasm32")) => Mode::Local,
            Some("cloud") => Mode::Cloud,
            _ => Mode::None,
        };
        if mode != Mode::None {
            open(mode, Storage::get_item(SERVER_KEY)).await?;
        }
        Ok(Mail::status())
    }

    /// The current status.
    pub fn status() -> Status {
        with(|s| s.status.clone())
    }

    /// Keeps mail on this device. Rejects with `ERR_UNSUPPORTED` on the web,
    /// which cannot reach mail servers.
    pub async fn use_local() -> Result<(), JsError> {
        if cfg!(target_arch = "wasm32") {
            return Err(MailError::new(
                MailError::UNSUPPORTED,
                "a browser cannot connect to mail servers: use an nsh-mail server",
            )
            .into());
        }
        open(Mode::Local, None).await?;
        remember(Mode::Local, None);
        Ok(())
    }

    /// Signs in to (or, with `sign_up`, creates an account on) the nsh-mail
    /// server at `server`, and syncs through it.
    pub async fn sign_in(
        server: String,
        email: String,
        password: String,
        sign_up: bool,
    ) -> Result<(), JsError> {
        let server = valid_server(&server)?;
        if !email.contains('@') {
            return Err(MailError::invalid("enter an email address").into());
        }
        if password.is_empty() || (sign_up && password.chars().count() < 8) {
            return Err(MailError::invalid("a password has at least 8 characters").into());
        }
        timer(std::time::Duration::from_millis(400)).await;
        open(Mode::Cloud, Some(server.clone())).await?;
        remember(Mode::Cloud, Some(&server));
        Ok(())
    }

    /// Ends the session: back to onboarding.
    pub async fn sign_out() -> Result<(), JsError> {
        if let Some(backend) = with(|s| s.backend.take()) {
            backend.sign_out().await?;
        }
        with(|s| s.status = Status::default());
        remember(Mode::None, None);
        emit(MailEvent::changed(None, None));
        Ok(())
    }

    /// The accounts, in the order they were added.
    pub async fn accounts() -> Result<Vec<Account>, JsError> {
        Ok(backend()?.accounts().await?)
    }

    /// The servers of `email`'s provider, `null` when unknown.
    pub async fn detect(email: String) -> Result<Option<ServerSettings>, JsError> {
        Ok(backend()?.detect(email).await?)
    }

    /// Adds an IMAP account after connecting to its IMAP and SMTP servers.
    pub async fn add_imap(input: ImapInput) -> Result<Account, JsError> {
        let account = backend()?.add_imap(input).await?;
        emit(MailEvent::changed(None, None));
        Ok(account)
    }

    /// Adds a Gmail or Outlook account through the provider's sign-in page.
    pub async fn add_o_auth(provider: OAuthProvider) -> Result<Account, JsError> {
        let account = backend()?.add_oauth(provider).await?;
        emit(MailEvent::changed(None, None));
        Ok(account)
    }

    /// Removes an account and its mail from this app.
    pub async fn remove_account(id: String) -> Result<(), JsError> {
        backend()?.remove_account(id).await?;
        emit(MailEvent::changed(None, None));
        Ok(())
    }

    /// A page of a folder, newest first.
    pub async fn list(query: ListQuery) -> Result<Vec<Summary>, JsError> {
        Ok(backend()?.list(query).await?)
    }

    /// A whole message.
    pub async fn get(id: String) -> Result<Message, JsError> {
        Ok(backend()?.get(id).await?)
    }

    /// Marks a message read or unread, flagged or not.
    pub async fn set_flags(id: String, flags: Flags) -> Result<(), JsError> {
        backend()?.set_flags(id, flags).await?;
        emit(MailEvent::changed(None, None));
        Ok(())
    }

    /// Sends a message.
    pub async fn send(draft: Draft) -> Result<(), JsError> {
        backend()?.send(draft).await?;
        emit(MailEvent::changed(None, None));
        Ok(())
    }

    /// Messages matching `text` in any folder, newest first.
    pub async fn search(text: String, limit: u32) -> Result<Vec<Summary>, JsError> {
        Ok(backend()?.search(text, limit).await?)
    }

    /// Syncs now.
    pub async fn refresh() -> Result<(), JsError> {
        Ok(backend()?.refresh().await?)
    }

    /// Calls `listener` with each event: `newMail`, `changed`, `syncState`,
    /// `error`. Returns a function that stops listening.
    pub fn on(#[args(event)] listener: Callback<MailEvent>) -> Stop {
        let id = with(|s| {
            s.next_listener += 1;
            s.listeners.push((s.next_listener, listener));
            s.next_listener
        });
        Stop::new(move || with(|s| s.listeners.retain(|(l, _)| *l != id)))
    }
}
