//! `CloudBackend`: mail through an nsh-mail server (docs/design.md §4). REST
//! goes through the engine's `fetch`, events come over a WebSocket that
//! reconnects with backoff, and on the web Web Push covers a closed page.
//! Nothing is cached on the device yet: without the server, lists are empty.

mod socket;
#[cfg(target_arch = "wasm32")]
mod web;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    future::{Future, poll_fn},
    pin::pin,
    rc::{Rc, Weak},
    task::Poll,
    time::Duration,
};

use nsh_core::prelude::*;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use mail_core::types::*;

use crate::backend::{Backend, Events, File, MailFuture, Progress};

#[cfg(target_arch = "wasm32")]
pub use web::{finish_oauth, page_origin};

/// The longest wait between two attempts to reach the event stream.
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// How long a provider's sign-in page may stay open.
const OAUTH_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// A staged file goes to the server in chunks of this size: each a small
/// request, and progress between them.
const CHUNK: usize = 1024 * 1024;

/// A session on one server.
struct Api {
    server: String,
    token: String,
}

impl Api {
    async fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<T> {
        let body = body.map(|b| ("application/json", b.to_string().into_bytes()));
        let text = request(&self.server, Some(&self.token), method, path, body).await?;
        parse(if text.is_empty() { "null" } else { &text })
    }

    async fn bytes(&self, path: &str) -> Result<Vec<u8>> {
        Ok(send(&self.server, Some(&self.token), "GET", path, None)
            .await?
            .body)
    }

    /// Sends a chunk of a file as the body of a `PATCH`.
    async fn put_bytes(&self, path: &str, bytes: Vec<u8>) -> Result<()> {
        let body = Some(("application/octet-stream", bytes));
        request(&self.server, Some(&self.token), "PATCH", path, body).await?;
        Ok(())
    }

    /// The event stream's URL: the token goes in the query, as browsers
    /// cannot set headers on a WebSocket.
    fn events_url(&self, device: Option<&str>) -> String {
        let base = self
            .server
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
        let mut url = format!("{base}/v1/events?token={}", enc(&self.token));
        if let Some(device) = device {
            url.push_str(&format!("&device={}", enc(device)));
        }
        url
    }
}

async fn request(
    server: &str,
    token: Option<&str>,
    method: &str,
    path: &str,
    body: Option<(&str, Vec<u8>)>,
) -> Result<String> {
    Ok(send(server, token, method, path, body).await?.text())
}

async fn send(
    server: &str,
    token: Option<&str>,
    method: &str,
    path: &str,
    body: Option<(&str, Vec<u8>)>,
) -> Result<HttpResponse> {
    let mut req =
        HttpRequest::new(method, format!("{server}/v1{path}")).header("Accept", "application/json");
    if let Some(token) = token {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    if let Some((content_type, body)) = body {
        req = req.header("Content-Type", content_type).body(body);
    }
    let res = fetch(req)
        .await
        .map_err(|e| Error::new(ErrorCode::Network, format!("cannot reach {server}: {e}")))?;
    if res.ok() {
        return Ok(res);
    }
    // The server's errors are mail-core's `{code, message}`.
    Err(
        serde_json::from_slice::<Error>(&res.body).unwrap_or_else(|_| {
            let code = if res.status() == 401 {
                ErrorCode::Auth
            } else {
                ErrorCode::Protocol
            };
            Error::new(code, format!("{server} answered HTTP {}", res.status()))
        }),
    )
}

fn parse<T: DeserializeOwned>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|e| {
        Error::new(
            ErrorCode::Protocol,
            format!("the server's answer is not what this app reads: {e}"),
        )
    })
}

fn to_json(value: &impl serde::Serialize) -> Option<Value> {
    serde_json::to_value(value).ok()
}

/// `GET` path of a page of `/messages` or `/threads`.
fn page(base: &str, query: &ListQuery) -> String {
    let folder = to_json(&query.folder);
    let mut path = format!(
        "{base}?folder={}&limit={}",
        folder.as_ref().and_then(Value::as_str).unwrap_or("inbox"),
        query.limit.max(1)
    );
    for (key, value) in [("account", &query.account), ("before", &query.before)] {
        if let Some(value) = value {
            path.push_str(&format!("&{key}={}", enc(value)));
        }
    }
    path
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// `future`'s output, or `None` once `limit` passed.
async fn within<T>(limit: Duration, future: impl Future<Output = T>) -> Option<T> {
    let mut future = pin!(future);
    let mut deadline = pin!(timer(limit));
    poll_fn(|cx| {
        if let Poll::Ready(value) = future.as_mut().poll(cx) {
            return Poll::Ready(Some(value));
        }
        deadline.as_mut().poll(cx).map(|_| None)
    })
    .await
}

/// The session token: in the OS keyring (or mail-core's encrypted file where
/// there is none) on native targets, in the app's storage on the web and
/// where the host keeps no files (headless tests).
mod token {
    use nsh_core::storage::Storage;

    use mail_core::types::{Error, ErrorCode, Result};

    const KEY: &str = "nshmail.cloud.token";

    #[cfg(not(target_arch = "wasm32"))]
    async fn with_secrets<T: Send + 'static>(
        dir: std::path::PathBuf,
        f: impl FnOnce(&dyn mail_core::SecretStore) -> mail_core::Result<T> + Send + 'static,
    ) -> Result<T> {
        nsh_core::prelude::blocking(move || {
            let (store, _) = mail_core::secrets::platform_store(&dir)?;
            f(store.as_ref())
        })
        .await
        .map_err(|e| Error::new(ErrorCode::Secrets, e.to_string()))?
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn secrets_dir() -> Option<std::path::PathBuf> {
        nsh_core::prelude::data_dir()
    }

    pub async fn load() -> Result<Option<String>> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = secrets_dir() {
            return with_secrets(dir, |s| s.get(KEY)).await;
        }
        Ok(Storage::get_item(KEY))
    }

    pub async fn save(token: &str) -> Result<()> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = secrets_dir() {
            let token = token.to_owned();
            return with_secrets(dir, move |s| s.set(KEY, &token)).await;
        }
        Storage::set_item(KEY, token)
            .map_err(|e| Error::new(ErrorCode::Storage, format!("cannot keep the session: {e}")))
    }

    pub async fn forget() {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = secrets_dir() {
            if let Err(e) = with_secrets(dir, |s| s.delete(KEY)).await {
                log::warn!("cannot forget the session token: {e}");
            }
            return;
        }
        Storage::remove_item(KEY);
    }
}

struct Inner {
    api: Api,
    sys: System,
    emit: RefCell<Option<Events>>,
    listening: RefCell<Option<Spawned<()>>>,
    /// This browser's push subscription on the server, once it has one.
    device: RefCell<Option<String>>,
    /// The server's cap on attachments, from `/me`.
    attachment_limit: Cell<u64>,
}

impl Inner {
    /// (Re)starts listening for events, as this device when it has an id.
    fn listen(&self) {
        let Some(emit) = self.emit.borrow().clone() else {
            return;
        };
        let url = self.api.events_url(self.device.borrow().as_deref());
        let sys = self.sys.clone();
        let task = Owner::detached().run(|| spawn(listen(url, sys, emit)));
        if let Some(old) = self.listening.replace(Some(task)) {
            old.cancel();
        }
    }

    /// This device's push registration on the server: listening as it, the
    /// server pushes to it only while its event stream is closed.
    fn registered(&self, device: String) {
        *self.device.borrow_mut() = Some(device);
        self.listen();
    }
}

/// Registers Android's FCM token (`push.rs`, from the Gradle host) with the server.
fn register_fcm(inner: Weak<Inner>) -> crate::push::TokenSink {
    Rc::new(move |token: &str| {
        let (inner, token) = (inner.clone(), token.to_owned());
        Owner::detached().run(|| {
            spawn(async move {
                let Some(this) = inner.upgrade() else { return };
                let body = json!({ "kind": "fcm", "token": token });
                match this.api.call::<Value>("POST", "/devices", Some(body)).await {
                    Ok(device) => match device["id"].as_str() {
                        Some(id) => this.registered(id.to_owned()),
                        None => log::warn!("the server registered no FCM device"),
                    },
                    Err(e) => log::warn!("cannot register for push: {}", e.message),
                }
            })
        });
    })
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(task) = self.listening.take() {
            task.cancel();
        }
    }
}

/// The event stream's own state, as a sync state of no account in particular
/// (`accountId` empty): its error shows in `Mail.status()`.
fn server_state(error: Option<Error>) -> MailEvent {
    MailEvent::SyncState {
        account_id: String::new(),
        syncing: false,
        error,
    }
}

/// Bridges the server's events to `Mail.on`, reconnecting with backoff. Each
/// connection starts with `changed`: lists reload, as events may have come
/// while none was open.
async fn listen(url: String, sys: System, emit: Events) {
    let mut delay = Duration::from_secs(1);
    let mut failing = false;
    loop {
        let mut opened = false;
        let mut stream = socket::connect(&url, &sys);
        while let Some(incoming) = stream.next().await {
            match incoming {
                socket::Incoming::Open => {
                    opened = true;
                    delay = Duration::from_secs(1);
                    if failing {
                        failing = false;
                        emit(server_state(None));
                    }
                    emit(MailEvent::Changed {
                        account_id: String::new(),
                        folder: FolderKind::Inbox,
                    });
                }
                socket::Incoming::Text(text) => match serde_json::from_str::<MailEvent>(&text) {
                    Ok(event) => emit(event),
                    Err(e) => log::warn!("an event this app does not read: {e}"),
                },
            }
        }
        drop(stream);
        if !opened {
            if !failing {
                failing = true;
                emit(server_state(Some(Error::new(
                    ErrorCode::Network,
                    "cannot reach the server for new mail; retrying",
                ))));
            }
            delay = (delay * 2).min(MAX_BACKOFF);
        }
        timer(delay).await;
    }
}

/// Mail through an nsh-mail server (see the module's docs).
pub struct CloudBackend(Rc<Inner>);

impl CloudBackend {
    fn new(server: String, token: String, sys: System) -> Self {
        Self(Rc::new(Inner {
            api: Api { server, token },
            sys,
            emit: RefCell::default(),
            listening: RefCell::default(),
            device: RefCell::default(),
            attachment_limit: Cell::new(MAX_ATTACHMENTS),
        }))
    }

    /// Signs in to `server` (or creates an account there) and keeps the session.
    pub async fn sign_in(
        server: String,
        email: String,
        password: String,
        sign_up: bool,
        sys: System,
    ) -> Result<Self> {
        let path = if sign_up {
            "/auth/signup"
        } else {
            "/auth/login"
        };
        let body = json!({ "email": email, "password": password }).to_string();
        let body = Some(("application/json", body.into_bytes()));
        let answer: Value = parse(&request(&server, None, "POST", path, body).await?)?;
        let token = answer["token"]
            .as_str()
            .ok_or_else(|| Error::new(ErrorCode::Protocol, "the server sent no session token"))?
            .to_owned();
        token::save(&token).await?;
        Ok(Self::new(server, token, sys))
    }

    /// The session kept from last time, if any.
    pub async fn restore(server: String, sys: System) -> Result<Option<Self>> {
        Ok(token::load()
            .await?
            .map(|token| Self::new(server, token, sys)))
    }

    fn api(&self) -> &Api {
        &self.0.api
    }

    /// The link the provider's sign-in ends with: `nshmail://oauth-done?…`
    /// on native targets, the web client's `/?oauth-done&…` on the web.
    async fn oauth_return(&self, provider: OAuthProvider) -> Result<String> {
        let name = provider.as_str();
        let target = if cfg!(target_arch = "wasm32") {
            "web"
        } else {
            "native"
        };
        let url = format!(
            "{}/v1/oauth/{name}/start?token={}&return={target}",
            self.api().server,
            enc(&self.api().token)
        );
        #[cfg(target_arch = "wasm32")]
        let returned = web::oauth_return();
        // Before any await: a browser opens the window only right after a press.
        let opened = open_url(url);
        if !opened.await {
            return Err(Error::new(
                ErrorCode::Unsupported,
                if cfg!(target_arch = "wasm32") {
                    "the sign-in window was blocked: allow pop-ups for this site"
                } else {
                    "cannot open the browser for the sign-in"
                },
            ));
        }
        #[cfg(not(target_arch = "wasm32"))]
        let returned = async {
            loop {
                let link = next_url_scheme_open().await;
                if link.starts_with("nshmail://oauth-done") {
                    return Ok::<_, Error>(link);
                }
            }
        };
        within(OAUTH_TIMEOUT, returned).await.unwrap_or_else(|| {
            Err(Error::new(
                ErrorCode::OAuth,
                "the sign-in took too long; try again",
            ))
        })
    }
}

impl Backend for CloudBackend {
    fn start(&self, events: Events) -> MailFuture<'_, ()> {
        *self.0.emit.borrow_mut() = Some(events);
        self.0.listen();
        let inner = Rc::downgrade(&self.0);
        Owner::detached().run(|| {
            spawn(async move {
                let Some(this) = inner.upgrade() else { return };
                match this.api.call::<Value>("GET", "/me", None).await {
                    Ok(me) => {
                        if let Some(limit) = me["maxAttachmentBytes"].as_u64() {
                            this.attachment_limit.set(limit);
                        }
                    }
                    Err(e) => log::warn!("cannot read the server's limits: {}", e.message),
                }
            })
        });
        crate::push::on_token(Some(register_fcm(Rc::downgrade(&self.0))));
        #[cfg(target_arch = "wasm32")]
        {
            let inner = Rc::downgrade(&self.0);
            Owner::detached().run(|| {
                spawn(async move {
                    let Some(this) = inner.upgrade() else { return };
                    match web::subscribe_push(&this.api).await {
                        Ok(Some(device)) => this.registered(device),
                        Ok(None) => {}
                        Err(e) => log::warn!("no Web Push on this browser: {}", e.message),
                    }
                })
            });
        }
        Box::pin(async { Ok(()) })
    }

    fn sign_out(&self) -> MailFuture<'_, ()> {
        Box::pin(async move {
            if let Some(task) = self.0.listening.take() {
                task.cancel();
            }
            crate::push::on_token(None);
            #[cfg(target_arch = "wasm32")]
            web::unsubscribe_push().await;
            let device = self.0.device.borrow_mut().take();
            if let Some(device) = device {
                let path = format!("/devices/{}", enc(&device));
                let _ = self.api().call::<()>("DELETE", &path, None).await;
            }
            // The session ends here even when the server cannot be told.
            let _ = self.api().call::<()>("POST", "/auth/logout", None).await;
            token::forget().await;
            Ok(())
        })
    }

    fn accounts(&self) -> MailFuture<'_, Vec<Account>> {
        Box::pin(self.api().call("GET", "/accounts", None))
    }

    fn detect(&self, email: String) -> MailFuture<'_, Option<ServerSettings>> {
        Box::pin(async move {
            let path = format!("/detect?email={}", enc(email.trim()));
            self.api().call("GET", &path, None).await
        })
    }

    fn add_imap(&self, input: ImapInput) -> MailFuture<'_, Account> {
        Box::pin(self.api().call("POST", "/accounts", to_json(&input)))
    }

    fn add_oauth(&self, provider: OAuthProvider) -> MailFuture<'_, Account> {
        Box::pin(async move {
            let link = self.oauth_return(provider).await?;
            let query = link.split_once('?').map_or("", |(_, q)| q);
            let mut account = None;
            for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
                match &*key {
                    "error" => return Err(Error::new(ErrorCode::OAuth, value)),
                    "account" => account = Some(value.into_owned()),
                    _ => {}
                }
            }
            let id = account.ok_or_else(|| {
                Error::new(ErrorCode::OAuth, "the sign-in ended without an account")
            })?;
            let accounts: Vec<Account> = self.api().call("GET", "/accounts", None).await?;
            accounts
                .into_iter()
                .find(|a| a.id == id)
                .ok_or_else(|| Error::new(ErrorCode::NotFound, "account not found"))
        })
    }

    fn remove_account(&self, id: String) -> MailFuture<'_, ()> {
        Box::pin(async move {
            let path = format!("/accounts/{}", enc(&id));
            self.api().call("DELETE", &path, None).await
        })
    }

    fn list(&self, query: ListQuery) -> MailFuture<'_, Vec<Summary>> {
        Box::pin(async move {
            let path = page("/messages", &query);
            self.api().call("GET", &path, None).await
        })
    }

    fn threads(&self, query: ListQuery) -> MailFuture<'_, Vec<Thread>> {
        Box::pin(async move {
            let path = page("/threads", &query);
            self.api().call("GET", &path, None).await
        })
    }

    fn thread(&self, id: String) -> MailFuture<'_, Vec<Summary>> {
        Box::pin(async move {
            let path = format!("/threads/{}", enc(&id));
            self.api().call("GET", &path, None).await
        })
    }

    fn set_thread_flags(&self, id: String, flags: Flags) -> MailFuture<'_, ()> {
        Box::pin(async move {
            let path = format!("/threads/{}", enc(&id));
            self.api().call("PATCH", &path, to_json(&flags)).await
        })
    }

    fn move_thread(&self, id: String, folder: FolderKind) -> MailFuture<'_, ()> {
        Box::pin(async move {
            let path = format!("/threads/{}/move", enc(&id));
            self.api()
                .call("POST", &path, to_json(&json!({ "folder": folder })))
                .await
        })
    }

    fn get(&self, id: String) -> MailFuture<'_, Message> {
        Box::pin(async move {
            let path = format!("/messages/{}", enc(&id));
            self.api().call("GET", &path, None).await
        })
    }

    fn attachment(&self, id: String, index: u32) -> MailFuture<'_, Vec<u8>> {
        Box::pin(async move {
            let path = format!("/messages/{}/attachments/{index}", enc(&id));
            self.api().bytes(&path).await
        })
    }

    fn set_flags(&self, id: String, flags: Flags) -> MailFuture<'_, ()> {
        Box::pin(async move {
            let path = format!("/messages/{}", enc(&id));
            self.api().call("PATCH", &path, to_json(&flags)).await
        })
    }

    fn move_to(&self, id: String, folder: FolderKind) -> MailFuture<'_, ()> {
        Box::pin(async move {
            let path = format!("/messages/{}/move", enc(&id));
            self.api()
                .call("POST", &path, to_json(&json!({ "folder": folder })))
                .await
        })
    }

    fn send(&self, draft: Draft) -> MailFuture<'_, ()> {
        Box::pin(self.api().call("POST", "/send", to_json(&draft)))
    }

    fn queue(&self, draft: Draft, when: SendAt) -> MailFuture<'_, Outgoing> {
        Box::pin(async move {
            let mut body = to_json(&draft).unwrap_or_default();
            if let (Some(body), Some(Value::Object(when))) = (body.as_object_mut(), to_json(&when))
            {
                body.extend(when);
            }
            self.api().call("POST", "/send", Some(body)).await
        })
    }

    fn outbox(&self) -> MailFuture<'_, Vec<Outgoing>> {
        Box::pin(self.api().call("GET", "/outbox", None))
    }

    fn cancel(&self, id: String) -> MailFuture<'_, Outgoing> {
        Box::pin(async move {
            let path = format!("/outbox/{}", enc(&id));
            self.api().call("DELETE", &path, None).await
        })
    }

    fn stage(&self, file: File, progress: Progress) -> MailFuture<'_, Upload> {
        Box::pin(async move {
            let File {
                name,
                mime_type,
                data,
            } = file;
            let meta = json!({ "filename": name, "mimeType": mime_type, "size": data.len() });
            let upload: Upload = self.api().call("POST", "/uploads", Some(meta)).await?;
            let path = format!("/uploads/{}", enc(&upload.id));
            let mut sent = 0;
            for chunk in data.chunks(CHUNK) {
                let at = format!("{path}?offset={sent}");
                if let Err(e) = self.api().put_bytes(&at, chunk.to_vec()).await {
                    let _ = self.api().call::<()>("DELETE", &path, None).await;
                    return Err(e);
                }
                sent += chunk.len();
                progress(sent as u64);
            }
            if data.is_empty() {
                progress(0);
            }
            Ok(upload)
        })
    }

    fn unstage(&self, id: String) -> MailFuture<'_, ()> {
        Box::pin(async move {
            let path = format!("/uploads/{}", enc(&id));
            match self.api().call::<()>("DELETE", &path, None).await {
                Err(e) if e.code != ErrorCode::NotFound => Err(e),
                _ => Ok(()),
            }
        })
    }

    fn attachment_limit(&self) -> u64 {
        self.0.attachment_limit.get()
    }

    fn search(&self, text: String, limit: u32) -> MailFuture<'_, Vec<Summary>> {
        Box::pin(async move {
            let path = format!("/search?q={}&limit={}", enc(&text), limit.max(1));
            self.api().call("GET", &path, None).await
        })
    }

    fn unread(&self) -> MailFuture<'_, HashMap<AccountId, u32>> {
        Box::pin(self.api().call("GET", "/unread", None))
    }

    fn refresh(&self) -> MailFuture<'_, ()> {
        Box::pin(self.api().call("POST", "/sync", None))
    }
}
