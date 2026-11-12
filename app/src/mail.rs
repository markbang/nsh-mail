//! `Mail`, the one interface TSX uses (docs/design.md §3). It owns the
//! session: which backend runs, its status, and the listeners of its events.

use std::{cell::RefCell, collections::HashSet, rc::Rc};

use mail_core::types::*;
use nsh_core::{prelude::*, storage::Storage};

use crate::{
    backend::{Backend, invalid, js},
    types::Js,
};

const MODE_KEY: &str = "nshmail.mode";
const SERVER_KEY: &str = "nshmail.server";

struct Session {
    backend: Option<Rc<dyn Backend>>,
    status: Status,
    /// The accounts whose sync runs.
    syncing: HashSet<String>,
    listeners: Vec<(u64, Callback<Js<MailEvent>>)>,
    next_listener: u64,
    started: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            backend: None,
            status: no_session(),
            syncing: HashSet::new(),
            listeners: Vec::new(),
            next_listener: 0,
            started: false,
        }
    }
}

fn no_session() -> Status {
    Status {
        mode: Mode::None,
        server: None,
        syncing: false,
        error: None,
    }
}

thread_local! {
    // Calls from TSX and backend tasks all run on the engine thread.
    static SESSION: RefCell<Session> = RefCell::default();
}

fn with<R>(f: impl FnOnce(&mut Session) -> R) -> R {
    SESSION.with(|s| f(&mut s.borrow_mut()))
}

fn backend() -> Result<Rc<dyn Backend>, JsError> {
    with(|s| s.backend.clone()).ok_or_else(|| js(invalid("choose local or cloud mode first")))
}

fn emit(event: MailEvent) {
    let listeners = with(|s| {
        match &event {
            MailEvent::SyncState {
                account_id,
                syncing,
                error,
            } => {
                if *syncing {
                    s.syncing.insert(account_id.clone());
                } else {
                    s.syncing.remove(account_id);
                }
                s.status.syncing = !s.syncing.is_empty();
                s.status.error = error.clone();
            }
            MailEvent::Error {
                account_id: Some(_),
                error,
            } => s.status.error = Some(error.clone()),
            _ => {}
        }
        s.listeners
            .iter()
            .map(|(_, l)| l.clone())
            .collect::<Vec<_>>()
    });
    for listener in listeners {
        listener.post(Js(event.clone()));
    }
}

fn changed(account_id: String, folder: FolderKind) {
    emit(MailEvent::Changed { account_id, folder });
}

/// The backend of `mode`. The in-memory demo backs the UI tests
/// (`nshmail.mode` = `demo`) and cloud mode until `CloudBackend` lands;
/// release builds have no demo.
fn backend_for(mode: Mode, demo: bool) -> Result<Rc<dyn Backend>, Error> {
    #[cfg(debug_assertions)]
    if demo || mode == Mode::Cloud {
        return Ok(Rc::new(crate::demo::DemoBackend::default()));
    }
    let _ = demo;
    match mode {
        #[cfg(not(target_arch = "wasm32"))]
        Mode::Local => Ok(Rc::new(crate::local::LocalBackend::default())),
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "this mode is not available in this build",
        )),
    }
}

/// Opens a session for `mode`, ending the last one.
async fn open(mode: Mode, server: Option<String>, demo: bool) -> Result<(), JsError> {
    let backend = backend_for(mode, demo).map_err(js)?;
    with(|s| {
        s.backend = Some(backend.clone());
        s.syncing.clear();
        s.status = Status {
            mode,
            server,
            syncing: false,
            error: None,
        };
    });
    if let Err(e) = backend.start(Rc::new(emit)).await {
        with(|s| {
            s.backend = None;
            s.status = no_session();
        });
        return Err(js(e));
    }
    Ok(())
}

fn remember(mode: Option<&str>, server: Option<&str>) {
    let Some(mode) = mode else {
        Storage::remove_item(MODE_KEY);
        Storage::remove_item(SERVER_KEY);
        return;
    };
    // A full storage only costs the remembered mode: onboarding shows again.
    let _ = Storage::set_item(MODE_KEY, mode);
    match server {
        Some(server) => drop(Storage::set_item(SERVER_KEY, server)),
        None => Storage::remove_item(SERVER_KEY),
    }
}

fn valid_server(server: &str) -> Result<String, JsError> {
    let server = server.trim().trim_end_matches('/');
    let local = server.starts_with("http://localhost") || server.starts_with("http://127.0.0.1");
    if !(server.starts_with("https://") || local) || server.len() < "https://x".len() {
        return Err(js(invalid("the server address starts with https://")));
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
    pub async fn init() -> Result<Js<Status>, JsError> {
        if with(|s| std::mem::replace(&mut s.started, true)) {
            return Ok(Mail::status());
        }
        let server = Storage::get_item(SERVER_KEY);
        match Storage::get_item(MODE_KEY).as_deref() {
            Some("local") if cfg!(not(target_arch = "wasm32")) => {
                open(Mode::Local, None, false).await?
            }
            Some("demo") if cfg!(debug_assertions) => open(Mode::Local, None, true).await?,
            Some("cloud") => open(Mode::Cloud, server, false).await?,
            _ => {}
        }
        Ok(Mail::status())
    }

    /// The current status.
    pub fn status() -> Js<Status> {
        Js(with(|s| s.status.clone()))
    }

    /// Keeps mail on this device. Rejects with `ERR_UNSUPPORTED` on the web,
    /// which cannot reach mail servers.
    pub async fn use_local() -> Result<(), JsError> {
        if cfg!(target_arch = "wasm32") {
            return Err(js(Error::new(
                ErrorCode::Unsupported,
                "a browser cannot connect to mail servers: use an nsh-mail server",
            )));
        }
        open(Mode::Local, None, false).await?;
        remember(Some("local"), None);
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
            return Err(js(invalid("enter an email address")));
        }
        if password.is_empty() || (sign_up && password.chars().count() < 8) {
            return Err(js(invalid("a password has at least 8 characters")));
        }
        open(Mode::Cloud, Some(server.clone()), false).await?;
        remember(Some("cloud"), Some(&server));
        Ok(())
    }

    /// Ends the session: back to onboarding.
    pub async fn sign_out() -> Result<(), JsError> {
        if let Some(backend) = with(|s| s.backend.take()) {
            backend.sign_out().await.map_err(js)?;
        }
        with(|s| {
            s.status = no_session();
            s.syncing.clear();
        });
        remember(None, None);
        Ok(())
    }

    /// The accounts, in the order they were added.
    pub async fn accounts() -> Result<Js<Vec<Account>>, JsError> {
        backend()?.accounts().await.map(Js).map_err(js)
    }

    /// The servers of `email`'s provider, `null` when unknown.
    pub async fn detect(email: String) -> Result<Js<Option<ServerSettings>>, JsError> {
        backend()?.detect(email).await.map(Js).map_err(js)
    }

    /// Adds an IMAP account after connecting to its IMAP and SMTP servers.
    pub async fn add_imap(input: Js<ImapInput>) -> Result<Js<Account>, JsError> {
        let account = backend()?.add_imap(input.0).await.map_err(js)?;
        changed(account.id.clone(), FolderKind::Inbox);
        Ok(Js(account))
    }

    /// Adds a Gmail or Outlook account through the provider's sign-in page.
    pub async fn add_o_auth(provider: Js<OAuthProvider>) -> Result<Js<Account>, JsError> {
        let account = backend()?.add_oauth(provider.0).await.map_err(js)?;
        changed(account.id.clone(), FolderKind::Inbox);
        Ok(Js(account))
    }

    /// Removes an account and its mail from this app.
    pub async fn remove_account(id: String) -> Result<(), JsError> {
        backend()?.remove_account(id.clone()).await.map_err(js)?;
        changed(id, FolderKind::Inbox);
        Ok(())
    }

    /// A page of a folder, newest first.
    pub async fn list(query: Js<ListQuery>) -> Result<Js<Vec<Summary>>, JsError> {
        backend()?.list(query.0).await.map(Js).map_err(js)
    }

    /// A whole message.
    pub async fn get(id: String) -> Result<Js<Message>, JsError> {
        backend()?.get(id).await.map(Js).map_err(js)
    }

    /// Marks a message read or unread, flagged or not.
    pub async fn set_flags(id: String, flags: Js<Flags>) -> Result<(), JsError> {
        backend()?.set_flags(id, flags.0).await.map_err(js)
    }

    /// Sends a message.
    pub async fn send(draft: Js<Draft>) -> Result<(), JsError> {
        let account = draft.0.account_id.clone();
        backend()?.send(draft.0).await.map_err(js)?;
        changed(account, FolderKind::Sent);
        Ok(())
    }

    /// Messages matching `text` in any folder, newest first.
    pub async fn search(text: String, limit: u32) -> Result<Js<Vec<Summary>>, JsError> {
        backend()?.search(text, limit).await.map(Js).map_err(js)
    }

    /// Syncs now.
    pub async fn refresh() -> Result<(), JsError> {
        backend()?.refresh().await.map_err(js)
    }

    /// Calls `listener` with each event: `newMail`, `changed`, `syncState`,
    /// `error`. Returns a function that stops listening.
    pub fn on(#[args(event)] listener: Callback<Js<MailEvent>>) -> Stop {
        let id = with(|s| {
            s.next_listener += 1;
            s.listeners.push((s.next_listener, listener));
            s.next_listener
        });
        Stop::new(move || with(|s| s.listeners.retain(|(l, _)| *l != id)))
    }
}
