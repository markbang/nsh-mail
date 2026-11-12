use std::{collections::HashMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use tokio::sync::{Mutex, broadcast};

use crate::{
    crypto::Cipher,
    db::Db,
    types::{
        Account, Draft, Error, ErrorCode, Flags, ImapInput, ListQuery, MailEvent, Message,
        OAuthProvider, Result, Summary,
    },
};

/// OAuth tokens from the server-side web flow, handed to the mail backend.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix seconds.
    pub expires_at: Option<i64>,
}

/// One user's mail: the seam where mail-core's `Engine` plugs in.
#[async_trait]
pub trait UserMail: Send + Sync {
    async fn accounts(&self) -> Result<Vec<Account>>;
    async fn add_imap(&self, input: ImapInput) -> Result<Account>;
    async fn add_oauth(
        &self,
        provider: OAuthProvider,
        email: String,
        tokens: OAuthTokens,
    ) -> Result<Account>;
    async fn remove_account(&self, id: &str) -> Result<()>;
    async fn list(&self, query: ListQuery) -> Result<Vec<Summary>>;
    async fn get(&self, id: &str) -> Result<Message>;
    async fn set_flags(&self, id: &str, flags: Flags) -> Result<()>;
    async fn send(&self, draft: Draft) -> Result<()>;
    async fn search(&self, text: &str, limit: u32) -> Result<Vec<Summary>>;
    fn subscribe(&self) -> broadcast::Receiver<MailEvent>;
}

/// Account credentials, persisted encrypted in server.sqlite.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

pub struct DbSecrets {
    pub db: Db,
    pub cipher: Cipher,
    pub user_id: i64,
}

impl DbSecrets {
    fn aad(&self, key: &str) -> Vec<u8> {
        format!("{}:{key}", self.user_id).into_bytes()
    }
}

fn secrets_err(e: anyhow::Error) -> Error {
    Error::new(ErrorCode::Secrets, format!("{e:#}"))
}

impl SecretStore for DbSecrets {
    fn get(&self, key: &str) -> Result<Option<String>> {
        let Some(sealed) = self.db.get_secret(self.user_id, key).map_err(secrets_err)? else {
            return Ok(None);
        };
        let plain = self
            .cipher
            .open(&sealed, &self.aad(key))
            .map_err(secrets_err)?;
        String::from_utf8(plain)
            .map(Some)
            .map_err(|e| secrets_err(e.into()))
    }
    fn set(&self, key: &str, value: &str) -> Result<()> {
        let sealed = self.cipher.seal(value.as_bytes(), &self.aad(key));
        self.db
            .set_secret(self.user_id, key, &sealed)
            .map_err(secrets_err)
    }
    fn delete(&self, key: &str) -> Result<()> {
        self.db
            .delete_secret(self.user_id, key)
            .map_err(secrets_err)
    }
}

/// Opens (and starts syncing) one user's mail.
#[async_trait]
pub trait MailBackend: Send + Sync {
    async fn open(
        &self,
        user_id: i64,
        db_path: PathBuf,
        secrets: Arc<dyn SecretStore>,
    ) -> anyhow::Result<Arc<dyn UserMail>>;
}

pub type OnOpen = Box<dyn Fn(i64, Arc<dyn UserMail>) + Send + Sync>;

/// Lazily opened, then kept for the process lifetime so sync keeps running.
pub struct Mailboxes {
    backend: Arc<dyn MailBackend>,
    db: Db,
    cipher: Cipher,
    users_dir: PathBuf,
    open: Mutex<HashMap<i64, Arc<dyn UserMail>>>,
    on_open: std::sync::OnceLock<OnOpen>,
}

impl Mailboxes {
    pub fn new(backend: Arc<dyn MailBackend>, db: Db, cipher: Cipher, users_dir: PathBuf) -> Self {
        Mailboxes {
            backend,
            db,
            cipher,
            users_dir,
            open: Mutex::default(),
            on_open: Default::default(),
        }
    }

    pub fn on_open(&self, f: OnOpen) {
        let _ = self.on_open.set(f);
    }

    pub async fn get(&self, user_id: i64) -> anyhow::Result<Arc<dyn UserMail>> {
        let mut open = self.open.lock().await;
        if let Some(m) = open.get(&user_id) {
            return Ok(m.clone());
        }
        let secrets = Arc::new(DbSecrets {
            db: self.db.clone(),
            cipher: self.cipher.clone(),
            user_id,
        });
        let mail = self
            .backend
            .open(
                user_id,
                self.users_dir.join(format!("{user_id}.sqlite")),
                secrets,
            )
            .await?;
        if let Some(f) = self.on_open.get() {
            f(user_id, mail.clone());
        }
        open.insert(user_id, mail.clone());
        Ok(mail)
    }
}

pub mod fake {
    //! In-memory backend for tests until mail-core is wired; accepts any IMAP password except "wrong".

    use std::sync::Mutex as StdMutex;

    use super::*;
    use crate::types::{Address, Auth, Endpoint, FolderKind, Security};

    fn not_found() -> Error {
        Error::new(ErrorCode::NotFound, "message not found")
    }

    #[derive(Default)]
    pub struct FakeBackend {
        users: StdMutex<Vec<Arc<FakeMail>>>,
    }

    impl FakeBackend {
        /// Simulates new mail arriving in an account's INBOX.
        pub fn deliver(&self, account_id: &str, from: &str, subject: &str) -> Option<Summary> {
            let users = self.users.lock().unwrap();
            let mail = users.iter().find(|m| {
                m.state
                    .lock()
                    .unwrap()
                    .accounts
                    .iter()
                    .any(|a| a.id == account_id)
            })?;
            let mut st = mail.state.lock().unwrap();
            st.seq += 1;
            let summary = Summary {
                id: format!("{account_id}-{}", st.seq),
                account_id: account_id.into(),
                folder: FolderKind::Inbox,
                from: Some(Address {
                    name: None,
                    email: from.into(),
                }),
                to: vec![],
                subject: subject.into(),
                snippet: String::new(),
                date: st.seq as i64,
                seen: false,
                flagged: false,
                has_attachments: false,
            };
            st.messages.push(summary.clone());
            let _ = mail.events.send(MailEvent::NewMail {
                summary: summary.clone(),
            });
            Some(summary)
        }
    }

    #[async_trait]
    impl MailBackend for FakeBackend {
        async fn open(
            &self,
            _user_id: i64,
            _db: PathBuf,
            secrets: Arc<dyn SecretStore>,
        ) -> anyhow::Result<Arc<dyn UserMail>> {
            let mail = Arc::new(FakeMail {
                secrets,
                events: broadcast::channel(64).0,
                state: Default::default(),
            });
            self.users.lock().unwrap().push(mail.clone());
            Ok(mail)
        }
    }

    #[derive(Default)]
    struct State {
        seq: u64,
        accounts: Vec<Account>,
        messages: Vec<Summary>,
    }

    pub struct FakeMail {
        secrets: Arc<dyn SecretStore>,
        events: broadcast::Sender<MailEvent>,
        state: StdMutex<State>,
    }

    impl FakeMail {
        fn add(
            &self,
            email: String,
            name: Option<String>,
            auth: Auth,
            secret: &str,
        ) -> Result<Account> {
            let endpoint = |host: &str| Endpoint {
                host: host.into(),
                port: 993,
                security: Security::Tls,
            };
            let mut st = self.state.lock().unwrap();
            st.seq += 1;
            let account = Account {
                id: format!("a{}", st.seq),
                username: email.clone(),
                email,
                name,
                provider: None,
                auth,
                imap: endpoint("imap.example.com"),
                smtp: endpoint("smtp.example.com"),
            };
            self.secrets.set(&account.id, secret)?;
            st.accounts.push(account.clone());
            Ok(account)
        }
    }

    #[async_trait]
    impl UserMail for FakeMail {
        async fn accounts(&self) -> Result<Vec<Account>> {
            Ok(self.state.lock().unwrap().accounts.clone())
        }
        async fn add_imap(&self, input: ImapInput) -> Result<Account> {
            if input.password == "wrong" {
                return Err(Error::new(
                    ErrorCode::Auth,
                    "IMAP login failed: authentication rejected",
                ));
            }
            self.add(input.email, input.name, Auth::Password, &input.password)
        }
        async fn add_oauth(
            &self,
            provider: OAuthProvider,
            email: String,
            tokens: OAuthTokens,
        ) -> Result<Account> {
            let secret = serde_json::to_string(&tokens)
                .map_err(|e| Error::new(ErrorCode::Internal, e.to_string()))?;
            self.add(email, None, Auth::OAuth { provider }, &secret)
        }
        async fn remove_account(&self, id: &str) -> Result<()> {
            let mut st = self.state.lock().unwrap();
            let before = st.accounts.len();
            st.accounts.retain(|a| a.id != id);
            if st.accounts.len() == before {
                return Err(Error::new(ErrorCode::NotFound, "account not found"));
            }
            st.messages.retain(|m| m.account_id != id);
            self.secrets.delete(id)?;
            Ok(())
        }
        async fn list(&self, q: ListQuery) -> Result<Vec<Summary>> {
            let st = self.state.lock().unwrap();
            let before = match &q.before {
                Some(id) => Some(
                    st.messages
                        .iter()
                        .find(|m| &m.id == id)
                        .ok_or_else(not_found)?
                        .date,
                ),
                None => None,
            };
            let mut out: Vec<_> = st
                .messages
                .iter()
                .filter(|m| m.folder == q.folder)
                .filter(|m| q.account.as_ref().is_none_or(|a| &m.account_id == a))
                .filter(|m| before.is_none_or(|b| m.date < b))
                .cloned()
                .collect();
            out.sort_by_key(|m| std::cmp::Reverse(m.date));
            out.truncate(q.limit as usize);
            Ok(out)
        }
        async fn get(&self, id: &str) -> Result<Message> {
            let st = self.state.lock().unwrap();
            let summary = st
                .messages
                .iter()
                .find(|m| m.id == id)
                .cloned()
                .ok_or_else(not_found)?;
            Ok(Message {
                summary,
                cc: vec![],
                reply_to: vec![],
                message_id: None,
                in_reply_to: None,
                references: vec![],
                text: Some(String::new()),
                html: None,
                remote_images: vec![],
                attachments: vec![],
            })
        }
        async fn set_flags(&self, id: &str, flags: Flags) -> Result<()> {
            let mut st = self.state.lock().unwrap();
            let m = st
                .messages
                .iter_mut()
                .find(|m| m.id == id)
                .ok_or_else(not_found)?;
            m.seen = flags.seen.unwrap_or(m.seen);
            m.flagged = flags.flagged.unwrap_or(m.flagged);
            let ev = MailEvent::Changed {
                account_id: m.account_id.clone(),
                folder: m.folder,
            };
            let _ = self.events.send(ev);
            Ok(())
        }
        async fn send(&self, draft: Draft) -> Result<()> {
            let st = self.state.lock().unwrap();
            if !st.accounts.iter().any(|a| a.id == draft.account_id) {
                return Err(Error::new(
                    ErrorCode::InvalidInput,
                    format!("unknown account {}", draft.account_id),
                ));
            }
            Ok(())
        }
        async fn search(&self, text: &str, limit: u32) -> Result<Vec<Summary>> {
            let text = text.to_lowercase();
            let st = self.state.lock().unwrap();
            Ok(st
                .messages
                .iter()
                .filter(|m| m.subject.to_lowercase().contains(&text))
                .take(limit as usize)
                .cloned()
                .collect())
        }
        fn subscribe(&self) -> broadcast::Receiver<MailEvent> {
            self.events.subscribe()
        }
    }
}
