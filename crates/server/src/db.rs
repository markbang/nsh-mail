use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY,
    email TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    hide_content INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
    token_hash BLOB PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    token TEXT NOT NULL,
    p256dh TEXT,
    auth TEXT,
    created_at INTEGER NOT NULL,
    vapid_key TEXT,
    UNIQUE (user_id, token)
);
CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    hash BLOB NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    scopes TEXT NOT NULL DEFAULT 'read',
    source_ids TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    last_used_at INTEGER,
    revoked_at INTEGER
);
CREATE INDEX IF NOT EXISTS api_keys_by_user ON api_keys (user_id, created_at);
CREATE TABLE IF NOT EXISTS oauth_states (
    state_hash BLOB PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    verifier TEXT NOT NULL,
    return_to TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
";

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// server.sqlite: users, sessions, devices, pending OAuth states.
/// Queries are short, so a single connection behind a mutex is enough for one instance.
#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

pub struct User {
    pub id: i64,
    pub email: String,
    pub password_hash: String,
    pub hide_content: bool,
}

pub struct Device {
    pub id: String,
    pub kind: String,
    pub token: String,
    pub p256dh: Option<String>,
    pub auth: Option<String>,
}

pub struct OAuthState {
    pub user_id: i64,
    pub provider: String,
    pub verifier: String,
    pub return_to: String,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn =
            Connection::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;",
        )?;
        conn.execute_batch(SCHEMA)?;
        let has_vapid_key: bool = conn.query_row(
            "SELECT count(*) FROM pragma_table_info('devices') WHERE name = 'vapid_key'",
            [],
            |r| r.get(0),
        )?;
        if !has_vapid_key {
            conn.execute_batch("ALTER TABLE devices ADD COLUMN vapid_key TEXT")?;
        }
        Ok(Db(Arc::new(Mutex::new(conn))))
    }

    pub(crate) fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        Ok(f(&conn)?)
    }

    pub fn ping(&self) -> Result<()> {
        self.with(|c| c.query_row("SELECT 1", [], |_| Ok(())))
    }

    /// Returns None when the email is taken.
    pub fn create_user(&self, email: &str, password_hash: &str) -> Result<Option<i64>> {
        self.with(|c| {
            let n = c.execute(
                "INSERT INTO users (email, password_hash, created_at) VALUES (?1, ?2, ?3) ON CONFLICT(email) DO NOTHING",
                params![email, password_hash, now()],
            )?;
            Ok((n == 1).then(|| c.last_insert_rowid()))
        })
    }

    fn user_row(r: &rusqlite::Row) -> rusqlite::Result<User> {
        Ok(User {
            id: r.get(0)?,
            email: r.get(1)?,
            password_hash: r.get(2)?,
            hide_content: r.get(3)?,
        })
    }

    pub fn user_by_email(&self, email: &str) -> Result<Option<User>> {
        self.with(|c| {
            c.query_row(
                "SELECT id, email, password_hash, hide_content FROM users WHERE email = ?1",
                [email],
                Self::user_row,
            )
            .optional()
        })
    }

    pub fn user(&self, id: i64) -> Result<Option<User>> {
        self.with(|c| {
            c.query_row(
                "SELECT id, email, password_hash, hide_content FROM users WHERE id = ?1",
                [id],
                Self::user_row,
            )
            .optional()
        })
    }

    pub fn set_hide_content(&self, user_id: i64, hide: bool) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE users SET hide_content = ?2 WHERE id = ?1",
                params![user_id, hide],
            )
            .map(drop)
        })
    }

    pub fn create_session(&self, token_hash: &[u8], user_id: i64) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO sessions (token_hash, user_id, created_at) VALUES (?1, ?2, ?3)",
                params![token_hash, user_id, now()],
            )
            .map(drop)
        })
    }

    pub fn session_user(&self, token_hash: &[u8]) -> Result<Option<i64>> {
        self.with(|c| {
            c.query_row(
                "SELECT user_id FROM sessions WHERE token_hash = ?1",
                [token_hash],
                |r| r.get(0),
            )
            .optional()
        })
    }

    pub fn delete_session(&self, token_hash: &[u8]) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM sessions WHERE token_hash = ?1", [token_hash])
                .map(drop)
        })
    }

    /// Re-registering the same token returns the existing device id. `vapid_key` is the server's
    /// public key a Web Push subscription was made for.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_device(
        &self,
        user_id: i64,
        id: &str,
        kind: &str,
        token: &str,
        p256dh: Option<&str>,
        auth: Option<&str>,
        vapid_key: Option<&str>,
    ) -> Result<String> {
        self.with(|c| {
            c.query_row(
                "INSERT INTO devices (id, user_id, kind, token, p256dh, auth, vapid_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(user_id, token) DO UPDATE SET kind = excluded.kind, p256dh = excluded.p256dh, auth = excluded.auth, vapid_key = excluded.vapid_key
                 RETURNING id",
                params![id, user_id, kind, token, p256dh, auth, vapid_key, now()],
                |r| r.get(0),
            )
        })
    }

    /// Web Push subscriptions are bound to the VAPID key they were made with; after a key change
    /// the push service rejects them, so they go and the browser subscribes again.
    pub fn drop_stale_webpush(&self, vapid_key: &str) -> Result<usize> {
        self.with(|c| {
            c.execute(
                "DELETE FROM devices WHERE kind = 'webpush' AND (vapid_key IS NULL OR vapid_key != ?1)",
                [vapid_key],
            )
        })
    }

    pub fn devices(&self, user_id: i64) -> Result<Vec<Device>> {
        self.with(|c| {
            let mut stmt =
                c.prepare("SELECT id, kind, token, p256dh, auth FROM devices WHERE user_id = ?1")?;
            stmt.query_map([user_id], |r| {
                Ok(Device {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    token: r.get(2)?,
                    p256dh: r.get(3)?,
                    auth: r.get(4)?,
                })
            })?
            .collect()
        })
    }

    pub fn delete_device(&self, user_id: i64, id: &str) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "DELETE FROM devices WHERE user_id = ?1 AND id = ?2",
                params![user_id, id],
            )
            .map(|n| n > 0)
        })
    }

    pub fn put_oauth_state(&self, state_hash: &[u8], s: &OAuthState) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM oauth_states WHERE created_at < ?1", [now() - 600])?;
            c.execute(
                "INSERT INTO oauth_states (state_hash, user_id, provider, verifier, return_to, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![state_hash, s.user_id, s.provider, s.verifier, s.return_to, now()],
            )
            .map(drop)
        })
    }

    /// One-shot: the state is deleted when taken; states older than 10 minutes are rejected.
    pub fn take_oauth_state(&self, state_hash: &[u8]) -> Result<Option<OAuthState>> {
        self.with(|c| {
            c.query_row(
                "DELETE FROM oauth_states WHERE state_hash = ?1 AND created_at >= ?2
                 RETURNING user_id, provider, verifier, return_to",
                params![state_hash, now() - 600],
                |r| {
                    Ok(OAuthState {
                        user_id: r.get(0)?,
                        provider: r.get(1)?,
                        verifier: r.get(2)?,
                        return_to: r.get(3)?,
                    })
                },
            )
            .optional()
        })
    }
}
