//! The facade the app (local mode) and the server use. One `Engine` per mail database; the server
//! opens one per user. Cheap to clone; all methods are `Send` futures on tokio.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use tokio::sync::{OwnedMutexGuard, broadcast};
use tokio::task::AbortHandle;

use crate::error::err;
use crate::imap::{self, Conn, Login};
use crate::mime;
use crate::oauth::{self, LoopbackAuthorization, OAuthApps, OAuthGrant, OAuthTokens};
use crate::provider;
use crate::secrets::SecretStore;
use crate::smtp::{self, SmtpAuth};
use crate::store::{Store, new_id};
use crate::types::*;

const IDLE_RENEW: Duration = Duration::from_secs(29 * 60);
const POLL_INTERVAL: Duration = Duration::from_secs(2 * 60);
const OP_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// A pooled connection unused for longer than this is checked with NOOP before reuse.
const STALE_AFTER: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(5 * 60);

pub struct EngineOptions {
    /// SQLite file, e.g. `data_dir/mail.sqlite` or `data/users/<id>.sqlite`.
    pub db_path: PathBuf,
    pub secrets: Arc<dyn SecretStore>,
    pub oauth: OAuthApps,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

struct Pooled {
    conn: Conn,
    last_used: Instant,
}

type Slot = Arc<tokio::sync::Mutex<Option<Pooled>>>;

struct Inner {
    store: Arc<Store>,
    secrets: Arc<dyn SecretStore>,
    oauth: OAuthApps,
    http: reqwest::Client,
    events: broadcast::Sender<MailEvent>,
    /// One command connection per account, shared by sync, body fetch, flags and append. Holding
    /// its lock also serializes syncs of an account.
    pool: Mutex<HashMap<AccountId, Slot>>,
    /// `Some` once `run` was called: the per-account IDLE loops.
    loops: Mutex<Option<HashMap<AccountId, AbortHandle>>>,
    refresh_lock: tokio::sync::Mutex<()>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(loops) = self
            .loops
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            loops.values().for_each(AbortHandle::abort);
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn password_key(id: &str) -> String {
    format!("account/{id}/password")
}

fn oauth_key(id: &str) -> String {
    format!("account/{id}/oauth")
}

enum Credential {
    Password(String),
    OAuth(String),
}

impl Engine {
    /// Opens (or creates) the database at `db_path` with the build's OAuth client ids.
    pub fn open(db_path: impl AsRef<Path>, secrets: Arc<dyn SecretStore>) -> Result<Self> {
        Self::open_with(EngineOptions {
            db_path: db_path.as_ref().to_path_buf(),
            secrets,
            oauth: OAuthApps::builtin(),
        })
    }

    pub fn open_with(options: EngineOptions) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Inner {
                store: Arc::new(Store::open(&options.db_path)?),
                secrets: options.secrets,
                oauth: options.oauth,
                http: crate::tls::http_client(),
                events: broadcast::channel(256).0,
                pool: Mutex::new(HashMap::new()),
                loops: Mutex::new(None),
                refresh_lock: tokio::sync::Mutex::new(()),
            }),
        })
    }

    /// Events from `run`, `sync`, `set_flags` and `send`. A lagging receiver loses the oldest
    /// events (`RecvError::Lagged`) and should reload what it shows.
    pub fn subscribe(&self) -> broadcast::Receiver<MailEvent> {
        self.inner.events.subscribe()
    }

    fn emit(&self, e: MailEvent) {
        let _ = self.inner.events.send(e);
    }

    fn store(&self) -> &Arc<Store> {
        &self.inner.store
    }

    async fn secrets<T: Send + 'static>(
        &self,
        f: impl FnOnce(&dyn SecretStore) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let s = self.inner.secrets.clone();
        tokio::task::spawn_blocking(move || f(s.as_ref())).await?
    }

    pub async fn accounts(&self) -> Result<Vec<Account>> {
        self.store().run(|s| s.accounts()).await
    }

    pub async fn detect(&self, email: &str) -> Result<Option<ServerSettings>> {
        provider::detect(&self.inner.http, email).await
    }

    /// Logs in to IMAP and SMTP with the given settings, then saves the account.
    pub async fn add_imap_account(&self, input: ImapInput) -> Result<Account> {
        let email = input.email.trim().to_string();
        if !email.contains('@') {
            return Err(err(
                ErrorCode::InvalidInput,
                format!("not an email address: {email}"),
            ));
        }
        for ep in [&input.imap, &input.smtp] {
            if ep.host.trim().is_empty() || ep.port == 0 {
                return Err(err(
                    ErrorCode::InvalidInput,
                    "server host and port are required",
                ));
            }
        }
        if self.find_account(&email).await?.is_some() {
            return Err(err(
                ErrorCode::InvalidInput,
                format!("{email} is already added"),
            ));
        }
        let username = input
            .username
            .clone()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| email.clone());
        let account = Account {
            id: new_id(),
            email,
            name: input.name.clone().filter(|n| !n.trim().is_empty()),
            provider: input.provider.clone(),
            username,
            auth: Auth::Password,
            imap: input.imap.clone(),
            smtp: input.smtp.clone(),
        };
        let cred = Credential::Password(input.password.clone());
        self.verify(&account, &cred).await?;
        let (key, value) = (password_key(&account.id), input.password);
        self.secrets(move |s| s.set(&key, &value)).await?;
        self.save_account(&account).await?;
        Ok(account)
    }

    /// Starts a native-app sign-in: open `url` of the result in the browser, then `finish()` it and
    /// pass the grant to `add_oauth_account`.
    pub async fn start_oauth(
        &self,
        provider: OAuthProvider,
        login_hint: Option<&str>,
    ) -> Result<LoopbackAuthorization> {
        oauth::start_loopback(
            self.inner.http.clone(),
            provider,
            &self.inner.oauth,
            login_hint,
        )
        .await
    }

    /// Saves an OAuth account after checking IMAP and SMTP accept the token. Signing in again to an
    /// existing account replaces its tokens.
    pub async fn add_oauth_account(&self, grant: OAuthGrant) -> Result<Account> {
        let preset_email = match grant.provider {
            OAuthProvider::Google => "x@gmail.com",
            OAuthProvider::Microsoft => "x@outlook.com",
        };
        let preset = provider::preset(preset_email).expect("built-in preset");
        let existing = self.find_account(&grant.email).await?;
        let is_new = existing.is_none();
        let account = match existing {
            Some(a)
                if a.auth
                    == (Auth::OAuth {
                        provider: grant.provider,
                    }) =>
            {
                a
            }
            Some(_) => {
                return Err(err(
                    ErrorCode::InvalidInput,
                    format!("{} is already added with a password", grant.email),
                ));
            }
            None => Account {
                id: new_id(),
                email: grant.email.clone(),
                name: grant.name.clone(),
                provider: preset.provider.clone(),
                username: grant.email.clone(),
                auth: Auth::OAuth {
                    provider: grant.provider,
                },
                imap: preset.imap,
                smtp: preset.smtp,
            },
        };
        self.verify(
            &account,
            &Credential::OAuth(grant.tokens.access_token.clone()),
        )
        .await?;
        let (key, value) = (
            oauth_key(&account.id),
            serde_json::to_string(&grant.tokens)?,
        );
        self.secrets(move |s| s.set(&key, &value)).await?;
        if is_new {
            self.save_account(&account).await?;
        } else {
            self.drop_connection(&account.id);
        }
        Ok(account)
    }

    async fn find_account(&self, email: &str) -> Result<Option<Account>> {
        Ok(self
            .accounts()
            .await?
            .into_iter()
            .find(|a| a.email.eq_ignore_ascii_case(email)))
    }

    async fn save_account(&self, account: &Account) -> Result<()> {
        let a = account.clone();
        self.store().run(move |s| s.insert_account(&a)).await?;
        self.start_loop(&account.id);
        Ok(())
    }

    async fn verify(&self, account: &Account, cred: &Credential) -> Result<()> {
        let mut conn = imap::connect(&account.imap, login(account, cred)).await?;
        let _ = conn.session.logout().await;
        smtp::verify(&account.smtp, smtp_auth(account, cred)).await
    }

    pub async fn remove_account(&self, id: &str) -> Result<()> {
        if let Some(loops) = lock(&self.inner.loops).as_mut()
            && let Some(h) = loops.remove(id)
        {
            h.abort();
        }
        self.drop_connection(id);
        let id = id.to_string();
        let (pk, ok) = (password_key(&id), oauth_key(&id));
        self.secrets(move |s| {
            s.delete(&pk)?;
            s.delete(&ok)
        })
        .await?;
        self.store().run(move |s| s.delete_account(&id)).await
    }

    async fn credential(&self, account: &Account) -> Result<Credential> {
        let missing = || {
            err(
                ErrorCode::Auth,
                format!(
                    "no saved credentials for {}; remove and add the account again",
                    account.email
                ),
            )
        };
        match account.auth {
            Auth::Password => {
                let key = password_key(&account.id);
                let p = self
                    .secrets(move |s| s.get(&key))
                    .await?
                    .ok_or_else(missing)?;
                Ok(Credential::Password(p))
            }
            Auth::OAuth { provider } => {
                let _g = self.inner.refresh_lock.lock().await;
                let key = oauth_key(&account.id);
                let raw = self
                    .secrets(move |s| s.get(&key))
                    .await?
                    .ok_or_else(missing)?;
                let tokens: OAuthTokens = serde_json::from_str(&raw)?;
                if tokens.expires_at - 120 > oauth::now() {
                    return Ok(Credential::OAuth(tokens.access_token));
                }
                let refresh = tokens.refresh_token.as_deref().ok_or_else(|| {
                    err(
                        ErrorCode::Auth,
                        format!("sign-in for {} expired; sign in again", account.email),
                    )
                })?;
                let fresh = oauth::refresh(
                    &self.inner.http,
                    provider,
                    self.inner.oauth.get(provider)?,
                    refresh,
                )
                .await?;
                let (key, value) = (oauth_key(&account.id), serde_json::to_string(&fresh)?);
                self.secrets(move |s| s.set(&key, &value)).await?;
                Ok(Credential::OAuth(fresh.access_token))
            }
        }
    }

    async fn connect(&self, account: &Account) -> Result<Conn> {
        let cred = self.credential(account).await?;
        imap::connect(&account.imap, login(account, &cred)).await
    }

    fn slot(&self, id: &str) -> Slot {
        lock(&self.inner.pool)
            .entry(id.to_string())
            .or_default()
            .clone()
    }

    fn drop_connection(&self, id: &str) {
        lock(&self.inner.pool).remove(id);
    }

    async fn pooled(&self, account: &Account) -> Result<OwnedMutexGuard<Option<Pooled>>> {
        let mut g = self.slot(&account.id).lock_owned().await;
        if let Some(p) = g.as_mut()
            && p.last_used.elapsed() > STALE_AFTER
            && p.conn.session.noop().await.is_err()
        {
            *g = None;
        }
        if g.is_none() {
            *g = Some(Pooled {
                conn: self.connect(account).await?,
                last_used: Instant::now(),
            });
        }
        Ok(g)
    }

    /// Runs `f` on the account's pooled connection; a failed operation discards the connection
    /// so the next one reconnects.
    async fn with_imap<T>(
        &self,
        account: &Account,
        f: impl AsyncFnOnce(&mut Conn) -> Result<T>,
    ) -> Result<T> {
        let mut g = self.pooled(account).await?;
        let pooled = g.as_mut().expect("connected above");
        let r = match tokio::time::timeout(OP_TIMEOUT, f(&mut pooled.conn)).await {
            Ok(r) => r,
            Err(e) => Err(e.into()),
        };
        match &r {
            Err(e) if e.code != ErrorCode::NotFound => *g = None,
            _ => pooled.last_used = Instant::now(),
        }
        r
    }

    /// Full sync of one account: folder discovery, then every known folder.
    pub async fn sync(&self, account_id: &str) -> Result<()> {
        let account = self.account(account_id).await?;
        self.sync_folders(&account, None).await
    }

    /// Syncs every account (pull to refresh). Errors are reported per account as `SyncState`
    /// events; the first one is also returned.
    pub async fn sync_all(&self) -> Result<()> {
        let mut first = None;
        for a in self.accounts().await? {
            if let Err(e) = self.sync_folders(&a, None).await {
                first.get_or_insert(e);
            }
        }
        first.map_or(Ok(()), Err)
    }

    async fn account(&self, id: &str) -> Result<Account> {
        let id = id.to_string();
        self.store().run(move |s| s.account(&id)).await
    }

    async fn sync_folders(&self, account: &Account, only: Option<FolderKind>) -> Result<()> {
        self.emit(MailEvent::SyncState {
            account_id: account.id.clone(),
            syncing: true,
            error: None,
        });
        let store = self.store().clone();
        let account_id = account.id.clone();
        let result = self
            .with_imap(account, async |conn| {
                let mut folders = match only {
                    Some(kind) => {
                        let id = account_id.clone();
                        store
                            .run(move |s| s.folder(&id, kind))
                            .await?
                            .into_iter()
                            .collect()
                    }
                    None => Vec::new(),
                };
                if folders.is_empty() {
                    let found = imap::discover(&mut conn.session).await?;
                    let id = account_id.clone();
                    folders = store
                        .run(move |s| {
                            s.set_folders(&id, &found)?;
                            s.folders(&id)
                        })
                        .await?;
                    folders.sort_by_key(|f| f.kind != FolderKind::Inbox);
                }
                let mut out = Vec::new();
                for f in folders {
                    let kind = f.kind;
                    out.push((kind, imap::sync_folder(conn, &store, f).await?));
                }
                Ok(out)
            })
            .await;
        let error = result.as_ref().err().cloned();
        if let Ok(results) = &result {
            for (kind, r) in results {
                if *kind == FolderKind::Inbox && !r.initial {
                    for s in r.inserted.iter().filter(|s| !s.seen) {
                        self.emit(MailEvent::NewMail { summary: s.clone() });
                    }
                }
                if r.changed {
                    self.emit(MailEvent::Changed {
                        account_id: account.id.clone(),
                        folder: *kind,
                    });
                }
            }
        }
        self.emit(MailEvent::SyncState {
            account_id: account.id.clone(),
            syncing: false,
            error,
        });
        result.map(|_| ())
    }

    /// Starts background sync: per account, a full sync, then IDLE on INBOX (renewed every 29
    /// minutes, with a full sync on each renewal), or polling every 2 minutes without IDLE.
    /// Accounts added later start automatically. Idempotent.
    pub async fn run(&self) -> Result<()> {
        let accounts = self.accounts().await?;
        lock(&self.inner.loops).get_or_insert_with(HashMap::new);
        for a in accounts {
            self.start_loop(&a.id);
        }
        Ok(())
    }

    /// Stops the background loops started by `run`.
    pub fn stop(&self) {
        if let Some(loops) = lock(&self.inner.loops).take() {
            loops.values().for_each(AbortHandle::abort);
        }
    }

    fn start_loop(&self, id: &str) {
        let mut loops = lock(&self.inner.loops);
        let Some(loops) = loops.as_mut() else { return };
        if loops.get(id).is_some_and(|h| !h.is_finished()) {
            return;
        }
        let task = tokio::spawn(account_loop(Arc::downgrade(&self.inner), id.to_string()));
        loops.insert(id.to_string(), task.abort_handle());
    }

    pub async fn list(&self, query: ListQuery) -> Result<Vec<Summary>> {
        self.store().run(move |s| s.list(&query)).await
    }

    pub async fn search(&self, text: &str, limit: u32) -> Result<Vec<Summary>> {
        let text = text.to_string();
        self.store().run(move |s| s.search(&text, limit)).await
    }

    /// The full message; the body is downloaded on first open and cached.
    pub async fn get(&self, id: &str) -> Result<Message> {
        let mid = id.to_string();
        let mut stored = self.store().run(move |s| s.message(&mid)).await?;
        let raw = match stored.body.take() {
            Some(raw) => raw,
            None => {
                let account = self.account(&stored.summary.account_id).await?;
                let (folder, uid) = (stored.folder_name.clone(), stored.uid);
                let raw = self
                    .with_imap(&account, async |conn| {
                        imap::fetch_body(&mut conn.session, &folder, uid).await
                    })
                    .await?;
                let parsed = mime::parse_body(&raw);
                let has_attachments = !parsed.attachments.is_empty();
                stored.summary.has_attachments = has_attachments;
                let (mid, raw2, text) = (id.to_string(), raw.clone(), parsed.index_text);
                self.store()
                    .run(move |s| s.set_body(&mid, &raw2, &text, has_attachments))
                    .await?;
                raw
            }
        };
        let body = tokio::task::spawn_blocking(move || mime::parse_body(&raw)).await?;
        stored.summary.has_attachments = !body.attachments.is_empty();
        Ok(Message {
            summary: stored.summary,
            cc: stored.cc,
            reply_to: body.reply_to,
            message_id: stored.message_id,
            in_reply_to: stored.in_reply_to,
            references: stored.references,
            text: body.text,
            html: body.html,
            remote_images: body.remote_images,
            attachments: body.attachments,
        })
    }

    /// Writes flags to the server, then to the local copy.
    pub async fn set_flags(&self, id: &str, flags: Flags) -> Result<()> {
        let mid = id.to_string();
        let stored = self.store().run(move |s| s.message(&mid)).await?;
        let account = self.account(&stored.summary.account_id).await?;
        let (folder, uid) = (stored.folder_name.clone(), stored.uid);
        self.with_imap(&account, async |conn| {
            imap::store_flags(&mut conn.session, &folder, uid, flags).await
        })
        .await?;
        let mid = id.to_string();
        self.store()
            .run(move |s| s.set_local_flags(&mid, flags))
            .await?;
        self.emit(MailEvent::Changed {
            account_id: account.id,
            folder: stored.summary.folder,
        });
        Ok(())
    }

    /// Sends over SMTP, then saves a copy to Sent (Gmail saves it itself). Once SMTP accepted the
    /// message this returns `Ok`; a failure to save the copy is reported as an `Error` event,
    /// so callers never resend a delivered message.
    pub async fn send(&self, draft: Draft) -> Result<()> {
        let account = self.account(&draft.account_id).await?;
        let message = smtp::build(&account, &draft)?;
        let cred = self.credential(&account).await?;
        smtp::send(&account.smtp, smtp_auth(&account, &cred), &message).await?;
        let raw = message.formatted();
        let gmail = is_gmail(&account);
        let store = self.store().clone();
        let account_id = account.id.clone();
        let saved = self
            .with_imap(&account, async |conn| {
                let id = account_id.clone();
                let mut sent = store.run(move |s| s.folder(&id, FolderKind::Sent)).await?;
                if sent.is_none() {
                    let mut found = imap::discover(&mut conn.session).await?;
                    if !found.iter().any(|(k, _)| *k == FolderKind::Sent) && !gmail {
                        imap::create(&mut conn.session, "Sent").await?;
                        found.push((FolderKind::Sent, "Sent".to_string()));
                    }
                    let id = account_id.clone();
                    sent = store
                        .run(move |s| {
                            s.set_folders(&id, &found)?;
                            s.folder(&id, FolderKind::Sent)
                        })
                        .await?;
                }
                let Some(sent) = sent else { return Ok(false) };
                if !gmail {
                    imap::append(&mut conn.session, &sent.name, &raw).await?;
                }
                Ok(imap::sync_folder(conn, &store, sent).await?.changed)
            })
            .await;
        match saved {
            Ok(changed) => {
                if changed {
                    self.emit(MailEvent::Changed {
                        account_id: account.id,
                        folder: FolderKind::Sent,
                    });
                }
            }
            Err(e) => self.emit(MailEvent::Error {
                account_id: Some(account.id),
                error: Error::new(
                    e.code,
                    format!("sent, but saving a copy to Sent failed: {}", e.message),
                ),
            }),
        }
        Ok(())
    }
}

fn is_gmail(a: &Account) -> bool {
    a.auth
        == (Auth::OAuth {
            provider: OAuthProvider::Google,
        })
        || a.provider.as_deref() == Some("gmail")
        || a.imap.host.eq_ignore_ascii_case("imap.gmail.com")
        || a.imap.host.eq_ignore_ascii_case("imap.googlemail.com")
}

fn login<'a>(account: &'a Account, cred: &'a Credential) -> Login<'a> {
    match cred {
        Credential::Password(p) => Login::Password {
            username: &account.username,
            password: p,
        },
        Credential::OAuth(t) => Login::XOAuth2 {
            username: &account.username,
            token: t,
        },
    }
}

fn smtp_auth(account: &Account, cred: &Credential) -> SmtpAuth {
    match cred {
        Credential::Password(p) => SmtpAuth::Password {
            username: account.username.clone(),
            password: p.clone(),
        },
        Credential::OAuth(t) => SmtpAuth::XOAuth2 {
            username: account.username.clone(),
            token: t.clone(),
        },
    }
}

/// Holds only a weak reference while waiting, so dropping the last `Engine` ends the loop.
async fn account_loop(inner: Weak<Inner>, account_id: AccountId) {
    let mut backoff = Duration::from_secs(5);
    loop {
        let Some(engine) = inner.upgrade().map(|inner| Engine { inner }) else {
            return;
        };
        let account = match engine.account(&account_id).await {
            Ok(a) => a,
            Err(_) => return,
        };
        let idle = match engine.sync_folders(&account, None).await {
            Ok(()) => engine.connect(&account).await,
            Err(e) => Err(e),
        };
        let conn = match idle {
            Ok(c) => {
                backoff = Duration::from_secs(5);
                c
            }
            Err(e) => {
                tracing::warn!(account = %account_id, code = ?e.code, "sync failed: {}", e.message);
                if e.code == ErrorCode::Auth {
                    // A wrong password will not fix itself; retry rarely.
                    backoff = MAX_BACKOFF;
                }
                drop(engine);
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };
        drop(engine);
        if !conn.idle {
            drop(conn);
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        }
        if let Err(e) = idle_loop(&inner, &account, conn).await {
            tracing::info!(account = %account_id, "IDLE ended, reconnecting: {}", e.message);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

async fn idle_loop(inner: &Weak<Inner>, account: &Account, conn: Conn) -> Result<()> {
    use async_imap::extensions::idle::IdleResponse;
    use async_imap::types::UnsolicitedResponse;

    let mut session = conn.session;
    session.select("INBOX").await?;
    let mut only = Some(FolderKind::Inbox);
    loop {
        {
            let Some(engine) = inner.upgrade().map(|inner| Engine { inner }) else {
                return Ok(());
            };
            // Errors are already reported as SyncState events; IDLE itself keeps going.
            let _ = engine.sync_folders(account, only).await;
        }
        // Mail that arrived while the other connection synced shows up here as EXISTS; catch it
        // before idling instead of waiting for the next wake-up.
        session.noop().await?;
        let mut arrived = false;
        while let Ok(r) = session.unsolicited_responses.try_recv() {
            arrived |= matches!(
                r,
                UnsolicitedResponse::Exists(_) | UnsolicitedResponse::Expunge(_)
            );
        }
        if arrived {
            only = Some(FolderKind::Inbox);
            continue;
        }
        let mut handle = session.idle();
        handle.init().await?;
        let response = {
            let (wait, _stop) = handle.wait_with_timeout(IDLE_RENEW);
            wait.await?
        };
        session = handle.done().await?;
        only = match response {
            IdleResponse::Timeout => None,
            _ => Some(FolderKind::Inbox),
        };
    }
}
