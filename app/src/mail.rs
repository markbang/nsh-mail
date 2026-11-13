//! `Mail`, the one interface TSX uses (docs/design.md §3). It owns the
//! session: which backend runs, its status, and the listeners of its events.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use mail_core::types::*;
use nsh_core::{prelude::*, storage::Storage};

use crate::{
    backend::{Backend, File, invalid, js},
    cloud::CloudBackend,
    types::Js,
};

const MODE_KEY: &str = "nshmail.mode";
const SERVER_KEY: &str = "nshmail.server";
const USER_KEY: &str = "nshmail.user";

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
        user: None,
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

/// The backend of local mode. The in-memory demo backs the UI tests
/// (`nshmail.mode` = `demo`) and the emulator's screenshots (a release build
/// with `--cfg nshmail_demo`, where it is the mode until one is chosen);
/// other release builds have no demo.
fn local_backend(demo: bool) -> Result<Rc<dyn Backend>, JsError> {
    #[cfg(any(debug_assertions, nshmail_demo))]
    if demo {
        return Ok(Rc::new(crate::demo::DemoBackend::default()));
    }
    let _ = demo;
    #[cfg(not(target_arch = "wasm32"))]
    return Ok(Rc::new(crate::local::LocalBackend::default()));
    #[cfg(target_arch = "wasm32")]
    Err(js(Error::new(
        ErrorCode::Unsupported,
        "this mode is not available in this build",
    )))
}

/// Opens a session for `mode` on `backend`, ending the last one.
async fn open(
    mode: Mode,
    server: Option<String>,
    user: Option<String>,
    backend: Rc<dyn Backend>,
) -> Result<(), JsError> {
    with(|s| {
        s.backend = Some(backend.clone());
        s.syncing.clear();
        s.status = Status {
            mode,
            server,
            user,
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

fn remember(mode: Option<&str>, server: Option<&str>, user: Option<&str>) {
    let Some(mode) = mode else {
        for key in [MODE_KEY, SERVER_KEY, USER_KEY] {
            Storage::remove_item(key);
        }
        return;
    };
    // A full storage only costs the remembered mode: onboarding shows again.
    let _ = Storage::set_item(MODE_KEY, mode);
    for (key, value) in [(SERVER_KEY, server), (USER_KEY, user)] {
        match value {
            Some(value) => drop(Storage::set_item(key, value)),
            None => Storage::remove_item(key),
        }
    }
}

fn too_large(name: &str, limit: u64) -> Error {
    Error::new(
        ErrorCode::TooLarge,
        format!("{name} is larger than {} MB", limit / (1024 * 1024)),
    )
}

fn valid_server(server: &str) -> Result<String, JsError> {
    let server = server.trim().trim_end_matches('/');
    // The web client is served by its server: the page's origin, by default.
    #[cfg(target_arch = "wasm32")]
    if let Some(origin) = crate::cloud::page_origin()
        && (server.is_empty() || server == origin)
    {
        return Ok(origin);
    }
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
    pub async fn init(sys: System) -> Result<Js<Status>, JsError> {
        if with(|s| std::mem::replace(&mut s.started, true)) {
            return Ok(Mail::status());
        }
        #[cfg(target_arch = "wasm32")]
        crate::cloud::finish_oauth();
        let server = Storage::get_item(SERVER_KEY);
        let mode =
            Storage::get_item(MODE_KEY).or_else(|| cfg!(nshmail_demo).then(|| "demo".to_owned()));
        match (mode.as_deref(), server) {
            (Some("local"), _) if cfg!(not(target_arch = "wasm32")) => {
                open(Mode::Local, None, None, local_backend(false)?).await?
            }
            (Some("demo"), _) if cfg!(any(debug_assertions, nshmail_demo)) => {
                open(Mode::Local, None, None, local_backend(true)?).await?
            }
            (Some("cloud"), Some(server)) => {
                match CloudBackend::restore(server.clone(), sys)
                    .await
                    .map_err(js)?
                {
                    Some(backend) => {
                        let user = Storage::get_item(USER_KEY);
                        open(Mode::Cloud, Some(server), user, Rc::new(backend)).await?
                    }
                    // Signed out elsewhere, or the keyring lost the session: sign in again.
                    None => remember(None, None, None),
                }
            }
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
        open(Mode::Local, None, None, local_backend(false)?).await?;
        remember(Some("local"), None, None);
        Ok(())
    }

    /// Signs in to (or, with `sign_up`, creates an account on) the nsh-mail
    /// server at `server`, and syncs through it. On the web an empty
    /// `server` is the page's own.
    pub async fn sign_in(
        server: String,
        email: String,
        password: String,
        sign_up: bool,
        sys: System,
    ) -> Result<(), JsError> {
        let server = valid_server(&server)?;
        if !email.contains('@') {
            return Err(js(invalid("enter an email address")));
        }
        if password.is_empty() || (sign_up && password.chars().count() < 8) {
            return Err(js(invalid("a password has at least 8 characters")));
        }
        let user = email.trim().to_owned();
        let backend = CloudBackend::sign_in(server.clone(), email, password, sign_up, sys)
            .await
            .map_err(js)?;
        open(
            Mode::Cloud,
            Some(server.clone()),
            Some(user.clone()),
            Rc::new(backend),
        )
        .await?;
        remember(Some("cloud"), Some(&server), Some(&user));
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
        remember(None, None, None);
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

    /// A page of conversations in a folder, newest first; `before` is the
    /// previous page's last conversation.
    pub async fn threads(query: Js<ListQuery>) -> Result<Js<Vec<Thread>>, JsError> {
        backend()?.threads(query.0).await.map(Js).map_err(js)
    }

    /// A conversation's messages, oldest first, in every folder and account.
    pub async fn thread(id: String) -> Result<Js<Vec<Summary>>, JsError> {
        backend()?.thread(id).await.map(Js).map_err(js)
    }

    /// Marks a whole conversation read or unread; flagging flags its latest
    /// message, unflagging every one.
    pub async fn set_thread_flags(id: String, flags: Js<Flags>) -> Result<(), JsError> {
        backend()?.set_thread_flags(id, flags.0).await.map_err(js)
    }

    /// Archives a conversation (its messages in the inbox) or deletes it (all
    /// of its messages go to Trash).
    pub async fn move_thread(id: String, folder: Js<FolderKind>) -> Result<(), JsError> {
        backend()?.move_thread(id, folder.0).await.map_err(js)
    }

    /// A whole message.
    pub async fn get(id: String) -> Result<Js<Message>, JsError> {
        backend()?.get(id).await.map(Js).map_err(js)
    }

    /// Opens attachment `attachment` of message `id`: in a new tab (images, PDFs) or as a
    /// download on the web; elsewhere saved where the user picks (in the app's files where
    /// there is no save dialog) and opened in its default app. Resolves with the saved file's
    /// path when no app opened it, else `null`.
    pub async fn open_attachment(
        id: String,
        attachment: Js<Attachment>,
    ) -> Result<Option<String>, JsError> {
        let a = attachment.0;
        let backend = backend()?;
        #[cfg(target_arch = "wasm32")]
        let tab = crate::attachments::Tab::open(&a);
        let bytes = backend.attachment(id.clone(), a.index).await.map_err(js)?;
        #[cfg(target_arch = "wasm32")]
        return crate::attachments::deliver(tab, &a, &bytes)
            .map(|()| None)
            .map_err(js);
        #[cfg(not(target_arch = "wasm32"))]
        crate::attachments::save(&id, &a, bytes).await.map_err(js)
    }

    /// A thumbnail of image attachment `attachment` of message `id`, as a `data:` URI. Rejects
    /// with `ERR_UNSUPPORTED` for other types and large images.
    pub async fn attachment_preview(
        id: String,
        attachment: Js<Attachment>,
    ) -> Result<String, JsError> {
        let a = attachment.0;
        if !crate::attachments::previewable(&a) {
            return Err(js(Error::new(
                ErrorCode::Unsupported,
                "no preview for this attachment",
            )));
        }
        let bytes = backend()?.attachment(id, a.index).await.map_err(js)?;
        Ok(crate::attachments::data_uri(&a, &bytes))
    }

    /// Marks a message read or unread, flagged or not.
    pub async fn set_flags(id: String, flags: Js<Flags>) -> Result<(), JsError> {
        backend()?.set_flags(id, flags.0).await.map_err(js)
    }

    /// Moves a message to another folder of its account: `"archive"`, `"trash"`.
    pub async fn move_to(id: String, folder: Js<FolderKind>) -> Result<(), JsError> {
        backend()?.move_to(id, folder.0).await.map_err(js)
    }

    /// Sends a message.
    pub async fn send(draft: Js<Draft>) -> Result<(), JsError> {
        let account = draft.0.account_id.clone();
        backend()?.send(draft.0).await.map_err(js)?;
        changed(account, FolderKind::Sent);
        Ok(())
    }

    /// Puts a message in the outbox: it goes after `when.delay` seconds (the
    /// undo window) or at `when.sendAt` (ms since the epoch), kept in the
    /// app's database in local mode (a restart sends what came due) and on
    /// the server in cloud mode. Resolves with it; `outbox` events follow.
    pub async fn queue(draft: Js<Draft>, when: Js<SendAt>) -> Result<Js<Outgoing>, JsError> {
        backend()?.queue(draft.0, when.0).await.map(Js).map_err(js)
    }

    /// The messages waiting to go (undo window, scheduled) or that failed to.
    pub async fn outbox() -> Result<Js<Vec<Outgoing>>, JsError> {
        backend()?.outbox().await.map(Js).map_err(js)
    }

    /// Takes a message out of the outbox before it goes. Its files are staged
    /// again under the ids its draft names, so it can be sent as it is.
    /// Rejects once it is on its way.
    pub async fn cancel(id: String) -> Result<Js<Outgoing>, JsError> {
        backend()?.cancel(id).await.map(Js).map_err(js)
    }

    /// Keeps a file for the draft being written: in the app in local mode, on
    /// the server in cloud mode, where it goes in chunks. `onProgress` hears
    /// how many bytes are there so far. Resolves with the id the draft's
    /// attachments name it by.
    pub async fn stage(
        name: String,
        mime_type: String,
        data: Vec<u8>,
        #[args(sent)] on_progress: Callback<f64>,
    ) -> Result<Js<Upload>, JsError> {
        let limit = backend()?.attachment_limit();
        if data.len() as u64 > limit {
            return Err(js(too_large(&name, limit)));
        }
        let file = File {
            name,
            mime_type,
            data,
        };
        let progress = Rc::new(move |sent: u64| on_progress.post(sent as f64));
        backend()?.stage(file, progress).await.map(Js).map_err(js)
    }

    /// `stage` for a local file, by path or `file:` URL: a desktop drop, or a
    /// file manager's copy pasted. Read here, as the app's TSX may not read
    /// files outside its own. Rejects with `ERR_UNSUPPORTED` on the web.
    pub async fn stage_file(
        path: String,
        mime_type: String,
        #[args(sent)] on_progress: Callback<f64>,
    ) -> Result<Js<Upload>, JsError> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (path, mime_type, on_progress);
            Err(js(Error::new(
                ErrorCode::Unsupported,
                "a page cannot read local files by path",
            )))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = match url::Url::parse(&path) {
                Ok(url) if url.scheme() == "file" => url
                    .to_file_path()
                    .map_err(|_| js(invalid(format!("not a local file: {path}"))))?,
                _ => std::path::PathBuf::from(path),
            };
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".into());
            let limit = backend()?.attachment_limit();
            let (shown, read) = (name.clone(), path.clone());
            let data = blocking(move || {
                if std::fs::metadata(&read)?.len() > limit {
                    return Ok(None);
                }
                std::fs::read(&read).map(Some)
            })
            .await
            .map_err(|e| js(Error::new(ErrorCode::Internal, e.to_string())))?
            .map_err(|e| {
                js(Error::new(
                    ErrorCode::Storage,
                    format!("cannot read {}: {e}", path.display()),
                ))
            })?
            .ok_or_else(|| js(too_large(&shown, limit)))?;
            let file = File {
                name,
                mime_type,
                data,
            };
            let progress = Rc::new(move |sent: u64| on_progress.post(sent as f64));
            backend()?.stage(file, progress).await.map(Js).map_err(js)
        }
    }

    /// Drops a staged file the draft no longer sends.
    pub async fn unstage(id: String) -> Result<(), JsError> {
        backend()?.unstage(id).await.map_err(js)
    }

    /// The most a message's attachments may add up to, in bytes.
    pub fn attachment_limit() -> f64 {
        with(|s| s.backend.clone()).map_or(MAX_ATTACHMENTS, |b| b.attachment_limit()) as f64
    }

    /// Messages matching `text` in any folder, newest first.
    pub async fn search(text: String, limit: u32) -> Result<Js<Vec<Summary>>, JsError> {
        backend()?.search(text, limit).await.map(Js).map_err(js)
    }

    /// Unread messages in each account's inbox, by account id.
    pub async fn unread() -> Result<Js<HashMap<String, u32>>, JsError> {
        backend()?.unread().await.map(Js).map_err(js)
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
