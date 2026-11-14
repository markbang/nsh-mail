//! SQLite store: accounts, folders with IMAP sync state, message envelopes, lazily cached bodies,
//! and an FTS5 index. One database holds the mail of many users (the server's `server.sqlite`;
//! the app keeps [`LOCAL_USER`] alone in `mail.sqlite`). Every row carries `user_id`, and the
//! queries live only on [`Store`], which [`MailDb::user`] binds to one user: there is no way to
//! read or write mail without naming whose it is.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};

use crate::engine::StagedFile;
use crate::error::err;
use crate::types::*;

/// The user id of the app's single-user database.
pub const LOCAL_USER: i64 = 0;

/// A mail database file. Opening it creates the schema, or migrates a database from before
/// `user_id` in place (its rows become [`LOCAL_USER`]'s).
#[derive(Clone)]
pub struct MailDb {
    conn: Arc<Mutex<Connection>>,
}

/// One user's view of a [`MailDb`]. Every statement on the mail tables binds `user_id` as `?1`;
/// search index rows are reached only through that user's messages.
pub(crate) struct Store {
    conn: Arc<Mutex<Connection>>,
    user: i64,
}

/// Rows copied by [`MailDb::import_legacy`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportCounts {
    pub accounts: i64,
    pub folders: i64,
    pub messages: i64,
    /// Search index entries.
    pub indexed: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct FolderRow {
    pub id: i64,
    pub kind: FolderKind,
    pub name: String,
    pub uidvalidity: Option<u32>,
    pub uidnext: Option<u32>,
    /// The next sequence number to page down from while catching up on mail above everything
    /// stored; `None` when there is nothing to catch up on. Kept apart from `backfill_seq` so an
    /// interrupted catch-up resumes where it stopped instead of skipping the gap.
    pub catchup_seq: Option<u32>,
    /// The next sequence number to page down from for older mail; `None` once every message is
    /// stored. Sequence positions (not UIDs) because IMAP can only take a page off the end:
    /// expunges shift them down and can only make a page fetch a message twice, never skip one.
    pub backfill_seq: Option<u32>,
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
    /// Gmail's `X-GM-THRID`.
    pub gmail_thread: Option<u64>,
}

/// An outbox row with its files, as the dispatcher sends it or a cancel hands it back.
pub(crate) struct Queued {
    pub outgoing: Outgoing,
    pub files: HashMap<String, StagedFile>,
    pub attempts: u32,
}

/// Outbox rows move `queued` → `sending` → gone (sent), or → `failed`; only `queued` and
/// `failed` rows can be taken back.
const OUTBOX_COLUMNS: &str = "id, send_at, scheduled, draft, error, attempts";

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

/// Where a message is on the server, for actions on it.
#[derive(Clone, Debug)]
pub(crate) struct Located {
    pub id: MessageId,
    pub account_id: AccountId,
    pub folder: FolderKind,
    pub folder_name: String,
    pub uid: u32,
    pub seen: bool,
    pub flagged: bool,
    pub date: i64,
}

/// Account, folder and message ids are random or rowids, unique across users, so the
/// constraints on them need no `user_id`; the indexes queries use lead with it.
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS accounts_by_user ON accounts (user_id, position);
CREATE TABLE IF NOT EXISTS folders (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    uidvalidity INTEGER,
    uidnext INTEGER,
    catchup_seq INTEGER,
    backfill_seq INTEGER,
    highestmodseq INTEGER,
    UNIQUE (account_id, kind)
);
CREATE INDEX IF NOT EXISTS folders_by_user ON folders (user_id, kind, account_id);
CREATE TABLE IF NOT EXISTS messages (
    pk INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
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
    thread_id TEXT,
    UNIQUE (folder_id, uid)
);
CREATE INDEX IF NOT EXISTS messages_by_user_date ON messages (user_id, folder_id, date DESC, pk DESC);
CREATE INDEX IF NOT EXISTS messages_by_thread ON messages (user_id, thread_id);
-- What links a message to its thread: its own Message-ID, the ones it references, Gmail's thread
-- id. Kept after the messages go, so a late reply still finds its thread.
CREATE TABLE IF NOT EXISTS thread_keys (
    user_id INTEGER NOT NULL,
    key TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    PRIMARY KEY (user_id, key)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS thread_keys_by_thread ON thread_keys (user_id, thread_id);
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5 (
    subject, from_text, to_text, body,
    content = '', contentless_delete = 1, tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER IF NOT EXISTS messages_fts_delete AFTER DELETE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.pk;
END;
CREATE TABLE IF NOT EXISTS outbox (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    send_at INTEGER NOT NULL,
    scheduled INTEGER NOT NULL,
    state TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    draft TEXT NOT NULL,
    error TEXT
);
CREATE INDEX IF NOT EXISTS outbox_by_user ON outbox (user_id, state, send_at);
CREATE TABLE IF NOT EXISTS outbox_files (
    outbox_id TEXT NOT NULL REFERENCES outbox(id) ON DELETE CASCADE,
    upload_id TEXT NOT NULL,
    filename TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    data BLOB NOT NULL,
    PRIMARY KEY (outbox_id, upload_id)
);
"#;

const MESSAGE_COLUMNS: &str = "uid, date, from_addr, to_addrs, cc_addrs, subject, snippet, message_id, \
     in_reply_to, refs, seen, flagged, has_attachments, size, body";

const SUMMARY_COLUMNS: &str = "m.id, f.account_id, f.kind, m.from_addr, m.to_addrs, m.subject, m.snippet, \
     m.date, m.seen, m.flagged, m.has_attachments, IFNULL(m.thread_id, m.id)";

const LOCATED_COLUMNS: &str =
    "m.id, f.account_id, f.kind, f.name, m.uid, m.seen, m.flagged, m.date";

/// A message of a conversation listed in folder `?2`, of account `?3` (all when NULL); see
/// [`in_view`].
const IN_VIEW: &str = "(CASE WHEN ?2 IN ('trash', 'drafts') THEN f.kind = ?2 \
     ELSE f.kind NOT IN ('trash', 'drafts') END) AND (?3 IS NULL OR f.account_id = ?3)";

/// How far apart a reply without `References` and the message it answers may be.
const SUBJECT_WINDOW_MS: i64 = 30 * 24 * 3600 * 1000;

impl MailDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| {
                err(
                    ErrorCode::Storage,
                    format!("cannot create {}: {e}", dir.display()),
                )
            })?;
        }
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // A page of 200 messages writes a few MiB of WAL, so SQLite's default 1000-page
        // autocheckpoint runs every few pages and turned page commits into ~300 ms fsyncs on a
        // 50k mailbox. 32 MiB keeps a page near 10 ms and the WAL bounded.
        conn.pragma_update(None, "wal_autocheckpoint", 8192)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        if has_table(&conn, "main", "accounts")?
            && !has_column(&conn, "main", "accounts", "user_id")?
        {
            add_user_id(&mut conn).map_err(|e| {
                err(
                    ErrorCode::Storage,
                    format!("cannot upgrade {}: {}", path.display(), e.message),
                )
            })?;
        }
        if has_table(&conn, "main", "messages")?
            && !has_column(&conn, "main", "messages", "thread_id")?
        {
            conn.execute_batch("ALTER TABLE messages ADD COLUMN thread_id TEXT")?;
        }
        if has_table(&conn, "main", "folders")?
            && !has_column(&conn, "main", "folders", "backfill_seq")?
        {
            conn.execute_batch("ALTER TABLE folders ADD COLUMN backfill_seq INTEGER")?;
        }
        if has_table(&conn, "main", "folders")?
            && !has_column(&conn, "main", "folders", "catchup_seq")?
        {
            conn.execute_batch("ALTER TABLE folders ADD COLUMN catchup_seq INTEGER")?;
        }
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.execute_batch(SCHEMA)?;
        thread_unthreaded(&mut conn).map_err(|e| {
            err(
                ErrorCode::Storage,
                format!(
                    "cannot group {} into conversations: {}",
                    path.display(),
                    e.message
                ),
            )
        })?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub(crate) fn user(&self, user: i64) -> Store {
        Store {
            conn: self.conn.clone(),
            user,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        lock(&self.conn)
    }

    /// Users that have at least one account.
    pub fn users(&self) -> Result<Vec<i64>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT DISTINCT user_id FROM accounts")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Copies a single-user database from before `user_id` (the server's old
    /// `users/<id>.sqlite`) in as `user`'s mail, in one transaction that commits only when every
    /// account, folder (with its sync state) and message (with its flags and cached body) matches
    /// the source and the search index has as many entries. Message ids stay, so clients' links
    /// keep working. When `user` already holds exactly the source's data (an earlier run
    /// committed but was stopped before the caller moved the file), nothing is copied. The
    /// source is only read.
    pub fn import_legacy(&self, user: i64, path: &Path) -> Result<ImportCounts> {
        if !path.is_file() {
            return Err(err(
                ErrorCode::NotFound,
                format!("{} is not a file", path.display()),
            ));
        }
        let mut conn = self.lock();
        conn.execute("ATTACH DATABASE ?1 AS legacy", [path.to_string_lossy()])?;
        let result = import(&mut conn, user);
        let detached = conn.execute_batch("DETACH DATABASE legacy");
        let counts = result?;
        detached?;
        thread_unthreaded(&mut conn)?;
        Ok(counts)
    }
}

fn lock(conn: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    // A panic while holding the lock cannot leave SQLite inconsistent (transactions roll back).
    conn.lock().unwrap_or_else(|e| e.into_inner())
}

fn has_table(conn: &Connection, schema: &str, table: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            &format!("SELECT 1 FROM {schema}.sqlite_master WHERE name = ?1"),
            [table],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn has_column(conn: &Connection, schema: &str, table: &str, column: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM pragma_table_info(?1, ?2) WHERE name = ?3",
            [table, schema, column],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn count(conn: &Connection, sql: &str, params: impl rusqlite::Params) -> Result<i64> {
    Ok(conn.query_row(sql, params, |r| r.get(0))?)
}

/// Rebuilds the three tables of a database from before `user_id` with the column added (the
/// rows become [`LOCAL_USER`]'s). `pk`s are kept, so the search index stays valid untouched.
fn add_user_id(conn: &mut Connection) -> Result<()> {
    // Renaming must not rewrite the old tables' foreign keys, which get dropped with them.
    conn.pragma_update(None, "foreign_keys", false)?;
    conn.pragma_update(None, "legacy_alter_table", true)?;
    let result = (|| {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let before: Vec<i64> = ["accounts", "folders", "messages"]
            .iter()
            .map(|t| count(&tx, &format!("SELECT count(*) FROM {t}"), []))
            .collect::<Result<_>>()?;
        tx.execute_batch(
            "DROP TRIGGER IF EXISTS messages_fts_delete;
             DROP INDEX IF EXISTS messages_by_date;
             ALTER TABLE accounts RENAME TO v0_accounts;
             ALTER TABLE folders RENAME TO v0_folders;
             ALTER TABLE messages RENAME TO v0_messages;",
        )?;
        tx.execute_batch(SCHEMA)?;
        tx.execute(
            "INSERT INTO accounts (id, user_id, position, data)
             SELECT id, ?1, position, data FROM v0_accounts",
            [LOCAL_USER],
        )?;
        tx.execute(
            "INSERT INTO folders (id, user_id, account_id, kind, name, uidvalidity, uidnext, highestmodseq)
             SELECT id, ?1, account_id, kind, name, uidvalidity, uidnext, highestmodseq FROM v0_folders",
            [LOCAL_USER],
        )?;
        tx.execute(
            &format!(
                "INSERT INTO messages (pk, user_id, id, folder_id, {MESSAGE_COLUMNS})
                 SELECT pk, ?1, id, folder_id, {MESSAGE_COLUMNS} FROM v0_messages"
            ),
            [LOCAL_USER],
        )?;
        let after: Vec<i64> = ["accounts", "folders", "messages"]
            .iter()
            .map(|t| count(&tx, &format!("SELECT count(*) FROM {t}"), []))
            .collect::<Result<_>>()?;
        if before != after {
            return Err(err(
                ErrorCode::Storage,
                format!("row counts changed from {before:?} to {after:?}"),
            ));
        }
        tx.execute_batch("DROP TABLE v0_messages; DROP TABLE v0_folders; DROP TABLE v0_accounts;")?;
        let broken: Option<String> = tx
            .query_row("PRAGMA foreign_key_check", [], |r| r.get(0))
            .optional()?;
        if let Some(table) = broken {
            return Err(err(
                ErrorCode::Storage,
                format!("a row in {table} lost its parent"),
            ));
        }
        tx.commit()?;
        Ok(())
    })();
    conn.pragma_update(None, "legacy_alter_table", false)?;
    result
}

fn import(conn: &mut Connection, user: i64) -> Result<ImportCounts> {
    let check: String = conn.query_row("PRAGMA legacy.quick_check", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(err(
            ErrorCode::Storage,
            format!("the file is damaged (SQLite quick_check: {check})"),
        ));
    }
    for table in ["accounts", "folders", "messages", "messages_fts"] {
        if !has_table(conn, "legacy", table)? {
            return Err(err(
                ErrorCode::Storage,
                format!("not an nsh-mail database: it has no {table} table"),
            ));
        }
    }
    if has_column(conn, "legacy", "accounts", "user_id")? {
        return Err(err(
            ErrorCode::Storage,
            "the file already has the multi-user layout; it is not an old per-user database",
        ));
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let present = count(
        &tx,
        "SELECT EXISTS (SELECT 1 FROM main.accounts WHERE user_id = ?1)
             OR EXISTS (SELECT 1 FROM main.messages WHERE user_id = ?1)",
        [user],
    )? == 1;
    if !present {
        copy_legacy(&tx, user)?;
    }
    let counts = verify_legacy(&tx, user).map_err(|e| match present {
        true => err(
            e.code,
            format!(
                "user {user} already has mail in the database that differs from this file: {}",
                e.message
            ),
        ),
        false => e,
    })?;
    tx.commit()?;
    Ok(counts)
}

fn copy_legacy(tx: &Connection, user: i64) -> Result<()> {
    tx.execute(
        "INSERT INTO main.accounts (id, user_id, position, data)
         SELECT id, ?1, position, data FROM legacy.accounts",
        [user],
    )?;
    tx.execute(
        "INSERT INTO main.folders (user_id, account_id, kind, name, uidvalidity, uidnext, highestmodseq)
         SELECT ?1, account_id, kind, name, uidvalidity, uidnext, highestmodseq FROM legacy.folders
         ORDER BY id",
        [user],
    )?;
    let source_columns = MESSAGE_COLUMNS
        .split(", ")
        .map(|c| format!("s.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    tx.execute(
        &format!(
            "INSERT INTO main.messages (user_id, id, folder_id, {MESSAGE_COLUMNS})
             SELECT ?1, s.id, f.id, {source_columns}
             FROM legacy.messages s
             JOIN legacy.folders lf ON lf.id = s.folder_id
             JOIN main.folders f ON f.user_id = ?1 AND f.account_id = lf.account_id AND f.kind = lf.kind
             ORDER BY s.pk"
        ),
        [user],
    )?;
    // A contentless FTS5 table cannot be read back, so the index is rebuilt from the same text
    // the engine indexes: the cached body's text, or the snippet when no body is cached.
    let mut stmt = tx.prepare(
        "SELECT pk, subject, from_addr, to_addrs, cc_addrs, snippet, body
         FROM main.messages WHERE user_id = ?1",
    )?;
    let mut rows = stmt.query([user])?;
    while let Some(r) = rows.next()? {
        let body: Option<Vec<u8>> = r.get(6)?;
        let text = match body {
            Some(raw) => crate::mime::index(&raw).0,
            None => r.get(5)?,
        };
        index_stored(
            tx,
            r.get(0)?,
            &r.get::<_, String>(1)?,
            r.get(2)?,
            &r.get::<_, String>(3)?,
            &r.get::<_, String>(4)?,
            &text,
        )?;
    }
    Ok(())
}

fn verify_legacy(tx: &Connection, user: i64) -> Result<ImportCounts> {
    let pairs = [
        (
            "accounts",
            "SELECT count(*) FROM legacy.accounts",
            "SELECT count(*) FROM main.accounts WHERE user_id = ?1",
        ),
        (
            "folders",
            "SELECT count(*) FROM legacy.folders",
            "SELECT count(*) FROM main.folders WHERE user_id = ?1",
        ),
        (
            "messages",
            "SELECT count(*) FROM legacy.messages",
            "SELECT count(*) FROM main.messages WHERE user_id = ?1",
        ),
        (
            "search index entries",
            "SELECT count(*) FROM legacy.messages_fts",
            "SELECT count(*) FROM main.messages_fts
             WHERE rowid IN (SELECT pk FROM main.messages WHERE user_id = ?1)",
        ),
    ];
    let mut n = [0i64; 4];
    for (i, (what, source, imported)) in pairs.iter().enumerate() {
        let (a, b) = (count(tx, source, [])?, count(tx, imported, [user])?);
        if a != b {
            return Err(err(
                ErrorCode::Storage,
                format!("{what}: the file has {a}, the import has {b}"),
            ));
        }
        n[i] = a;
    }
    let same_message = MESSAGE_COLUMNS
        .split(", ")
        .map(|c| format!("m.{c} IS s.{c}"))
        .collect::<Vec<_>>()
        .join(" AND ");
    let mismatches = [
        (
            "accounts",
            "SELECT count(*) FROM legacy.accounts s WHERE NOT EXISTS (
                SELECT 1 FROM main.accounts a
                WHERE a.id = s.id AND a.user_id = ?1 AND a.position IS s.position AND a.data IS s.data)"
                .to_string(),
        ),
        (
            "folders",
            "SELECT count(*) FROM legacy.folders s WHERE NOT EXISTS (
                SELECT 1 FROM main.folders f
                WHERE f.account_id = s.account_id AND f.kind = s.kind AND f.user_id = ?1
                  AND f.name IS s.name AND f.uidvalidity IS s.uidvalidity
                  AND f.uidnext IS s.uidnext AND f.highestmodseq IS s.highestmodseq)"
                .to_string(),
        ),
        (
            "messages",
            format!(
                "SELECT count(*) FROM legacy.messages s JOIN legacy.folders lf ON lf.id = s.folder_id
                 WHERE NOT EXISTS (
                    SELECT 1 FROM main.messages m JOIN main.folders f ON f.id = m.folder_id
                    WHERE m.id = s.id AND m.user_id = ?1
                      AND f.account_id = lf.account_id AND f.kind = lf.kind AND {same_message})"
            ),
        ),
    ];
    for (what, sql) in mismatches {
        let bad = count(tx, &sql, [user])?;
        if bad > 0 {
            return Err(err(
                ErrorCode::Storage,
                format!("{bad} {what} differ between the file and the import"),
            ));
        }
    }
    Ok(ImportCounts {
        accounts: n[0],
        folders: n[1],
        messages: n[2],
        indexed: n[3],
    })
}

/// What threading reads of a message.
struct Linkage<'a> {
    pk: i64,
    account_id: &'a str,
    message_id: Option<&'a str>,
    in_reply_to: Option<&'a str>,
    references: &'a [String],
    gmail_thread: Option<u64>,
    subject: &'a str,
    date: i64,
    addresses: Vec<String>,
}

fn message_key(id: &str) -> Option<String> {
    let id = id
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim();
    (!id.is_empty()).then(|| id.to_string())
}

fn emails<'a>(addrs: impl Iterator<Item = &'a Address>) -> Vec<String> {
    addrs.map(|a| a.email.trim().to_lowercase()).collect()
}

/// The addresses of a user's own accounts, which every message of theirs shares.
fn own_addresses(conn: &Connection, user: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare_cached("SELECT data FROM accounts WHERE user_id = ?1")?;
    let rows = stmt.query_map([user], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for data in rows {
        let account: Account = serde_json::from_str(&data?)?;
        out.push(account.email.to_lowercase());
    }
    Ok(out)
}

/// Puts a stored message in a thread, JWZ-style: the threads its own Message-ID, the
/// Message-IDs it references and its Gmail thread id already belong to (merged into one when
/// it joins several); else, for a reply that references nothing, the latest message within
/// [`SUBJECT_WINDOW_MS`] with the same [`base_subject`] and someone besides the user in
/// common; else a thread of its own.
fn thread_message(conn: &Connection, user: i64, own: &[String], m: &Linkage) -> Result<()> {
    let mut keys: Vec<String> = m
        .message_id
        .into_iter()
        .chain(m.in_reply_to)
        .chain(m.references.iter().map(String::as_str))
        .filter_map(message_key)
        .collect();
    if let Some(g) = m.gmail_thread {
        keys.push(format!("gm:{}:{g}", m.account_id));
    }
    keys.sort();
    keys.dedup();
    let mut found: Vec<String> = Vec::new();
    {
        let mut stmt = conn
            .prepare_cached("SELECT thread_id FROM thread_keys WHERE user_id = ?1 AND key = ?2")?;
        for key in &keys {
            if let Some(t) = stmt
                .query_row(params![user, key], |r| r.get::<_, String>(0))
                .optional()?
                && !found.contains(&t)
            {
                found.push(t);
            }
        }
    }
    let unlinked = m.in_reply_to.is_none() && m.references.is_empty() && m.gmail_thread.is_none();
    if found.is_empty() && unlinked {
        found.extend(by_subject(conn, user, own, m)?);
    }
    found.sort();
    let thread = match found.split_first() {
        Some((keep, others)) => {
            for other in others {
                conn.execute(
                    "UPDATE messages SET thread_id = ?3 WHERE user_id = ?1 AND thread_id = ?2",
                    params![user, other, keep],
                )?;
                conn.execute(
                    "UPDATE thread_keys SET thread_id = ?3 WHERE user_id = ?1 AND thread_id = ?2",
                    params![user, other, keep],
                )?;
            }
            keep.clone()
        }
        None => new_id(),
    };
    conn.prepare_cached("UPDATE messages SET thread_id = ?3 WHERE user_id = ?1 AND pk = ?2")?
        .execute(params![user, m.pk, thread])?;
    let mut insert = conn.prepare_cached(
        "INSERT INTO thread_keys (user_id, key, thread_id) VALUES (?1, ?2, ?3)
         ON CONFLICT (user_id, key) DO UPDATE SET thread_id = excluded.thread_id",
    )?;
    for key in &keys {
        insert.execute(params![user, key, thread])?;
    }
    Ok(())
}

fn by_subject(conn: &Connection, user: i64, own: &[String], m: &Linkage) -> Result<Option<String>> {
    let (base, replied) = base_subject(m.subject);
    if !replied || base.is_empty() {
        return Ok(None);
    }
    let others: Vec<&String> = m.addresses.iter().filter(|a| !own.contains(a)).collect();
    if others.is_empty() {
        return Ok(None);
    }
    let mut stmt = conn.prepare_cached(
        "SELECT thread_id, subject, from_addr, to_addrs, cc_addrs FROM messages
         WHERE user_id = ?1 AND pk != ?2 AND thread_id IS NOT NULL AND date BETWEEN ?3 AND ?4
         ORDER BY abs(date - ?5) LIMIT 2000",
    )?;
    let mut rows = stmt.query(params![
        user,
        m.pk,
        m.date - SUBJECT_WINDOW_MS,
        m.date + SUBJECT_WINDOW_MS,
        m.date
    ])?;
    while let Some(r) = rows.next()? {
        if base_subject(&r.get::<_, String>(1)?).0 != base {
            continue;
        }
        let from: Option<Address> = r
            .get::<_, Option<String>>(2)?
            .and_then(|f| serde_json::from_str(&f).ok());
        let to: Vec<Address> = json_col(r, 3)?;
        let cc: Vec<Address> = json_col(r, 4)?;
        if emails(from.iter().chain(&to).chain(&cc))
            .iter()
            .any(|a| others.contains(&a))
        {
            return Ok(Some(r.get(0)?));
        }
    }
    Ok(None)
}

/// A stored message as threading reads it, before it has a thread.
struct Unthreaded {
    pk: i64,
    user: i64,
    account_id: String,
    message_id: Option<String>,
    in_reply_to: Option<String>,
    references: Vec<String>,
    subject: String,
    date: i64,
    addresses: Vec<String>,
}

/// Threads every message without a thread (rows from before threads, or copied in by
/// [`MailDb::import_legacy`]), oldest first, in one transaction.
fn thread_unthreaded(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let rows: Vec<Unthreaded> = {
        let mut stmt = tx.prepare(
            "SELECT m.pk, m.user_id, f.account_id, m.message_id, m.in_reply_to, m.refs, m.subject,
                    m.date, m.from_addr, m.to_addrs, m.cc_addrs
             FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE m.thread_id IS NULL ORDER BY m.user_id, m.date, m.pk",
        )?;
        let rows = stmt.query_map([], |r| {
            let from: Option<Address> = r
                .get::<_, Option<String>>(8)?
                .and_then(|f| serde_json::from_str(&f).ok());
            let to: Vec<Address> = json_col(r, 9)?;
            let cc: Vec<Address> = json_col(r, 10)?;
            Ok(Unthreaded {
                pk: r.get(0)?,
                user: r.get(1)?,
                account_id: r.get(2)?,
                message_id: r.get(3)?,
                in_reply_to: r.get(4)?,
                references: json_col(r, 5)?,
                subject: r.get(6)?,
                date: r.get(7)?,
                addresses: emails(from.iter().chain(&to).chain(&cc)),
            })
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut own: HashMap<i64, Vec<String>> = HashMap::new();
    for m in &rows {
        if let std::collections::hash_map::Entry::Vacant(e) = own.entry(m.user) {
            e.insert(own_addresses(&tx, m.user)?);
        }
        let link = Linkage {
            pk: m.pk,
            account_id: &m.account_id,
            message_id: m.message_id.as_deref(),
            in_reply_to: m.in_reply_to.as_deref(),
            references: &m.references,
            gmail_thread: None,
            subject: &m.subject,
            date: m.date,
            addresses: m.addresses.clone(),
        };
        thread_message(&tx, m.user, &own[&m.user], &link)?;
    }
    tx.commit()?;
    Ok(())
}

/// Adds a message's search entry under `pk`.
fn index(
    conn: &Connection,
    pk: i64,
    subject: &str,
    from: &str,
    to: &str,
    text: &str,
) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO messages_fts (rowid, subject, from_text, to_text, body) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?
    .execute(params![pk, fts_text(subject), fts_text(from), fts_text(to), fts_text(text)])?;
    Ok(())
}

/// [`index`] from the stored JSON address columns.
fn index_stored(
    conn: &Connection,
    pk: i64,
    subject: &str,
    from: Option<String>,
    to: &str,
    cc: &str,
    text: &str,
) -> Result<()> {
    let from: Option<Address> = from.map(|f| serde_json::from_str(&f)).transpose()?;
    let to: Vec<Address> = serde_json::from_str(to)?;
    let cc: Vec<Address> = serde_json::from_str(cc)?;
    index(
        conn,
        pk,
        subject,
        &addresses_text(from.iter()),
        &addresses_text(to.iter().chain(&cc)),
        text,
    )
}

impl Store {
    /// Runs store work on tokio's blocking pool so SQLite never stalls an async worker.
    pub async fn run<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || f(&store)).await?
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        lock(&self.conn)
    }

    pub fn insert_account(&self, account: &Account) -> Result<()> {
        self.conn().execute(
            "INSERT INTO accounts (user_id, id, position, data)
             VALUES (?1, ?2, (SELECT IFNULL(MAX(position), 0) + 1 FROM accounts WHERE user_id = ?1), ?3)",
            params![self.user, account.id, serde_json::to_string(account)?],
        )?;
        Ok(())
    }

    pub fn update_account(&self, account: &Account) -> Result<()> {
        let n = self.conn().execute(
            "UPDATE accounts SET data = ?3 WHERE user_id = ?1 AND id = ?2",
            params![self.user, account.id, serde_json::to_string(account)?],
        )?;
        if n == 0 {
            return Err(err(
                ErrorCode::NotFound,
                format!("no account with id {}", account.id),
            ));
        }
        Ok(())
    }

    pub fn accounts(&self) -> Result<Vec<Account>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT data FROM accounts WHERE user_id = ?1 ORDER BY position")?;
        let rows = stmt.query_map([self.user], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }

    pub fn account(&self, id: &str) -> Result<Account> {
        let data: Option<String> = self
            .conn()
            .query_row(
                "SELECT data FROM accounts WHERE user_id = ?1 AND id = ?2",
                params![self.user, id],
                |r| r.get(0),
            )
            .optional()?;
        match data {
            Some(d) => Ok(serde_json::from_str(&d)?),
            None => Err(err(ErrorCode::NotFound, format!("no account with id {id}"))),
        }
    }

    pub fn delete_account(&self, id: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM accounts WHERE user_id = ?1 AND id = ?2",
            params![self.user, id],
        )?;
        Ok(())
    }

    pub fn folders(&self, account_id: &str) -> Result<Vec<FolderRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, kind, name, uidvalidity, uidnext, catchup_seq, backfill_seq, highestmodseq FROM folders
             WHERE user_id = ?1 AND account_id = ?2",
        )?;
        let rows = stmt.query_map(params![self.user, account_id], folder_row)?;
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
        let user = self.user;
        // The account must be this user's before folders are added under it.
        let owned: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM accounts WHERE user_id = ?1 AND id = ?2",
                params![user, account_id],
                |r| r.get(0),
            )
            .optional()?;
        if owned.is_none() {
            return Err(err(
                ErrorCode::NotFound,
                format!("no account with id {account_id}"),
            ));
        }
        for kind in FolderKind::ALL {
            let existing: Option<(i64, String)> = tx
                .query_row(
                    "SELECT id, name FROM folders WHERE user_id = ?1 AND account_id = ?2 AND kind = ?3",
                    params![user, account_id, kind.as_str()],
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
                    tx.execute(
                        "DELETE FROM messages WHERE user_id = ?1 AND folder_id = ?2",
                        params![user, id],
                    )?;
                    tx.execute(
                        "UPDATE folders SET name = ?3, uidvalidity = NULL, uidnext = NULL,
                             catchup_seq = NULL, backfill_seq = NULL, highestmodseq = NULL
                         WHERE user_id = ?1 AND id = ?2",
                        params![user, id, name],
                    )?;
                }
                (Some((id, _)), None) => {
                    tx.execute(
                        "DELETE FROM folders WHERE user_id = ?1 AND id = ?2",
                        params![user, id],
                    )?;
                }
                (None, Some(name)) => {
                    tx.execute(
                        "INSERT INTO folders (user_id, account_id, kind, name) VALUES (?1, ?2, ?3, ?4)",
                        params![user, account_id, kind.as_str(), name],
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
            "UPDATE folders SET uidvalidity = ?3, uidnext = ?4, catchup_seq = ?5,
                 backfill_seq = ?6, highestmodseq = ?7
             WHERE user_id = ?1 AND id = ?2",
            params![
                self.user,
                folder.id,
                folder.uidvalidity,
                folder.uidnext,
                folder.catchup_seq,
                folder.backfill_seq,
                folder.highestmodseq.map(|m| m as i64)
            ],
        )?;
        Ok(())
    }

    pub fn clear_folder(&self, folder_id: i64) -> Result<()> {
        self.conn().execute(
            "DELETE FROM messages WHERE user_id = ?1 AND folder_id = ?2",
            params![self.user, folder_id],
        )?;
        Ok(())
    }

    /// Inserts one sync page and records the folder's watermark in the same transaction, so a
    /// page interrupted midway leaves neither rows nor a watermark past them.
    pub fn insert_page(
        &self,
        folder_id: i64,
        messages: &[NewMessage],
        state: Option<&FolderRow>,
    ) -> Result<Vec<Summary>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut inserted = Vec::new();
        let account_id: Option<String> = tx
            .query_row(
                "SELECT account_id FROM folders WHERE user_id = ?1 AND id = ?2",
                params![self.user, folder_id],
                |r| r.get(0),
            )
            .optional()?;
        let own = own_addresses(&tx, self.user)?;
        {
            // Selecting the folder through `user_id` makes a folder of another user insert nothing.
            let mut insert = tx.prepare(&format!(
                "INSERT INTO messages (user_id, id, folder_id, {MESSAGE_COLUMNS})
                 SELECT ?1, ?2, id, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18
                 FROM folders WHERE user_id = ?1 AND id = ?3
                 ON CONFLICT (folder_id, uid) DO NOTHING"
            ))?;
            for m in messages {
                let id = new_id();
                let n = insert.execute(params![
                    self.user,
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
                thread_message(
                    &tx,
                    self.user,
                    &own,
                    &Linkage {
                        pk,
                        account_id: account_id.as_deref().unwrap_or_default(),
                        message_id: m.message_id.as_deref(),
                        in_reply_to: m.in_reply_to.as_deref(),
                        references: &m.references,
                        gmail_thread: m.gmail_thread,
                        subject: &m.subject,
                        date: m.date,
                        addresses: emails(m.from.iter().chain(&m.to).chain(&m.cc)),
                    },
                )?;
                index(
                    &tx,
                    pk,
                    &m.subject,
                    &addresses_text(m.from.iter()),
                    &addresses_text(m.to.iter().chain(&m.cc)),
                    m.index_text.as_deref().unwrap_or(&m.snippet),
                )?;
                inserted.push(id);
            }
        }
        if let Some(f) = state {
            tx.execute(
                "UPDATE folders SET uidvalidity = ?3, uidnext = ?4, catchup_seq = ?5,
                     backfill_seq = ?6, highestmodseq = ?7
                 WHERE user_id = ?1 AND id = ?2",
                params![
                    self.user,
                    f.id,
                    f.uidvalidity,
                    f.uidnext,
                    f.catchup_seq,
                    f.backfill_seq,
                    f.highestmodseq.map(|m| m as i64)
                ],
            )?;
        }
        tx.commit()?;
        drop(conn);
        inserted.iter().map(|id| self.summary(id)).collect()
    }

    pub fn uids(&self, folder_id: i64) -> Result<Vec<u32>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT uid FROM messages WHERE user_id = ?1 AND folder_id = ?2 ORDER BY uid",
        )?;
        let rows = stmt.query_map(params![self.user, folder_id], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Applies server flags; returns how many rows changed.
    pub fn update_flags(&self, folder_id: i64, flags: &[(u32, bool, bool)]) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut changed = 0;
        {
            let mut stmt = tx.prepare(
                "UPDATE messages SET seen = ?4, flagged = ?5
                 WHERE user_id = ?1 AND folder_id = ?2 AND uid = ?3 AND (seen != ?4 OR flagged != ?5)",
            )?;
            for (uid, seen, flagged) in flags {
                changed += stmt.execute(params![self.user, folder_id, uid, seen, flagged])?;
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    pub fn delete_uids(&self, folder_id: i64, uids: &[u32]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "DELETE FROM messages WHERE user_id = ?1 AND folder_id = ?2 AND uid = ?3",
            )?;
            for uid in uids {
                stmt.execute(params![self.user, folder_id, uid])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_message(&self, id: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM messages WHERE user_id = ?1 AND id = ?2",
            params![self.user, id],
        )?;
        Ok(())
    }

    pub fn set_local_flags(&self, id: &str, flags: Flags) -> Result<()> {
        self.conn().execute(
            "UPDATE messages SET seen = IFNULL(?3, seen), flagged = IFNULL(?4, flagged)
             WHERE user_id = ?1 AND id = ?2",
            params![self.user, id, flags.seen, flags.flagged],
        )?;
        Ok(())
    }

    pub fn set_has_attachments(&self, id: &str, has_attachments: bool) -> Result<()> {
        self.conn().execute(
            "UPDATE messages SET has_attachments = ?3 WHERE user_id = ?1 AND id = ?2",
            params![self.user, id, has_attachments],
        )?;
        Ok(())
    }

    /// Caches the raw message and reindexes it with its full text.
    pub fn set_body(&self, id: &str, raw: &[u8], text: &str, has_attachments: bool) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let (pk, subject, from, to, cc): (i64, String, Option<String>, String, String) = tx
            .query_row(
                "SELECT pk, subject, from_addr, to_addrs, cc_addrs FROM messages
                 WHERE user_id = ?1 AND id = ?2",
                params![self.user, id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?
            .ok_or_else(|| not_found(id))?;
        tx.execute(
            "UPDATE messages SET body = ?3, has_attachments = ?4 WHERE user_id = ?1 AND pk = ?2",
            params![self.user, pk, raw, has_attachments],
        )?;
        tx.execute("DELETE FROM messages_fts WHERE rowid = ?1", [pk])?;
        index_stored(&tx, pk, &subject, from, &to, &cc, text)?;
        tx.commit()?;
        Ok(())
    }

    pub fn summary(&self, id: &str) -> Result<Summary> {
        self.conn()
            .query_row(
                &format!(
                    "SELECT {SUMMARY_COLUMNS} FROM messages m JOIN folders f ON f.id = m.folder_id
                     WHERE m.user_id = ?1 AND m.id = ?2"
                ),
                params![self.user, id],
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
                     FROM messages m JOIN folders f ON f.id = m.folder_id
                     WHERE m.user_id = ?1 AND m.id = ?2"
                ),
                params![self.user, id],
                |r| {
                    Ok(StoredMessage {
                        summary: summary_row(r)?,
                        folder_name: r.get(12)?,
                        uid: r.get(13)?,
                        cc: json_col(r, 14)?,
                        message_id: r.get(15)?,
                        in_reply_to: r.get(16)?,
                        references: json_col(r, 17)?,
                        body: r.get(18)?,
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
                conn.query_row(
                    "SELECT date, pk FROM messages WHERE user_id = ?1 AND id = ?2",
                    params![self.user, id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?
                .ok_or_else(|| not_found(id))?,
            ),
            None => None,
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT {SUMMARY_COLUMNS} FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE m.user_id = ?1 AND f.user_id = ?1 AND f.kind = ?2
               AND (?3 IS NULL OR f.account_id = ?3)
               AND (?4 IS NULL OR m.date < ?4 OR (m.date = ?4 AND m.pk < ?5))
             ORDER BY m.date DESC, m.pk DESC LIMIT ?6"
        ))?;
        let rows = stmt.query_map(
            params![
                self.user,
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

    /// Conversations with a message in the folder, by their latest message in view
    /// ([`in_view`]), newest first; `before` (a thread id) continues after that thread.
    pub fn threads(&self, q: &ListQuery) -> Result<Vec<Thread>> {
        let conn = self.conn();
        let view = q.folder.as_str();
        let cursor: Option<(i64, &str)> = match &q.before {
            Some(id) => {
                let date: Option<i64> = conn.query_row(
                    &format!(
                        "SELECT MAX(m.date) FROM messages m JOIN folders f ON f.id = m.folder_id
                         WHERE m.user_id = ?1 AND m.thread_id = ?4 AND {IN_VIEW}"
                    ),
                    params![self.user, view, q.account, id],
                    |r| r.get(0),
                )?;
                let date = date.ok_or_else(|| {
                    err(ErrorCode::NotFound, format!("no conversation with id {id}"))
                })?;
                Some((date, id.as_str()))
            }
            None => None,
        };
        let ids: Vec<String> = {
            let mut stmt = conn.prepare(&format!(
                "SELECT m.thread_id, MAX(m.date) AS latest FROM messages m JOIN folders f ON f.id = m.folder_id
                 WHERE m.user_id = ?1 AND {IN_VIEW} AND m.thread_id IN (
                     SELECT l.thread_id FROM messages l JOIN folders lf ON lf.id = l.folder_id
                     WHERE l.user_id = ?1 AND lf.user_id = ?1 AND lf.kind = ?2
                       AND (?3 IS NULL OR lf.account_id = ?3))
                 GROUP BY m.thread_id
                 HAVING ?4 IS NULL OR latest < ?4 OR (latest = ?4 AND m.thread_id < ?5)
                 ORDER BY latest DESC, m.thread_id DESC LIMIT ?6"
            ))?;
            let rows = stmt.query_map(
                params![
                    self.user,
                    view,
                    q.account,
                    cursor.map(|c| c.0),
                    cursor.map(|c| c.1),
                    q.limit.clamp(1, 500)
                ],
                |r| r.get(0),
            )?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut members: HashMap<String, Vec<(Summary, Option<String>)>> = HashMap::new();
        {
            let mut stmt = conn.prepare(&format!(
                "SELECT {SUMMARY_COLUMNS}, m.message_id FROM messages m JOIN folders f ON f.id = m.folder_id
                 WHERE m.user_id = ?1 AND {IN_VIEW} AND m.thread_id IN (SELECT value FROM json_each(?4))
                 ORDER BY m.pk"
            ))?;
            let rows = stmt.query_map(
                params![self.user, view, q.account, serde_json::to_string(&ids)?],
                |r| Ok((summary_row(r)?, r.get::<_, Option<String>>(12)?)),
            )?;
            for row in rows {
                let (summary, message_id) = row?;
                members
                    .entry(summary.thread_id.clone())
                    .or_default()
                    .push((summary, message_id));
            }
        }
        Ok(ids
            .into_iter()
            .filter_map(|id| {
                let messages = conversation(members.remove(&id)?);
                Thread::of(id, &messages)
            })
            .collect())
    }

    /// Every message of a thread, in any folder and account, oldest first, each once
    /// ([`conversation`]).
    pub fn thread(&self, id: &str) -> Result<Vec<Summary>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SUMMARY_COLUMNS}, m.message_id FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE m.user_id = ?1 AND m.thread_id = ?2 ORDER BY m.pk"
        ))?;
        let rows = stmt.query_map(params![self.user, id], |r| {
            Ok((summary_row(r)?, r.get::<_, Option<String>>(12)?))
        })?;
        let messages = conversation(rows.collect::<rusqlite::Result<_>>()?);
        if messages.is_empty() {
            return Err(err(
                ErrorCode::NotFound,
                format!("no conversation with id {id}"),
            ));
        }
        Ok(messages)
    }

    /// Every stored copy of a thread's messages, oldest first.
    pub fn thread_copies(&self, id: &str) -> Result<Vec<Located>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {LOCATED_COLUMNS} FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE m.user_id = ?1 AND m.thread_id = ?2 ORDER BY m.date, m.pk"
        ))?;
        let rows = stmt.query_map(params![self.user, id], located_row)?;
        let copies: Vec<Located> = rows.collect::<rusqlite::Result<_>>()?;
        if copies.is_empty() {
            return Err(err(
                ErrorCode::NotFound,
                format!("no conversation with id {id}"),
            ));
        }
        Ok(copies)
    }

    pub fn located(&self, id: &str) -> Result<Located> {
        self.conn()
            .query_row(
                &format!(
                    "SELECT {LOCATED_COLUMNS} FROM messages m JOIN folders f ON f.id = m.folder_id
                     WHERE m.user_id = ?1 AND m.id = ?2"
                ),
                params![self.user, id],
                located_row,
            )
            .optional()?
            .ok_or_else(|| not_found(id))
    }

    /// Unread messages in each account's inbox.
    pub fn unread(&self) -> Result<HashMap<AccountId, u32>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT f.account_id, COUNT(*) FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE m.user_id = ?1 AND f.user_id = ?1 AND f.kind = 'inbox' AND m.seen = 0
             GROUP BY f.account_id",
        )?;
        let rows = stmt.query_map([self.user], |r| Ok((r.get(0)?, r.get(1)?)))?;
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
             WHERE messages_fts MATCH ?2 AND m.user_id = ?1
             ORDER BY m.date DESC, m.pk DESC LIMIT ?3"
        ))?;
        let rows = stmt.query_map(params![self.user, query, limit.clamp(1, 500)], summary_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn queue_outgoing(&self, o: &Outgoing, files: &HashMap<String, StagedFile>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO outbox (id, user_id, account_id, send_at, scheduled, state, draft)
             VALUES (?2, ?1, ?3, ?4, ?5, 'queued', ?6)",
            params![
                self.user,
                o.id,
                o.draft.account_id,
                o.send_at,
                o.scheduled,
                serde_json::to_string(&o.draft)?
            ],
        )?;
        for (id, f) in files {
            tx.execute(
                "INSERT INTO outbox_files (outbox_id, upload_id, filename, mime_type, data)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![o.id, id, f.filename, f.mime_type, f.data],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Every message in the outbox, the next to go first.
    pub fn outbox(&self) -> Result<Vec<Outgoing>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {OUTBOX_COLUMNS} FROM outbox WHERE user_id = ?1 ORDER BY send_at, id"
        ))?;
        let rows = stmt
            .query_map([self.user], outbox_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(o, _)| {
                let mut o = o?;
                o.files = outbox_files(&conn, &o, false)?.0;
                Ok(o)
            })
            .collect()
    }

    /// When the next queued message is due.
    pub fn next_due(&self) -> Result<Option<i64>> {
        Ok(self.conn().query_row(
            "SELECT MIN(send_at) FROM outbox WHERE user_id = ?1 AND state = 'queued'",
            [self.user],
            |r| r.get(0),
        )?)
    }

    /// Marks the messages due by `now` as being sent and returns them.
    pub fn claim_due(&self, now: i64) -> Result<Vec<Queued>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let rows = {
            let mut stmt = tx.prepare(&format!(
                "UPDATE outbox SET state = 'sending'
                 WHERE user_id = ?1 AND state = 'queued' AND send_at <= ?2
                 RETURNING {OUTBOX_COLUMNS}"
            ))?;
            stmt.query_map(params![self.user, now], outbox_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut due = Vec::with_capacity(rows.len());
        for (o, attempts) in rows {
            let mut outgoing = o?;
            let (meta, files) = outbox_files(&tx, &outgoing, true)?;
            outgoing.files = meta;
            due.push(Queued {
                outgoing,
                files,
                attempts,
            });
        }
        tx.commit()?;
        due.sort_by_key(|q| q.outgoing.send_at);
        Ok(due)
    }

    /// Removes a message that waits (queued or failed) and returns it with its files.
    pub fn take_outgoing(&self, id: &str) -> Result<Queued> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let row: Option<(String, (Result<Outgoing>, u32))> = tx
            .query_row(
                &format!(
                    "SELECT state, {OUTBOX_COLUMNS} FROM outbox WHERE user_id = ?1 AND id = ?2"
                ),
                params![self.user, id],
                |r| Ok((r.get(0)?, outbox_row_at(r, 1)?)),
            )
            .optional()?;
        let Some((state, (o, attempts))) = row else {
            return Err(err(
                ErrorCode::NotFound,
                "this message is no longer waiting: it was sent or cancelled",
            ));
        };
        if state == "sending" {
            return Err(err(
                ErrorCode::InvalidInput,
                "this message is being sent right now",
            ));
        }
        let mut outgoing = o?;
        let (meta, files) = outbox_files(&tx, &outgoing, true)?;
        outgoing.files = meta;
        tx.execute(
            "DELETE FROM outbox WHERE user_id = ?1 AND id = ?2",
            params![self.user, id],
        )?;
        tx.commit()?;
        Ok(Queued {
            outgoing,
            files,
            attempts,
        })
    }

    pub fn outgoing_sent(&self, id: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM outbox WHERE user_id = ?1 AND id = ?2",
            params![self.user, id],
        )?;
        Ok(())
    }

    pub fn outgoing_failed(&self, id: &str, error: &Error) -> Result<()> {
        self.conn().execute(
            "UPDATE outbox SET state = 'failed', error = ?3 WHERE user_id = ?1 AND id = ?2",
            params![self.user, id, serde_json::to_string(error)?],
        )?;
        Ok(())
    }

    /// Queues a message again after a failure that may pass (the network), at `at`.
    pub fn outgoing_retry(&self, id: &str, at: i64, error: &Error) -> Result<()> {
        self.conn().execute(
            "UPDATE outbox SET state = 'queued', send_at = ?3, attempts = attempts + 1, error = ?4
             WHERE user_id = ?1 AND id = ?2",
            params![self.user, id, at, serde_json::to_string(error)?],
        )?;
        Ok(())
    }

    /// Messages left `sending` by a process that stopped: they may or may not have gone, so
    /// they wait for the user rather than go twice.
    pub fn outgoing_interrupted(&self, error: &Error) -> Result<Vec<Outgoing>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "UPDATE outbox SET state = 'failed', error = ?2
             WHERE user_id = ?1 AND state = 'sending' RETURNING {OUTBOX_COLUMNS}"
        ))?;
        let rows = stmt
            .query_map(
                params![self.user, serde_json::to_string(error)?],
                outbox_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(|(o, _)| o).collect()
    }
}

fn outbox_row(r: &Row) -> rusqlite::Result<(Result<Outgoing>, u32)> {
    outbox_row_at(r, 0)
}

/// An outbox row's columns ([`OUTBOX_COLUMNS`]) from column `at`; its `files` are left empty.
fn outbox_row_at(r: &Row, at: usize) -> rusqlite::Result<(Result<Outgoing>, u32)> {
    let (id, send_at, scheduled): (String, i64, bool) =
        (r.get(at)?, r.get(at + 1)?, r.get(at + 2)?);
    let draft: String = r.get(at + 3)?;
    let error: Option<String> = r.get(at + 4)?;
    let outgoing = serde_json::from_str::<Draft>(&draft)
        .map_err(Error::from)
        .map(|draft| Outgoing {
            id,
            draft,
            send_at,
            scheduled,
            files: Vec::new(),
            error: error.and_then(|e| serde_json::from_str(&e).ok()),
        });
    Ok((outgoing, r.get(at + 5)?))
}

/// A message's files in the order its draft names them, with their bytes when `data`.
fn outbox_files(
    conn: &Connection,
    o: &Outgoing,
    data: bool,
) -> Result<(Vec<Upload>, HashMap<String, StagedFile>)> {
    let mut stmt = conn.prepare(if data {
        "SELECT upload_id, filename, mime_type, length(data), data FROM outbox_files WHERE outbox_id = ?1"
    } else {
        "SELECT upload_id, filename, mime_type, length(data), NULL FROM outbox_files WHERE outbox_id = ?1"
    })?;
    let mut found: HashMap<String, (Upload, Option<Vec<u8>>)> = stmt
        .query_map([&o.id], |r| {
            let id: String = r.get(0)?;
            let upload = Upload {
                id: id.clone(),
                filename: r.get(1)?,
                mime_type: r.get(2)?,
                size: r.get::<_, i64>(3)? as u64,
            };
            Ok((id, (upload, r.get(4)?)))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut meta = Vec::new();
    let mut files = HashMap::new();
    for a in &o.draft.attachments {
        let DraftAttachment::Upload { id } = a else {
            continue;
        };
        if let Some((upload, bytes)) = found.remove(id) {
            if let Some(data) = bytes {
                let file = StagedFile {
                    filename: upload.filename.clone(),
                    mime_type: upload.mime_type.clone(),
                    data,
                };
                files.insert(id.clone(), file);
            }
            meta.push(upload);
        }
    }
    Ok((meta, files))
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
        catchup_seq: r.get(5)?,
        backfill_seq: r.get(6)?,
        highestmodseq: r.get::<_, Option<i64>>(7)?.map(|m| m as u64),
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
        thread_id: r.get(11)?,
    })
}

fn located_row(r: &Row) -> rusqlite::Result<Located> {
    let kind: String = r.get(2)?;
    Ok(Located {
        id: r.get(0)?,
        account_id: r.get(1)?,
        folder: FolderKind::parse(&kind).unwrap_or(FolderKind::Inbox),
        folder_name: r.get(3)?,
        uid: r.get(4)?,
        seen: r.get(5)?,
        flagged: r.get(6)?,
        date: r.get(7)?,
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

/// Store-level checks and the 50k sync-page benchmark, in-crate because `Store` is not public:
/// `cargo test -p mail-core --lib -- --ignored bench_store_50k --nocapture`.
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn seeded_db(dir: &tempfile::TempDir) -> (MailDb, Store, i64) {
        let db = MailDb::open(&dir.path().join("mail.sqlite")).unwrap();
        let store = db.user(LOCAL_USER);
        let account = Account {
            id: "a".into(),
            email: "a@test.local".into(),
            name: None,
            provider: None,
            username: "a@test.local".into(),
            auth: Auth::Password,
            imap: Endpoint {
                host: "127.0.0.1".into(),
                port: 993,
                security: Security::Tls,
            },
            smtp: Endpoint {
                host: "127.0.0.1".into(),
                port: 465,
                security: Security::Tls,
            },
            signature: Signature::default(),
        };
        store.insert_account(&account).unwrap();
        store
            .set_folders("a", &[(FolderKind::Inbox, "INBOX".into())])
            .unwrap();
        let folder = store.folder("a", FolderKind::Inbox).unwrap().unwrap();
        (db, store, folder.id)
    }

    fn page(start: u32, count: u32) -> Vec<NewMessage> {
        (start..start + count)
            .map(|i| NewMessage {
                uid: i,
                date: 1_700_000_000_000 + i as i64,
                from: Some(Address {
                    name: None,
                    email: "from@test.local".into(),
                }),
                to: vec![Address {
                    name: None,
                    email: "a@test.local".into(),
                }],
                cc: Vec::new(),
                subject: format!("message {i:05}"),
                snippet: format!("body {i}"),
                message_id: Some(format!("<{i}@test.local>")),
                in_reply_to: None,
                references: Vec::new(),
                seen: false,
                flagged: false,
                has_attachments: false,
                size: 100,
                body: Some(format!("body {i}").into_bytes()),
                index_text: Some(format!("body {i}")),
                gmail_thread: None,
            })
            .collect()
    }

    /// One page per transaction, and a page inserted twice adds nothing.
    #[test]
    fn sync_page_is_atomic_and_keeps_the_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, store, folder_id) = seeded_db(&dir);
        let mut state = FolderRow {
            id: folder_id,
            kind: FolderKind::Inbox,
            name: "INBOX".into(),
            uidvalidity: Some(7),
            uidnext: None,
            catchup_seq: None,
            backfill_seq: None,
            highestmodseq: None,
        };
        state.uidnext = Some(201);
        state.backfill_seq = Some(800);
        let first = store
            .insert_page(folder_id, &page(1, 200), Some(&state))
            .unwrap();
        assert_eq!(first.len(), 200);
        let again = store
            .insert_page(folder_id, &page(1, 200), Some(&state))
            .unwrap();
        assert!(again.is_empty());
        assert_eq!(store.uids(folder_id).unwrap().len(), 200);
        let seen = store.folder("a", FolderKind::Inbox).unwrap().unwrap();
        assert_eq!(
            (seen.uidvalidity, seen.uidnext, seen.backfill_seq),
            (Some(7), Some(201), Some(800))
        );
    }

    #[test]
    #[ignore = "benchmark: seeds 50,000 messages"]
    fn bench_store_50k() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, store, folder_id) = seeded_db(&dir);
        let state = FolderRow {
            id: folder_id,
            kind: FolderKind::Inbox,
            name: "INBOX".into(),
            uidvalidity: Some(1),
            uidnext: Some(50_001),
            catchup_seq: None,
            backfill_seq: None,
            highestmodseq: None,
        };
        let mut per_page = Vec::new();
        let started = Instant::now();
        for start in (1..=50_000).step_by(200) {
            let t = Instant::now();
            store
                .insert_page(folder_id, &page(start, 200), Some(&state))
                .unwrap();
            per_page.push(t.elapsed());
        }
        let total = started.elapsed();
        per_page.sort();
        let p = |q: f64| per_page[(per_page.len() as f64 * q) as usize];
        println!(
            "50k insert: total {total:?}, per 200-page p50 {:?} p95 {:?} max {:?}",
            p(0.5),
            p(0.95),
            per_page.last().unwrap()
        );

        let t = Instant::now();
        let mut before = None;
        let mut seen = 0;
        loop {
            let page = store
                .list(&ListQuery {
                    folder: FolderKind::Inbox,
                    account: Some("a".into()),
                    before,
                    limit: 500,
                })
                .unwrap();
            if page.is_empty() {
                break;
            }
            seen += page.len();
            before = page.last().map(|s| s.id.clone());
        }
        println!(
            "list all {seen} in {:?} ({:?} per 500-page)",
            t.elapsed(),
            t.elapsed() / (seen as u32).div_ceil(500)
        );
        assert_eq!(seen, 50_000);

        let t = Instant::now();
        let hits = store.search("message 49999", 50).unwrap();
        println!(
            "FTS hit {:?} in {:?}",
            hits.first().map(|h| &h.subject),
            t.elapsed()
        );
        assert!(!hits.is_empty());
    }
}
