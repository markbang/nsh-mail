//! The facade the app (local mode) and the server use. One `Engine` per user of a mail database:
//! the app's single user, or each user of the server's shared database. Cheap to clone; all
//! methods are `Send` futures on tokio.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use async_imap::extensions::idle::IdleResponse;
use async_imap::imap_proto::Response;
use async_imap::types::UnsolicitedResponse;
use tokio::sync::{OwnedMutexGuard, broadcast};
use tokio::task::AbortHandle;

use crate::error::err;
use crate::imap::{self, Conn, Login};
use crate::mime;
use crate::oauth::{self, LoopbackAuthorization, OAuthApps, OAuthGrant, OAuthTokens};
use crate::provider;
use crate::secrets::SecretStore;
use crate::smtp::{self, SmtpAuth};
use crate::store::{LOCAL_USER, Located, MailDb, Store, StoredMessage, new_id};
use crate::types::*;

mod outbox;

const IDLE_RENEW: Duration = Duration::from_secs(29 * 60);
/// Servers may hold back changes made by other clients until IDLE ends (GreenMail sends flag
/// changes only on DONE or with the next EXISTS/EXPUNGE); restarting IDLE this often bounds that
/// delay. A restart that brings nothing new costs one round trip and no sync.
const IDLE_CHECK: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_secs(2 * 60);
const OP_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// A pooled connection unused for longer than this is checked with NOOP before reuse.
const STALE_AFTER: Duration = Duration::from_secs(60);
const MIN_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(5 * 60);

pub struct EngineOptions {
    pub db: MailDb,
    /// Whose mail in `db` this engine reads and syncs.
    pub user_id: i64,
    pub secrets: Arc<dyn SecretStore>,
    pub oauth: OAuthApps,
    /// The most a message's attachments may add up to, in bytes ([`MAX_ATTACHMENTS`] by default).
    pub max_attachments: u64,
}

/// The bytes of a draft's `upload` attachment, staged by the caller.
pub struct StagedFile {
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
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
    max_attachments: u64,
    http: reqwest::Client,
    events: broadcast::Sender<MailEvent>,
    /// One command connection per account, shared by sync, body fetch, flags and append. Holding
    /// its lock also serializes syncs of an account.
    pool: Mutex<HashMap<AccountId, Slot>>,
    /// `Some` once `run` was called: the per-account IDLE loops.
    loops: Mutex<Option<HashMap<AccountId, AbortHandle>>>,
    /// The outbox's sender, once `run` was called, and what wakes it when a message is queued.
    outbox_task: Mutex<Option<AbortHandle>>,
    outbox_wake: Arc<tokio::sync::Notify>,
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
        if let Some(task) = self
            .outbox_task
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            task.abort();
        }
    }
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
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
    /// Opens (or creates) the single-user database at `db_path` with the build's OAuth client ids.
    pub fn open(db_path: impl AsRef<Path>, secrets: Arc<dyn SecretStore>) -> Result<Self> {
        Self::open_with(EngineOptions {
            db: MailDb::open(db_path.as_ref())?,
            user_id: LOCAL_USER,
            secrets,
            oauth: OAuthApps::builtin(),
            max_attachments: MAX_ATTACHMENTS,
        })
    }

    pub fn open_with(options: EngineOptions) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Inner {
                store: Arc::new(options.db.user(options.user_id)),
                secrets: options.secrets,
                oauth: options.oauth,
                max_attachments: options.max_attachments,
                http: crate::tls::http_client(),
                events: broadcast::channel(256).0,
                pool: Mutex::new(HashMap::new()),
                loops: Mutex::new(None),
                outbox_task: Mutex::new(None),
                outbox_wake: Arc::new(tokio::sync::Notify::new()),
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

    /// The account's pooled connection, and whether it was reused rather than opened now.
    async fn pooled(&self, account: &Account) -> Result<(OwnedMutexGuard<Option<Pooled>>, bool)> {
        let mut g = self.slot(&account.id).lock_owned().await;
        if let Some(p) = g.as_mut()
            && p.last_used.elapsed() > STALE_AFTER
            && p.conn.session.noop().await.is_err()
        {
            *g = None;
        }
        let reused = g.is_some();
        if !reused {
            *g = Some(Pooled {
                conn: self.connect(account).await?,
                last_used: Instant::now(),
            });
        }
        Ok((g, reused))
    }

    /// Runs `f` on the account's pooled connection; a failed operation discards the connection
    /// so the next one reconnects. Servers close connections that sat unused (GreenMail after
    /// 30 s), so when a reused connection turns out to be closed, `f` runs once more on a new one.
    /// Every operation passed here is safe to repeat; other errors are never retried.
    async fn with_imap<T>(
        &self,
        account: &Account,
        f: impl AsyncFnOnce(&mut Conn) -> Result<T> + Clone,
    ) -> Result<T> {
        let (mut g, reused) = self.pooled(account).await?;
        let pooled = g.as_mut().expect("connected above");
        let mut r = tokio::time::timeout(OP_TIMEOUT, f.clone()(&mut pooled.conn)).await;
        if matches!(r, Ok(Err(_)))
            && reused
            && g.as_ref()
                .is_some_and(|p| p.conn.session.get_ref().closed())
        {
            // Cleared first so a failed reconnect leaves no dead connection in the pool.
            *g = None;
            *g = Some(Pooled {
                conn: self.connect(account).await?,
                last_used: Instant::now(),
            });
            let pooled = g.as_mut().expect("connected above");
            r = tokio::time::timeout(OP_TIMEOUT, f(&mut pooled.conn)).await;
        }
        let r = r.unwrap_or_else(|e| Err(e.into()));
        match &r {
            Err(e) if e.code != ErrorCode::NotFound => *g = None,
            _ => {
                if let Some(p) = g.as_mut() {
                    p.last_used = Instant::now();
                }
            }
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
    /// Accounts added later start automatically. Also starts sending the outbox: what came due
    /// while nothing ran goes at once. Idempotent.
    pub async fn run(&self) -> Result<()> {
        let accounts = self.accounts().await?;
        lock(&self.inner.loops).get_or_insert_with(HashMap::new);
        for a in accounts {
            self.start_loop(&a.id);
        }
        let mut task = lock(&self.inner.outbox_task);
        if task.as_ref().is_none_or(|t| t.is_finished()) {
            let handle = tokio::spawn(outbox::outbox_loop(Arc::downgrade(&self.inner)));
            *task = Some(handle.abort_handle());
        }
        Ok(())
    }

    /// Stops the background loops started by `run`.
    pub fn stop(&self) {
        if let Some(loops) = lock(&self.inner.loops).take() {
            loops.values().for_each(AbortHandle::abort);
        }
        if let Some(task) = lock(&self.inner.outbox_task).take() {
            task.abort();
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

    pub async fn threads(&self, query: ListQuery) -> Result<Vec<Thread>> {
        self.store().run(move |s| s.threads(&query)).await
    }

    /// A conversation's messages, oldest first; open each with [`get`](Self::get).
    pub async fn thread(&self, id: &str) -> Result<Vec<Summary>> {
        let id = id.to_string();
        self.store().run(move |s| s.thread(&id)).await
    }

    pub async fn unread(&self) -> Result<HashMap<AccountId, u32>> {
        self.store().run(|s| s.unread()).await
    }

    pub async fn search(&self, text: &str, limit: u32) -> Result<Vec<Summary>> {
        let text = text.to_string();
        self.store().run(move |s| s.search(&text, limit)).await
    }

    /// The full message; the body is downloaded on first open and cached.
    pub async fn get(&self, id: &str) -> Result<Message> {
        let (mut stored, raw) = self.body(id).await?;
        let body = tokio::task::spawn_blocking(move || mime::parse_body(&raw)).await?;
        let has_attachments = !body.attachments.is_empty();
        // Rows synced before inline images stopped counting are corrected as they are opened.
        if stored.summary.has_attachments != has_attachments {
            let mid = id.to_string();
            self.store()
                .run(move |s| s.set_has_attachments(&mid, has_attachments))
                .await?;
            stored.summary.has_attachments = has_attachments;
            self.emit(MailEvent::Changed {
                account_id: stored.summary.account_id.clone(),
                folder: stored.summary.folder,
            });
        }
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

    /// Attachment `index` of a message (`Message::attachments`) and its decoded bytes; the
    /// body is downloaded and cached as `get` does.
    pub async fn attachment(&self, id: &str, index: u32) -> Result<(Attachment, Vec<u8>)> {
        let (_, raw) = self.body(id).await?;
        tokio::task::spawn_blocking(move || mime::attachment(&raw, index))
            .await?
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::NotFound,
                    format!("no attachment {index} in this message"),
                )
            })
    }

    /// The stored message and its raw body, downloaded and cached on first use.
    async fn body(&self, id: &str) -> Result<(StoredMessage, Vec<u8>)> {
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
                let (text, has_attachments) = mime::index(&raw);
                let (mid, raw2) = (id.to_string(), raw.clone());
                self.store()
                    .run(move |s| s.set_body(&mid, &raw2, &text, has_attachments))
                    .await?;
                raw
            }
        };
        Ok((stored, raw))
    }

    /// Writes flags to the server, then to the local copy.
    pub async fn set_flags(&self, id: &str, flags: Flags) -> Result<()> {
        let mid = id.to_string();
        let message = self.store().run(move |s| s.located(&mid)).await?;
        self.store_flags(vec![message], flags).await
    }

    /// Marks a whole conversation read or unread; flagging flags its latest message (as Gmail
    /// stars one), unflagging clears every flag in it.
    pub async fn set_thread_flags(&self, id: &str, flags: Flags) -> Result<()> {
        let tid = id.to_string();
        let copies = self.store().run(move |s| s.thread_copies(&tid)).await?;
        if let Some(seen) = flags.seen {
            let changed = copies.iter().filter(|m| m.seen != seen).cloned().collect();
            self.store_flags(
                changed,
                Flags {
                    seen: Some(seen),
                    flagged: None,
                },
            )
            .await?;
        }
        if let Some(flagged) = flags.flagged {
            let targets: Vec<Located> = if flagged {
                let shown = copies.iter().filter(|m| m.folder != FolderKind::Trash);
                shown.max_by_key(|m| m.date).into_iter().cloned().collect()
            } else {
                copies.iter().filter(|m| m.flagged).cloned().collect()
            };
            self.store_flags(
                targets,
                Flags {
                    seen: None,
                    flagged: Some(flagged),
                },
            )
            .await?;
        }
        Ok(())
    }

    /// Writes flags to the server, one command per folder, then to the local copies.
    async fn store_flags(&self, messages: Vec<Located>, flags: Flags) -> Result<()> {
        for (account_id, folders) in by_account(messages) {
            let account = self.account(&account_id).await?;
            let sets: Vec<(String, String)> = folders
                .iter()
                .map(|(name, ms)| (name.clone(), imap::uid_set(ms.iter().map(|m| m.uid))))
                .collect();
            self.with_imap(&account, async |conn| {
                for (folder, uids) in &sets {
                    imap::store_flags(&mut conn.session, folder, uids, flags).await?;
                }
                Ok(())
            })
            .await?;
            let ids: Vec<String> = folders
                .iter()
                .flat_map(|(_, ms)| ms.iter().map(|m| m.id.clone()))
                .collect();
            self.store()
                .run(move |s| ids.iter().try_for_each(|id| s.set_local_flags(id, flags)))
                .await?;
            for kind in kinds(&folders) {
                self.emit(MailEvent::Changed {
                    account_id: account.id.clone(),
                    folder: kind,
                });
            }
        }
        Ok(())
    }

    /// Moves a message to another folder of its account (archive, delete to Trash), creating
    /// Archive or Trash when the server has none. The message leaves its folder here at once;
    /// it shows in the other folder once that one syncs, which this starts.
    pub async fn move_to(&self, id: &str, to: FolderKind) -> Result<()> {
        let mid = id.to_string();
        let message = self.store().run(move |s| s.located(&mid)).await?;
        if message.folder == to {
            return Ok(());
        }
        self.move_messages(vec![message], to).await
    }

    /// Archives a conversation (its messages in the inbox go to Archive) or deletes it (all its
    /// messages go to Trash, the replies in Sent too, as Gmail does).
    pub async fn move_thread(&self, id: &str, to: FolderKind) -> Result<()> {
        let tid = id.to_string();
        let copies = self.store().run(move |s| s.thread_copies(&tid)).await?;
        let mut moving: Vec<Located> = match to {
            FolderKind::Archive => copies
                .into_iter()
                .filter(|m| m.folder == FolderKind::Inbox)
                .collect(),
            FolderKind::Trash => copies
                .into_iter()
                .filter(|m| m.folder != FolderKind::Trash)
                .collect(),
            _ => {
                return Err(err(
                    ErrorCode::InvalidInput,
                    "a conversation moves to archive or trash",
                ));
            }
        };
        // Gmail's All Mail holds a copy of everything: moving the others first takes those
        // copies along, and what is left of All Mail goes last.
        moving.sort_by_key(|m| m.folder == FolderKind::Archive);
        self.move_messages(moving, to).await
    }

    async fn move_messages(&self, messages: Vec<Located>, to: FolderKind) -> Result<()> {
        for (account_id, folders) in by_account(messages) {
            let account = self.account(&account_id).await?;
            let gmail = is_gmail(&account);
            let store = self.store().clone();
            let sets: Vec<(String, String)> = folders
                .iter()
                .map(|(name, ms)| (name.clone(), imap::uid_set(ms.iter().map(|m| m.uid))))
                .collect();
            let target = self
                .with_imap(&account, async |conn| {
                    let id = account_id.clone();
                    let mut target = store.run(move |s| s.folder(&id, to)).await?;
                    if target.is_none() {
                        let mut found = imap::discover(&mut conn.session).await?;
                        let name = match to {
                            FolderKind::Archive => "Archive",
                            FolderKind::Trash => "Trash",
                            _ => "",
                        };
                        if !found.iter().any(|(k, _)| *k == to) && !gmail && !name.is_empty() {
                            imap::create(&mut conn.session, name).await?;
                            found.push((to, name.to_string()));
                        }
                        let id = account_id.clone();
                        target = store
                            .run(move |s| {
                                s.set_folders(&id, &found)?;
                                s.folder(&id, to)
                            })
                            .await?;
                    }
                    let target = target.ok_or_else(|| {
                        err(
                            ErrorCode::Unsupported,
                            format!(
                                "{} has no {to:?} folder to move the message to",
                                account.email
                            ),
                        )
                    })?;
                    for (folder, uids) in &sets {
                        if *folder != target.name {
                            imap::move_messages(conn, folder, uids, &target.name).await?;
                        }
                    }
                    Ok(target)
                })
                .await?;
            let ids: Vec<String> = folders
                .iter()
                .filter(|(name, _)| *name != target.name)
                .flat_map(|(_, ms)| ms.iter().map(|m| m.id.clone()))
                .collect();
            self.store()
                .run(move |s| ids.iter().try_for_each(|id| s.delete_message(id)))
                .await?;
            for kind in kinds(&folders) {
                self.emit(MailEvent::Changed {
                    account_id: account.id.clone(),
                    folder: kind,
                });
            }
            let store = self.store().clone();
            let synced = self
                .with_imap(&account, async |conn| {
                    imap::sync_folder(conn, &store, target).await
                })
                .await;
            if synced.is_ok_and(|r| r.changed) {
                self.emit(MailEvent::Changed {
                    account_id: account.id,
                    folder: to,
                });
            }
        }
        Ok(())
    }

    /// Sends over SMTP, then saves a copy to Sent (Gmail saves it itself). Once SMTP accepted the
    /// message this returns `Ok`; a failure to save the copy is reported as an `Error` event,
    /// so callers never resend a delivered message.
    pub async fn send(&self, draft: Draft) -> Result<()> {
        self.send_with(draft, HashMap::new()).await
    }

    /// [`send`](Self::send) with attachments: a draft's `upload`s come from `staged` by id, its
    /// `forward`s from the store.
    pub async fn send_with(
        &self,
        draft: Draft,
        mut staged: HashMap<String, StagedFile>,
    ) -> Result<()> {
        let account = self.account(&draft.account_id).await?;
        let mut files = Vec::with_capacity(draft.attachments.len());
        let mut total = 0u64;
        for attachment in &draft.attachments {
            let file = match attachment {
                DraftAttachment::Upload { id } => staged.remove(id).ok_or_else(|| {
                    err(
                        ErrorCode::NotFound,
                        "an attached file is gone (it expires a day after it was added); attach it again",
                    )
                })?,
                DraftAttachment::Forward { message_id, index } => {
                    let (meta, data) = self.attachment(message_id, *index).await?;
                    StagedFile {
                        filename: meta.filename.unwrap_or_else(|| "attachment".into()),
                        mime_type: meta.mime_type,
                        data,
                    }
                }
            };
            total += file.data.len() as u64;
            if total > self.inner.max_attachments {
                return Err(too_large(self.inner.max_attachments));
            }
            files.push(file);
        }
        let message = smtp::build(&account, &draft, files)?;
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

type Folders = Vec<(String, Vec<Located>)>;

/// Messages by account, then by server folder, in the order given.
fn by_account(messages: Vec<Located>) -> Vec<(AccountId, Folders)> {
    let mut out: Vec<(AccountId, Folders)> = Vec::new();
    for m in messages {
        let at = match out.iter().position(|(a, _)| *a == m.account_id) {
            Some(at) => at,
            None => {
                out.push((m.account_id.clone(), Vec::new()));
                out.len() - 1
            }
        };
        let folders = &mut out[at].1;
        match folders.iter_mut().find(|(name, _)| *name == m.folder_name) {
            Some((_, ms)) => ms.push(m),
            None => folders.push((m.folder_name.clone(), vec![m])),
        }
    }
    out
}

fn kinds(folders: &Folders) -> Vec<FolderKind> {
    let mut out: Vec<FolderKind> = Vec::new();
    for kind in folders
        .iter()
        .filter_map(|(_, ms)| ms.first().map(|m| m.folder))
    {
        if !out.contains(&kind) {
            out.push(kind);
        }
    }
    out
}

pub(crate) fn too_large(limit: u64) -> Error {
    err(
        ErrorCode::TooLarge,
        format!(
            "attachments can add up to {} MB; remove some or share a link instead",
            limit / (1024 * 1024)
        ),
    )
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

fn upgrade(inner: &Weak<Inner>) -> Option<Engine> {
    inner.upgrade().map(|inner| Engine { inner })
}

/// Holds only a weak reference while waiting, so dropping the last `Engine` ends the loop.
async fn account_loop(inner: Weak<Inner>, account_id: AccountId) {
    let mut backoff = MIN_BACKOFF;
    loop {
        let started = Instant::now();
        let e = match watch(&inner, &account_id).await {
            Ok(()) => return,
            Err(e) => e,
        };
        // A session that stayed up this long was healthy: start the backoff over.
        if started.elapsed() > MAX_BACKOFF {
            backoff = MIN_BACKOFF;
        }
        if e.code == ErrorCode::Auth {
            // A wrong password will not fix itself; retry rarely.
            backoff = MAX_BACKOFF;
        }
        tracing::warn!(
            account = %account_id,
            code = ?e.code,
            "sync stopped, reconnecting in {}s: {}",
            backoff.as_secs(),
            e.message
        );
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

/// One session: a full sync, then IDLE on INBOX, or polling without IDLE, until something fails.
/// `Ok` means the engine or the account is gone.
async fn watch(inner: &Weak<Inner>, account_id: &str) -> Result<()> {
    let (account, conn) = {
        let Some(engine) = upgrade(inner) else {
            return Ok(());
        };
        let account = match engine.account(account_id).await {
            Err(e) if e.code == ErrorCode::NotFound => return Ok(()),
            a => a?,
        };
        engine.sync_folders(&account, None).await?;
        let conn = engine.connect(&account).await?;
        (account, conn)
    };
    if conn.idle {
        return idle_loop(inner, &account, conn).await;
    }
    drop(conn);
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        let Some(engine) = upgrade(inner) else {
            return Ok(());
        };
        engine.sync_folders(&account, None).await?;
    }
}

/// Whether an untagged response means INBOX changed: a new EXISTS count (servers may repeat it),
/// an EXPUNGE, or a FETCH with flags changed by another client.
fn changed(r: UnsolicitedResponse, exists: &mut u32) -> bool {
    match r {
        UnsolicitedResponse::Exists(n) => std::mem::replace(exists, n) != n,
        UnsolicitedResponse::Expunge(_) => {
            *exists = exists.saturating_sub(1);
            true
        }
        UnsolicitedResponse::Other(d) => matches!(d.parsed(), Response::Fetch(..)),
        _ => false,
    }
}

async fn idle_loop(inner: &Weak<Inner>, account: &Account, conn: Conn) -> Result<()> {
    let mut session = conn.session;
    let mut exists = tokio::time::timeout(OP_TIMEOUT, session.select("INBOX"))
        .await??
        .exists;
    // Untagged responses outside a command's own replies land here: what the server reports
    // while IDLE starts (before `+ idling`), which `Handle::wait` never sees, and on DONE.
    let unsolicited = session.unsolicited_responses.clone();
    let drain = |exists: &mut u32| {
        let mut any = false;
        while let Ok(r) = unsolicited.try_recv() {
            any |= changed(r, exists);
        }
        any
    };
    let mut full_sync_at = Instant::now() + IDLE_RENEW;
    let mut sync = true;
    loop {
        if sync {
            // The sync below covers everything reported so far.
            drain(&mut exists);
            let Some(engine) = upgrade(inner) else {
                return Ok(());
            };
            let only = if Instant::now() >= full_sync_at {
                full_sync_at = Instant::now() + IDLE_RENEW;
                None
            } else {
                Some(FolderKind::Inbox)
            };
            engine.sync_folders(account, only).await?;
        }
        let mut handle = session.idle();
        tokio::time::timeout(OP_TIMEOUT, handle.init()).await??;
        let mut arrived = drain(&mut exists);
        let response = if arrived {
            None
        } else {
            let wait = IDLE_CHECK.min(full_sync_at.saturating_duration_since(Instant::now()));
            let (wait, _stop) = handle.wait_with_timeout(wait);
            Some(wait.await?)
        };
        session = tokio::time::timeout(OP_TIMEOUT, handle.done()).await??;
        arrived |= drain(&mut exists);
        sync = arrived
            || !matches!(response, Some(IdleResponse::Timeout))
            || Instant::now() >= full_sync_at;
    }
}
