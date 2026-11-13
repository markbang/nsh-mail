//! SQLite store: accounts, folders with IMAP sync state, message envelopes, lazily cached bodies,
//! and an FTS5 index. One file per user (`data_dir/mail.sqlite`, or `data/users/<id>.sqlite`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::err;
use crate::types::*;

pub(crate) struct Store {
    conn: Mutex<Connection>,
}

#[derive(Clone, Debug)]
pub(crate) struct FolderRow {
    pub id: i64,
    pub kind: FolderKind,
    pub name: String,
    pub uidvalidity: Option<u32>,
    pub uidnext: Option<u32>,
    pub highestmodseq: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct NewMessage {
    pub uid: u32,
    pub date: i64,
    pub from: Option<Address>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub subject: String,
    pub snippet: String,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub seen: bool,
    pub flagged: bool,
    pub has_attachments: bool,
    pub size: u32,
    /// Raw message when it was small enough to download during sync.
    pub body: Option<Vec<u8>>,
    pub index_text: Option<String>,
}

pub(crate) struct StoredMessage {
    pub summary: Summary,
    pub folder_name: String,
    pub uid: u32,
    pub cc: Vec<Address>,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub body: Option<Vec<u8>>,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    position INTEGER NOT NULL,
    data TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS folders (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    uidvalidity INTEGER,
    uidnext INTEGER,
    highestmodseq INTEGER,
    UNIQUE (account_id, kind)
);
CREATE TABLE IF NOT EXISTS messages (
    pk INTEGER PRIMARY KEY,
    id TEXT NOT NULL UNIQUE,
    folder_id INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    uid INTEGER NOT NULL,
    date INTEGER NOT NULL,
    from_addr TEXT,
    to_addrs TEXT NOT NULL,
    cc_addrs TEXT NOT NULL,
    subject TEXT NOT NULL,
    snippet TEXT NOT NULL,
    message_id TEXT,
    in_reply_to TEXT,
    refs TEXT NOT NULL,
    seen INTEGER NOT NULL,
    flagged INTEGER NOT NULL,
    has_attachments INTEGER NOT NULL,
    size INTEGER NOT NULL,
    body BLOB,
    UNIQUE (folder_id, uid)
);
CREATE INDEX IF NOT EXISTS messages_by_date ON messages (folder_id, date DESC, pk DESC);
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5 (
    subject, from_text, to_text, body,
    content = '', contentless_delete = 1, tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER IF NOT EXISTS messages_fts_delete AFTER DELETE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.pk;
END;
"#;

const SUMMARY_COLUMNS: &str = "m.id, f.account_id, f.kind, m.from_addr, m.to_addrs, m.subject, m.snippet, \
     m.date, m.seen, m.flagged, m.has_attachments";

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| {
                err(
                    ErrorCode::Storage,
                    format!("cannot create {}: {e}", dir.display()),
                )
            })?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Runs store work on tokio's blocking pool so SQLite never stalls an async worker.
    pub async fn run<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || f(&store)).await?
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // A panic while holding the lock cannot leave SQLite inconsistent (transactions roll back).
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn insert_account(&self, account: &Account) -> Result<()> {
        self.conn().execute(
            "INSERT INTO accounts (id, position, data)
             VALUES (?1, (SELECT IFNULL(MAX(position), 0) + 1 FROM accounts), ?2)",
            params![account.id, serde_json::to_string(account)?],
        )?;
        Ok(())
    }

    pub fn accounts(&self) -> Result<Vec<Account>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT data FROM accounts ORDER BY position")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }

    pub fn account(&self, id: &str) -> Result<Account> {
        let data: Option<String> = self
            .conn()
            .query_row("SELECT data FROM accounts WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        match data {
            Some(d) => Ok(serde_json::from_str(&d)?),
            None => Err(err(ErrorCode::NotFound, format!("no account with id {id}"))),
        }
    }

    pub fn delete_account(&self, id: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM accounts WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn folders(&self, account_id: &str) -> Result<Vec<FolderRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, kind, name, uidvalidity, uidnext, highestmodseq FROM folders WHERE account_id = ?1",
        )?;
        let rows = stmt.query_map([account_id], folder_row)?;
        Ok(rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect())
    }

    pub fn folder(&self, account_id: &str, kind: FolderKind) -> Result<Option<FolderRow>> {
        Ok(self
            .folders(account_id)?
            .into_iter()
            .find(|f| f.kind == kind))
    }

    /// Records which server mailbox holds each kind. A renamed or vanished mailbox drops its
    /// cached messages, since UIDs belong to the old mailbox.
    pub fn set_folders(&self, account_id: &str, folders: &[(FolderKind, String)]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for kind in FolderKind::ALL {
            let existing: Option<(i64, String)> = tx
                .query_row(
                    "SELECT id, name FROM folders WHERE account_id = ?1 AND kind = ?2",
                    params![account_id, kind.as_str()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let wanted = folders
                .iter()
                .find(|(k, _)| *k == kind)
                .map(|(_, n)| n.as_str());
            match (existing, wanted) {
                (Some((_, old)), Some(name)) if old == name => {}
                (Some((id, _)), Some(name)) => {
                    tx.execute("DELETE FROM messages WHERE folder_id = ?1", [id])?;
                    tx.execute(
                        "UPDATE folders SET name = ?2, uidvalidity = NULL, uidnext = NULL, highestmodseq = NULL
                         WHERE id = ?1",
                        params![id, name],
                    )?;
                }
                (Some((id, _)), None) => {
                    tx.execute("DELETE FROM folders WHERE id = ?1", [id])?;
                }
                (None, Some(name)) => {
                    tx.execute(
                        "INSERT INTO folders (account_id, kind, name) VALUES (?1, ?2, ?3)",
                        params![account_id, kind.as_str(), name],
                    )?;
                }
                (None, None) => {}
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_folder_state(&self, folder: &FolderRow) -> Result<()> {
        self.conn().execute(
            "UPDATE folders SET uidvalidity = ?2, uidnext = ?3, highestmodseq = ?4 WHERE id = ?1",
            params![
                folder.id,
                folder.uidvalidity,
                folder.uidnext,
                folder.highestmodseq.map(|m| m as i64)
            ],
        )?;
        Ok(())
    }

    pub fn clear_folder(&self, folder_id: i64) -> Result<()> {
        self.conn()
            .execute("DELETE FROM messages WHERE folder_id = ?1", [folder_id])?;
        Ok(())
    }

    /// Inserts messages not yet stored; returns summaries of the inserted ones.
    pub fn insert_messages(&self, folder_id: i64, messages: &[NewMessage]) -> Result<Vec<Summary>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut inserted = Vec::new();
        {
            let mut insert = tx.prepare(
                "INSERT INTO messages (id, folder_id, uid, date, from_addr, to_addrs, cc_addrs, subject,
                     snippet, message_id, in_reply_to, refs, seen, flagged, has_attachments, size, body)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
                 ON CONFLICT (folder_id, uid) DO NOTHING",
            )?;
            let mut fts = tx.prepare(
                "INSERT INTO messages_fts (rowid, subject, from_text, to_text, body) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for m in messages {
                let id = new_id();
                let n = insert.execute(params![
                    id,
                    folder_id,
                    m.uid,
                    m.date,
                    m.from.as_ref().map(serde_json::to_string).transpose()?,
                    serde_json::to_string(&m.to)?,
                    serde_json::to_string(&m.cc)?,
                    m.subject,
                    m.snippet,
                    m.message_id,
                    m.in_reply_to,
                    serde_json::to_string(&m.references)?,
                    m.seen,
                    m.flagged,
                    m.has_attachments,
                    m.size,
                    m.body,
                ])?;
                if n == 0 {
                    continue;
                }
                let pk = tx.last_insert_rowid();
                fts.execute(params![
                    pk,
                    fts_text(&m.subject),
                    fts_text(&addresses_text(m.from.iter())),
                    fts_text(&addresses_text(m.to.iter().chain(&m.cc))),
                    fts_text(m.index_text.as_deref().unwrap_or(&m.snippet)),
                ])?;
                inserted.push(id);
            }
        }
        tx.commit()?;
        drop(conn);
        inserted.iter().map(|id| self.summary(id)).collect()
    }

    pub fn uids(&self, folder_id: i64) -> Result<Vec<u32>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT uid FROM messages WHERE folder_id = ?1 ORDER BY uid")?;
        let rows = stmt.query_map([folder_id], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Applies server flags; returns how many rows changed.
    pub fn update_flags(&self, folder_id: i64, flags: &[(u32, bool, bool)]) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut changed = 0;
        {
            let mut stmt = tx.prepare(
                "UPDATE messages SET seen = ?3, flagged = ?4
                 WHERE folder_id = ?1 AND uid = ?2 AND (seen != ?3 OR flagged != ?4)",
            )?;
            for (uid, seen, flagged) in flags {
                changed += stmt.execute(params![folder_id, uid, seen, flagged])?;
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    pub fn delete_uids(&self, folder_id: i64, uids: &[u32]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("DELETE FROM messages WHERE folder_id = ?1 AND uid = ?2")?;
            for uid in uids {
                stmt.execute(params![folder_id, uid])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_local_flags(&self, id: &str, flags: Flags) -> Result<()> {
        self.conn().execute(
            "UPDATE messages SET seen = IFNULL(?2, seen), flagged = IFNULL(?3, flagged) WHERE id = ?1",
            params![id, flags.seen, flags.flagged],
        )?;
        Ok(())
    }

    /// Caches the raw message and reindexes it with its full text.
    pub fn set_body(&self, id: &str, raw: &[u8], text: &str, has_attachments: bool) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let (pk, subject, from, to, cc): (i64, String, Option<String>, String, String) = tx
            .query_row(
                "SELECT pk, subject, from_addr, to_addrs, cc_addrs FROM messages WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
        tx.execute(
            "UPDATE messages SET body = ?2, has_attachments = ?3 WHERE pk = ?1",
            params![pk, raw, has_attachments],
        )?;
        let from: Option<Address> = from.map(|f| serde_json::from_str(&f)).transpose()?;
        let to: Vec<Address> = serde_json::from_str(&to)?;
        let cc: Vec<Address> = serde_json::from_str(&cc)?;
        tx.execute("DELETE FROM messages_fts WHERE rowid = ?1", [pk])?;
        tx.execute(
            "INSERT INTO messages_fts (rowid, subject, from_text, to_text, body) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                pk,
                fts_text(&subject),
                fts_text(&addresses_text(from.iter())),
                fts_text(&addresses_text(to.iter().chain(&cc))),
                fts_text(text),
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn summary(&self, id: &str) -> Result<Summary> {
        self.conn()
            .query_row(
                &format!("SELECT {SUMMARY_COLUMNS} FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.id = ?1"),
                [id],
                summary_row,
            )
            .optional()?
            .ok_or_else(|| not_found(id))
    }

    pub fn message(&self, id: &str) -> Result<StoredMessage> {
        self.conn()
            .query_row(
                &format!(
                    "SELECT {SUMMARY_COLUMNS}, f.name, m.uid, m.cc_addrs, m.message_id, m.in_reply_to, m.refs, m.body
                     FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.id = ?1"
                ),
                [id],
                |r| {
                    Ok(StoredMessage {
                        summary: summary_row(r)?,
                        folder_name: r.get(11)?,
                        uid: r.get(12)?,
                        cc: json_col(r, 13)?,
                        message_id: r.get(14)?,
                        in_reply_to: r.get(15)?,
                        references: json_col(r, 16)?,
                        body: r.get(17)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| not_found(id))
    }

    /// Newest first across the matching accounts; `before` continues after that message.
    pub fn list(&self, q: &ListQuery) -> Result<Vec<Summary>> {
        let conn = self.conn();
        let cursor: Option<(i64, i64)> = match &q.before {
            Some(id) => Some(
                conn.query_row("SELECT date, pk FROM messages WHERE id = ?1", [id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?
                .ok_or_else(|| not_found(id))?,
            ),
            None => None,
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT {SUMMARY_COLUMNS} FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE f.kind = ?1 AND (?2 IS NULL OR f.account_id = ?2)
               AND (?3 IS NULL OR m.date < ?3 OR (m.date = ?3 AND m.pk < ?4))
             ORDER BY m.date DESC, m.pk DESC LIMIT ?5"
        ))?;
        let rows = stmt.query_map(
            params![
                q.folder.as_str(),
                q.account,
                cursor.map(|c| c.0),
                cursor.map(|c| c.1),
                q.limit.clamp(1, 500)
            ],
            summary_row,
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Unread messages in each account's inbox.
    pub fn unread(&self) -> Result<HashMap<AccountId, u32>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT f.account_id, COUNT(*) FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE f.kind = 'inbox' AND m.seen = 0 GROUP BY f.account_id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn search(&self, text: &str, limit: u32) -> Result<Vec<Summary>> {
        let Some(query) = fts_query(text) else {
            return Ok(Vec::new());
        };
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SUMMARY_COLUMNS} FROM messages_fts s
             JOIN messages m ON m.pk = s.rowid JOIN folders f ON f.id = m.folder_id
             WHERE messages_fts MATCH ?1 ORDER BY m.date DESC, m.pk DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![query, limit.clamp(1, 500)], summary_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

fn not_found(id: &str) -> Error {
    err(ErrorCode::NotFound, format!("no message with id {id}"))
}

fn folder_row(r: &Row) -> rusqlite::Result<Option<FolderRow>> {
    let kind: String = r.get(1)?;
    let Some(kind) = FolderKind::parse(&kind) else {
        return Ok(None);
    };
    Ok(Some(FolderRow {
        id: r.get(0)?,
        kind,
        name: r.get(2)?,
        uidvalidity: r.get(3)?,
        uidnext: r.get(4)?,
        highestmodseq: r.get::<_, Option<i64>>(5)?.map(|m| m as u64),
    }))
}

fn summary_row(r: &Row) -> rusqlite::Result<Summary> {
    let kind: String = r.get(2)?;
    let from: Option<String> = r.get(3)?;
    Ok(Summary {
        id: r.get(0)?,
        account_id: r.get(1)?,
        folder: FolderKind::parse(&kind).unwrap_or(FolderKind::Inbox),
        from: from.and_then(|f| serde_json::from_str(&f).ok()),
        to: json_col(r, 4)?,
        subject: r.get(5)?,
        snippet: r.get(6)?,
        date: r.get(7)?,
        seen: r.get(8)?,
        flagged: r.get(9)?,
        has_attachments: r.get(10)?,
    })
}

fn json_col<T: serde::de::DeserializeOwned + Default>(r: &Row, i: usize) -> rusqlite::Result<T> {
    let s: String = r.get(i)?;
    Ok(serde_json::from_str(&s).unwrap_or_default())
}

pub(crate) fn new_id() -> String {
    let mut b = [0u8; 12];
    getrandom::fill(&mut b).expect("OS random source unavailable");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn addresses_text<'a>(addrs: impl Iterator<Item = &'a Address>) -> String {
    let mut s = String::new();
    for a in addrs {
        if let Some(n) = &a.name {
            s.push_str(n);
            s.push(' ');
        }
        s.push_str(&a.email);
        s.push(' ');
    }
    s
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F)
}

/// unicode61 treats a run of CJK characters as one token; splitting them makes every character a
/// token so a phrase query finds any substring.
fn fts_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if is_cjk(c) {
            out.push(' ');
            out.push(c);
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// Every whitespace-separated term must match, as a prefix phrase.
fn fts_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split_whitespace()
        .map(|t| {
            let seg = fts_text(t).split_whitespace().collect::<Vec<_>>().join(" ");
            format!("\"{}\"*", seg.replace('"', "\"\""))
        })
        .filter(|t| t != "\"\"*")
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}
