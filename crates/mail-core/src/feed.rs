//! The notification feed: `sources` (where messages come from) and `feed_messages` (one unified
//! list), plus the projection that mirrors mail into it. Lives in the same database as mail so a
//! local-mode app has the same model, and so a feed insert and its mail row commit together.
//!
//! Mail is *projected*, never moved: triggers on `accounts`, `folders` and `messages` write the
//! matching feed row in the same transaction, which is why mail code does not change and why a
//! projection bug cannot lose mail — [`MailDb::rebuild_feed`] recreates every row from the mail
//! tables.

use std::sync::{Arc, Mutex};

use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::err;
use crate::store::{MailDb, fts_query, new_id};
use crate::types::{ErrorCode, Result};

/// `config` keys and served JSON use camelCase, like the rest of the client API.
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sources (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    icon TEXT,
    color TEXT,
    config TEXT NOT NULL DEFAULT '{}',
    ingest_token_hash BLOB,
    min_priority INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS sources_by_token ON sources (ingest_token_hash);
CREATE INDEX IF NOT EXISTS sources_by_user ON sources (user_id, kind, created_at);

-- The mail tables already use the name `messages`, so the feed is `feed_messages`.
CREATE TABLE IF NOT EXISTS feed_messages (
    pk INTEGER PRIMARY KEY,
    id TEXT NOT NULL UNIQUE,
    user_id INTEGER NOT NULL,
    source_id TEXT NOT NULL,
    external_id TEXT,
    sender TEXT,
    title TEXT NOT NULL,
    body_text TEXT,
    body_html TEXT,
    body_markdown TEXT,
    link TEXT,
    received_at INTEGER NOT NULL,
    read_at INTEGER,
    archived_at INTEGER,
    priority INTEGER NOT NULL DEFAULT 0,
    tags TEXT NOT NULL DEFAULT '[]',
    payload BLOB,
    thread_id TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS feed_messages_by_external
    ON feed_messages (source_id, external_id) WHERE external_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS feed_messages_by_user ON feed_messages (user_id, received_at DESC, id);
CREATE INDEX IF NOT EXISTS feed_messages_by_source
    ON feed_messages (user_id, source_id, received_at DESC);
CREATE INDEX IF NOT EXISTS feed_messages_unread ON feed_messages (user_id) WHERE read_at IS NULL;
-- Migration marker: the projection is reconciled once, not on every open of a large mailbox.
CREATE TABLE IF NOT EXISTS feed_meta (version INTEGER NOT NULL);
CREATE VIRTUAL TABLE IF NOT EXISTS feed_messages_fts USING fts5 (
    title, sender, body,
    content = '', contentless_delete = 1, tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER IF NOT EXISTS feed_messages_fts_delete AFTER DELETE ON feed_messages BEGIN
    DELETE FROM feed_messages_fts WHERE rowid = old.pk;
END;

-- A mail account is a source of kind `mail`; its id is derived from the account, so both the
-- migration and a rebuild converge on the same row.
CREATE TRIGGER IF NOT EXISTS accounts_source_insert AFTER INSERT ON accounts BEGIN
    INSERT OR IGNORE INTO sources (id, user_id, kind, name, config, created_at)
    VALUES ('src_m_' || new.id, new.user_id, 'mail',
            IFNULL(json_extract(new.data, '$.email'), new.id),
            json_object('accountId', new.id),
            CAST(strftime('%s', 'now') AS INTEGER) * 1000);
END;
CREATE TRIGGER IF NOT EXISTS accounts_source_update AFTER UPDATE OF data ON accounts BEGIN
    UPDATE sources SET name = IFNULL(json_extract(new.data, '$.email'), new.id)
    WHERE id = 'src_m_' || new.id AND user_id = new.user_id;
END;
CREATE TRIGGER IF NOT EXISTS accounts_source_delete AFTER DELETE ON accounts BEGIN
    DELETE FROM feed_messages WHERE user_id = old.user_id AND source_id = 'src_m_' || old.id;
    DELETE FROM sources WHERE id = 'src_m_' || old.id;
END;

-- Drafts and trash are not notifications; everything else mirrors, with the mail id as the feed
-- id so the two tables stay 1:1. Mail in Archive arrives archived: the app moves it there, so the
-- notification is handled and leaves the feed view.
CREATE TRIGGER IF NOT EXISTS messages_feed_insert AFTER INSERT ON messages
WHEN (SELECT kind FROM folders WHERE id = new.folder_id) NOT IN ('drafts', 'trash') BEGIN
    INSERT INTO feed_messages (id, user_id, source_id, sender, title, body_text, received_at,
                               read_at, archived_at, priority, tags, thread_id)
    SELECT new.id, new.user_id, 'src_m_' || f.account_id,
           IFNULL(json_extract(new.from_addr, '$.name'), json_extract(new.from_addr, '$.email')),
           new.subject, new.snippet, new.date,
           CASE WHEN new.seen THEN new.date END,
           CASE WHEN f.kind = 'archive' THEN new.date END,
           0, '[]', new.thread_id
    FROM folders f WHERE f.id = new.folder_id
    ON CONFLICT (id) DO NOTHING;
END;
CREATE TRIGGER IF NOT EXISTS messages_feed_seen AFTER UPDATE OF seen ON messages BEGIN
    UPDATE feed_messages
    SET read_at = CASE WHEN new.seen
                       THEN IFNULL(read_at, CAST(strftime('%s', 'now') AS INTEGER) * 1000) END
    WHERE id = new.id AND user_id = new.user_id;
END;
CREATE TRIGGER IF NOT EXISTS messages_feed_delete AFTER DELETE ON messages BEGIN
    DELETE FROM feed_messages WHERE id = old.id AND user_id = old.user_id;
END;
"#;

/// Bumped when the mail projection needs to be reconciled again at open. 2: mail in Archive is
/// projected as archived.
const SCHEMA_VERSION: i64 = 2;

/// The mail → feed projection, shared by the migration and the rebuild so the two cannot drift.
/// `ON CONFLICT DO NOTHING` with the `NOT EXISTS` check keeps a large mailbox from rewriting rows
/// that are already projected.
const PROJECT_MAIL_SQL: &str = "
INSERT INTO feed_messages (id, user_id, source_id, sender, title, body_text, received_at,
                           read_at, archived_at, priority, tags, thread_id)
SELECT m.id, m.user_id, 'src_m_' || f.account_id,
       IFNULL(json_extract(m.from_addr, '$.name'), json_extract(m.from_addr, '$.email')),
       m.subject, m.snippet, m.date,
       CASE WHEN m.seen THEN m.date END,
       CASE WHEN f.kind = 'archive' THEN m.date END,
       0, '[]', m.thread_id
FROM messages m JOIN folders f ON f.id = m.folder_id
WHERE f.kind NOT IN ('drafts', 'trash')
  AND NOT EXISTS (SELECT 1 FROM feed_messages x WHERE x.id = m.id)
ON CONFLICT (id) DO NOTHING";

/// Every kind the design allows. IM sources are accepted here but only the HTTP kinds ingest yet.
pub const KINDS: [&str; 10] = [
    "mail", "webhook", "gotify", "ntfy", "telegram", "discord", "slack", "feishu", "dingtalk",
    "wecom",
];

/// Kinds whose ingest URL carries a token.
pub const TOKEN_KINDS: [&str; 3] = ["webhook", "gotify", "ntfy"];

pub const TITLE_LIMIT: usize = 1_000;
pub const BODY_LIMIT: usize = 64 * 1024;
pub const LINK_LIMIT: usize = 2_048;
pub const TAGS_LIMIT: usize = 20;
pub const TAG_LIMIT: usize = 64;
/// The raw request body kept for debugging.
pub const PAYLOAD_LIMIT: usize = 64 * 1024;

/// HTML from any source, sanitized like mail (scripts, event handlers, forms, frames, remote
/// images) so the feed can render it without leaking a read receipt.
pub fn sanitize_html(html: &str) -> String {
    crate::mime::sanitize_html(html, &Default::default(), true).0
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    /// Owner; never serialized, since a source only ever leaves the server to its own user.
    #[serde(skip)]
    pub user_id: i64,
    pub kind: String,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub config: Value,
    pub min_priority: i64,
    pub enabled: bool,
    pub created_at: i64,
    /// Whether an ingest token is set; the token itself is only ever returned by the create and
    /// rotate calls.
    pub has_token: bool,
}

impl Source {
    pub fn is_mail(&self) -> bool {
        self.kind == "mail"
    }

    pub fn accepts_token(&self) -> bool {
        TOKEN_KINDS.contains(&self.kind.as_str())
    }

    /// The mail account this source mirrors.
    pub fn account_id(&self) -> Option<&str> {
        self.config.get("accountId")?.as_str()
    }
}

/// What the API sends to create a source.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NewSource {
    pub kind: String,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub config: Option<Value>,
    pub min_priority: Option<i64>,
}

/// Every field left `None` stays as it is.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SourcePatch {
    pub name: Option<String>,
    pub icon: Option<Option<String>>,
    pub color: Option<Option<String>>,
    pub config: Option<Value>,
    pub min_priority: Option<i64>,
    pub enabled: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FeedMessage {
    /// The mail message id when the source is a mail account.
    pub id: String,
    /// Monotonic per user; SSE resumes from it.
    pub seq: i64,
    pub source_id: String,
    pub external_id: Option<String>,
    pub sender: Option<String>,
    pub title: String,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub body_markdown: Option<String>,
    pub link: Option<String>,
    pub received_at: i64,
    pub read_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub priority: i64,
    pub tags: Vec<String>,
    pub thread_id: Option<String>,
}

/// A message to store. `body_html` must already be sanitized; [`Feed::insert`] caps every field.
#[derive(Clone, Debug, Default)]
pub struct NewFeedMessage {
    pub source_id: String,
    pub external_id: Option<String>,
    pub sender: Option<String>,
    pub title: String,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub body_markdown: Option<String>,
    pub link: Option<String>,
    /// Milliseconds since the epoch; [`now_ms`] unless the sender's own timestamp is used.
    pub received_at: i64,
    pub priority: i64,
    pub tags: Vec<String>,
    pub payload: Option<Vec<u8>>,
}

/// Whether an insert stored the message or hit the per-source `external_id` unique index.
#[derive(Clone, Debug)]
pub enum Inserted {
    New(FeedMessage),
    Duplicate(FeedMessage),
}

impl Inserted {
    pub fn message(&self) -> &FeedMessage {
        match self {
            Inserted::New(m) | Inserted::Duplicate(m) => m,
        }
    }
}

/// Row counts of a feed rebuild, per user.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeedCounts {
    pub users: i64,
    pub mail_sources: i64,
    pub sources: i64,
    pub mail_messages: i64,
    pub projected: i64,
    pub feed_messages: i64,
}

/// Where a page starts: the last row of the page before, in the feed's order (newest first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub received_at: i64,
    pub id: String,
}

/// Which archived rows a query sees. Archived means handled: the notification left the feed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Archived {
    #[default]
    Hide,
    Only,
    Any,
}

/// A page of the feed, newest first.
#[derive(Clone, Debug)]
pub struct FeedPage {
    pub messages: Vec<FeedMessage>,
    /// Rows wait beyond this page: pass the last message as the next [`ListFilter::before`].
    pub more: bool,
}

/// One feed query; every field narrows. The defaults are the app's feed view: newest first,
/// archived hidden.
#[derive(Clone, Debug, Default)]
pub struct ListFilter {
    /// The sources the caller may see (an API key's scope); empty or `None` is every source.
    pub sources: Option<Vec<String>>,
    /// One source on top of [`sources`](Self::sources).
    pub source: Option<String>,
    /// Only unread notifications that are still in the feed.
    pub unread: bool,
    pub archived: Archived,
    /// Words to find in title, sender and body, over both full-text indexes.
    pub text: Option<String>,
    /// Only notifications received after this (ms since the epoch).
    pub since: Option<i64>,
    pub before: Option<Cursor>,
    /// 1–[`LIST_LIMIT`].
    pub limit: u32,
}

/// The most one page may hold.
pub const LIST_LIMIT: u32 = 200;

/// One user's view of the feed, bound by [`MailDb::feed`] so no query can touch another user's
/// rows: every statement binds `user_id` as `?1`.
#[derive(Clone)]
pub struct Feed {
    conn: Arc<Mutex<Connection>>,
    user: i64,
}

const SOURCE_COLUMNS: &str = "id, user_id, kind, name, icon, color, config, min_priority, \
     enabled, created_at, ingest_token_hash IS NOT NULL";

const MESSAGE_COLUMNS: &str = "id, pk, source_id, external_id, sender, title, body_text, \
     body_html, body_markdown, link, received_at, read_at, archived_at, priority, tags, thread_id";

impl MailDb {
    /// Runs the feed migration (create the tables, triggers and sources, project every mail row
    /// that has none) and hands back this user's feed.
    pub(crate) fn migrate_feed(conn: &mut Connection) -> Result<()> {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(SCHEMA)?;
        let version: i64 =
            tx.query_row("SELECT IFNULL(MAX(version), 0) FROM feed_meta", [], |r| {
                r.get(0)
            })?;
        if version >= SCHEMA_VERSION {
            return tx.commit().map_err(Into::into);
        }
        if version < 2 {
            // Version 1 predates archived mail in the feed: mail already in Archive joins it
            // archived, so it does not show up as a new notification.
            tx.execute(
                "UPDATE feed_messages SET archived_at =
                    (SELECT m.date FROM messages m WHERE m.id = feed_messages.id)
                 WHERE archived_at IS NULL
                   AND source_id IN (SELECT id FROM sources WHERE kind = 'mail')
                   AND id IN (SELECT m.id FROM messages m JOIN folders f ON f.id = m.folder_id
                              WHERE f.kind = 'archive')",
                [],
            )?;
        }
        tx.execute(
            "INSERT OR IGNORE INTO sources (id, user_id, kind, name, config, created_at)
             SELECT 'src_m_' || id, user_id, 'mail', IFNULL(json_extract(data, '$.email'), id),
                    json_object('accountId', id), CAST(strftime('%s', 'now') AS INTEGER) * 1000
             FROM accounts",
            [],
        )?;
        tx.execute(PROJECT_MAIL_SQL, [])?;
        tx.execute(
            "DELETE FROM feed_messages
             WHERE source_id IN (SELECT id FROM sources WHERE kind = 'mail')
               AND id NOT IN (SELECT m.id FROM messages m JOIN folders f ON f.id = m.folder_id
                              WHERE f.kind NOT IN ('drafts', 'trash'))",
            [],
        )?;
        check_mail_projection(&tx)?;
        tx.execute(
            "INSERT INTO feed_meta (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn feed(&self, user: i64) -> Feed {
        Feed {
            conn: self.conn.clone(),
            user,
        }
    }

    /// The source behind an ingest token, found by the SHA-256 of the token. The only lookup that
    /// is not bound to a user: the token says whose source it is.
    pub fn source_for_token(&self, hash: &[u8]) -> Result<Option<Source>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.query_row(
            &format!("SELECT {SOURCE_COLUMNS} FROM sources WHERE ingest_token_hash = ?1"),
            [hash],
            source_row,
        )
        .optional()
        .map_err(Into::into)
    }

    /// Recreates every mail projection from the mail tables, then reports what is there. Mail
    /// rows themselves are only read.
    pub fn rebuild_feed(&self) -> Result<FeedCounts> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(SCHEMA)?;
        tx.execute(
            "DELETE FROM feed_messages WHERE source_id IN (SELECT id FROM sources WHERE kind = 'mail')",
            [],
        )?;
        tx.execute(
            "DELETE FROM feed_messages_fts WHERE rowid NOT IN (SELECT pk FROM feed_messages)",
            [],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO sources (id, user_id, kind, name, config, created_at)
             SELECT 'src_m_' || id, user_id, 'mail', IFNULL(json_extract(data, '$.email'), id),
                    json_object('accountId', id), CAST(strftime('%s', 'now') AS INTEGER) * 1000
             FROM accounts",
            [],
        )?;
        tx.execute(
            "DELETE FROM sources
             WHERE kind = 'mail' AND id NOT IN (SELECT 'src_m_' || id FROM accounts)",
            [],
        )?;
        tx.execute(PROJECT_MAIL_SQL, [])?;
        check_mail_projection(&tx)?;
        tx.execute("DELETE FROM feed_meta", [])?;
        tx.execute(
            "INSERT INTO feed_meta (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )?;
        let counts = read_counts(&tx)?;
        tx.commit()?;
        Ok(counts)
    }
}

/// A projection that dropped or doubled a row is a bug that must stop the server rather than
/// silently lose notifications.
fn check_mail_projection(conn: &Connection) -> Result<()> {
    let (mail, projected): (i64, i64) = conn.query_row(
        "SELECT (SELECT count(*) FROM messages m JOIN folders f ON f.id = m.folder_id
                 WHERE f.kind NOT IN ('drafts', 'trash')),
                (SELECT count(*) FROM feed_messages x JOIN sources s ON s.id = x.source_id
                 WHERE s.kind = 'mail')",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if mail != projected {
        return Err(err(
            ErrorCode::Storage,
            format!(
                "the mail projection holds {projected} of {mail} mail messages; run \
                 `nsh-mail-server rebuild-feed`"
            ),
        ));
    }
    Ok(())
}

fn read_counts(conn: &Connection) -> Result<FeedCounts> {
    let one = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [], |r| r.get(0))?) };
    Ok(FeedCounts {
        users: one("SELECT count(DISTINCT user_id) FROM sources")?,
        mail_sources: one("SELECT count(*) FROM sources WHERE kind = 'mail'")?,
        sources: one("SELECT count(*) FROM sources")?,
        mail_messages: one(
            "SELECT count(*) FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE f.kind NOT IN ('drafts', 'trash')",
        )?,
        projected: one(
            "SELECT count(*) FROM feed_messages x JOIN sources s ON s.id = x.source_id
             WHERE s.kind = 'mail'",
        )?,
        feed_messages: one("SELECT count(*) FROM feed_messages")?,
    })
}

impl Feed {
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn sources(&self) -> Result<Vec<Source>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SOURCE_COLUMNS} FROM sources WHERE user_id = ?1 ORDER BY created_at, id"
        ))?;
        let rows = stmt.query_map([self.user], source_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn source(&self, id: &str) -> Result<Source> {
        let conn = self.conn();
        conn.query_row(
            &format!("SELECT {SOURCE_COLUMNS} FROM sources WHERE user_id = ?1 AND id = ?2"),
            params![self.user, id],
            source_row,
        )
        .optional()?
        .ok_or_else(|| not_found("source", id))
    }

    pub fn create_source(&self, new: &NewSource, token_hash: Option<&[u8]>) -> Result<Source> {
        if !KINDS.contains(&new.kind.as_str()) || new.kind == "mail" {
            return Err(err(
                ErrorCode::InvalidInput,
                format!(
                    "cannot create a source of kind {:?}; mail sources follow their account and \
                     the others are one of {}",
                    new.kind,
                    KINDS[1..].join(", ")
                ),
            ));
        }
        let name = new.name.trim();
        if name.is_empty() {
            return Err(err(ErrorCode::InvalidInput, "a source needs a name"));
        }
        let config = new
            .config
            .clone()
            .unwrap_or_else(|| Value::Object(Default::default()));
        if !config.is_object() {
            return Err(err(ErrorCode::InvalidInput, "config must be a JSON object"));
        }
        let id = format!("src_{}", new_id());
        self.conn().execute(
            "INSERT INTO sources (id, user_id, kind, name, icon, color, config,
                                  ingest_token_hash, min_priority, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10)",
            params![
                id,
                self.user,
                new.kind,
                name,
                new.icon,
                new.color,
                config.to_string(),
                token_hash,
                new.min_priority.unwrap_or(0).clamp(0, 10),
                now_ms()
            ],
        )?;
        self.source(&id)
    }

    pub fn update_source(&self, id: &str, patch: &SourcePatch) -> Result<Source> {
        let mut source = self.source(id)?;
        if let Some(name) = &patch.name {
            if name.trim().is_empty() {
                return Err(err(ErrorCode::InvalidInput, "a source needs a name"));
            }
            source.name = name.trim().to_string();
        }
        if let Some(icon) = &patch.icon {
            source.icon.clone_from(icon);
        }
        if let Some(color) = &patch.color {
            source.color.clone_from(color);
        }
        if let Some(config) = &patch.config {
            if !config.is_object() {
                return Err(err(ErrorCode::InvalidInput, "config must be a JSON object"));
            }
            if source.is_mail() {
                return Err(err(
                    ErrorCode::InvalidInput,
                    "a mail source follows its account; change the account instead",
                ));
            }
            source.config = config.clone();
        }
        if let Some(p) = patch.min_priority {
            source.min_priority = p.clamp(0, 10);
        }
        if let Some(enabled) = patch.enabled {
            source.enabled = enabled;
        }
        self.conn().execute(
            "UPDATE sources SET name = ?3, icon = ?4, color = ?5, config = ?6, min_priority = ?7,
                    enabled = ?8
             WHERE user_id = ?1 AND id = ?2",
            params![
                self.user,
                id,
                source.name,
                source.icon,
                source.color,
                source.config.to_string(),
                source.min_priority,
                source.enabled
            ],
        )?;
        self.source(id)
    }

    /// Deletes the source with its messages; mail sources go with their account.
    pub fn delete_source(&self, id: &str) -> Result<()> {
        let source = self.source(id)?;
        if source.is_mail() {
            return Err(err(
                ErrorCode::InvalidInput,
                "a mail source is deleted with its account, not on its own",
            ));
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM feed_messages WHERE user_id = ?1 AND source_id = ?2",
            params![self.user, id],
        )?;
        tx.execute(
            "DELETE FROM sources WHERE user_id = ?1 AND id = ?2",
            params![self.user, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Replaces the token (rotation). The caller generates it and only stores the hash.
    pub fn set_token_hash(&self, id: &str, hash: &[u8]) -> Result<Source> {
        let source = self.source(id)?;
        if !source.accepts_token() {
            return Err(err(
                ErrorCode::InvalidInput,
                format!("a {} source has no ingest token", source.kind),
            ));
        }
        self.conn().execute(
            "UPDATE sources SET ingest_token_hash = ?3 WHERE user_id = ?1 AND id = ?2",
            params![self.user, id, hash],
        )?;
        self.source(id)
    }

    /// Stores a message, or returns the existing one when `(source_id, external_id)` is already
    /// there (a sender's retry). The text fields are capped, so an oversized body cannot bloat
    /// the database.
    pub fn insert(&self, m: &NewFeedMessage) -> Result<Inserted> {
        let source = self.source(&m.source_id)?;
        let id = format!("msg_{}", new_id());
        let tags = capped_tags(&m.tags);
        let mut conn = self.conn();
        let inserted = {
            let tx = conn.transaction()?;
            let n = tx.execute(
                "INSERT INTO feed_messages (id, user_id, source_id, external_id, sender, title,
                                        body_text, body_html, body_markdown, link, received_at,
                                        priority, tags, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT (source_id, external_id) WHERE external_id IS NOT NULL DO NOTHING",
                params![
                    id,
                    self.user,
                    m.source_id,
                    m.external_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|e| !e.is_empty()),
                    m.sender.as_deref().map(|s| truncate(s, TITLE_LIMIT)),
                    truncate(
                        if m.title.trim().is_empty() {
                            "(no title)"
                        } else {
                            &m.title
                        },
                        TITLE_LIMIT
                    ),
                    m.body_text.as_deref().map(|s| truncate(s, BODY_LIMIT)),
                    m.body_html.as_deref().map(|s| truncate(s, BODY_LIMIT)),
                    m.body_markdown.as_deref().map(|s| truncate(s, BODY_LIMIT)),
                    m.link.as_deref().map(|s| truncate(s, LINK_LIMIT)),
                    m.received_at,
                    m.priority.clamp(0, 10),
                    tags.to_string(),
                    m.payload
                        .as_deref()
                        .map(|p| &p[..p.len().min(PAYLOAD_LIMIT)]),
                ],
            )?;
            if n == 1 {
                let pk = tx.last_insert_rowid();
                if !source.is_mail() {
                    let body = m
                        .body_text
                        .as_deref()
                        .or(m.body_markdown.as_deref())
                        .unwrap_or_default();
                    tx.execute(
                        "INSERT INTO feed_messages_fts (rowid, title, sender, body)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![
                            pk,
                            fts_text(&m.title),
                            fts_text(m.sender.as_deref().unwrap_or_default()),
                            fts_text(body)
                        ],
                    )?;
                }
            }
            tx.commit()?;
            n == 1
        };
        drop(conn);
        let stored = if inserted {
            self.message(&id)
        } else {
            self.by_external(&m.source_id, m.external_id.as_deref().unwrap_or_default())
        };
        stored.map(|m| {
            if inserted {
                Inserted::New(m)
            } else {
                Inserted::Duplicate(m)
            }
        })
    }

    pub fn by_external(&self, source_id: &str, external_id: &str) -> Result<FeedMessage> {
        let conn = self.conn();
        conn.query_row(
            &format!(
                "SELECT {MESSAGE_COLUMNS} FROM feed_messages
                 WHERE user_id = ?1 AND source_id = ?2 AND external_id = ?3"
            ),
            params![self.user, source_id, external_id],
            message_row,
        )
        .optional()?
        .ok_or_else(|| {
            err(
                ErrorCode::NotFound,
                format!("no message of {source_id} with external id {external_id}"),
            )
        })
    }

    pub fn message(&self, id: &str) -> Result<FeedMessage> {
        let conn = self.conn();
        conn.query_row(
            &format!("SELECT {MESSAGE_COLUMNS} FROM feed_messages WHERE user_id = ?1 AND id = ?2"),
            params![self.user, id],
            message_row,
        )
        .optional()?
        .ok_or_else(|| not_found("message", id))
    }

    /// The request body kept for debugging; empty when none was stored.
    pub fn payload(&self, id: &str) -> Result<Vec<u8>> {
        let conn = self.conn();
        Ok(conn
            .query_row(
                "SELECT payload FROM feed_messages WHERE user_id = ?1 AND id = ?2",
                params![self.user, id],
                |r| r.get::<_, Option<Vec<u8>>>(0),
            )
            .optional()?
            .flatten()
            .unwrap_or_default())
    }

    /// Marks a message read or unread. A mail message's own flag is the source of truth, so the
    /// mail row is updated and the trigger carries it into the feed.
    pub fn set_read(&self, id: &str, read: bool) -> Result<FeedMessage> {
        let message = self.message(id)?;
        let mut conn = self.conn();
        {
            let tx = conn.transaction()?;
            let mail: bool = tx.query_row(
                "SELECT kind = 'mail' FROM sources WHERE user_id = ?1 AND id = ?2",
                params![self.user, message.source_id],
                |r| r.get(0),
            )?;
            if mail {
                tx.execute(
                    "UPDATE messages SET seen = ?3 WHERE user_id = ?1 AND id = ?2",
                    params![self.user, id, read],
                )?;
            } else {
                tx.execute(
                    "UPDATE feed_messages SET read_at = ?3 WHERE user_id = ?1 AND id = ?2",
                    params![self.user, id, read.then(now_ms)],
                )?;
            }
            tx.commit()?;
        }
        drop(conn);
        self.message(id)
    }

    /// Hides a message from the feed. Mail moves between folders, which the mail engine owns, so
    /// for a mail row this only affects the feed copy.
    pub fn set_archived(&self, id: &str, archived: bool) -> Result<FeedMessage> {
        self.message(id)?;
        self.conn().execute(
            "UPDATE feed_messages SET archived_at = ?3 WHERE user_id = ?1 AND id = ?2",
            params![self.user, id, archived.then(now_ms)],
        )?;
        self.message(id)
    }

    /// Messages received after `after_seq`, oldest first, for catching a stream up (`since=`).
    pub fn since(
        &self,
        source_id: Option<&str>,
        after_seq: i64,
        limit: u32,
    ) -> Result<Vec<FeedMessage>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM feed_messages
             WHERE user_id = ?1 AND pk > ?2 AND (?3 IS NULL OR source_id = ?3)
             ORDER BY pk LIMIT ?4"
        ))?;
        let rows = stmt.query_map(
            params![self.user, after_seq, source_id, limit.min(500)],
            message_row,
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// One page of the feed, newest first, with every filter applied in one statement so keyset
    /// paging stays cheap on a large mailbox.
    pub fn list(&self, filter: &ListFilter) -> Result<FeedPage> {
        let limit = filter.limit.clamp(1, LIST_LIMIT) as i64;
        let scope = source_scope(filter);
        if scope.as_ref().is_some_and(Vec::is_empty) {
            return Ok(FeedPage {
                messages: Vec::new(),
                more: false,
            });
        }
        let mut sql = format!("SELECT {MESSAGE_COLUMNS} FROM feed_messages WHERE user_id = ?1");
        let mut args: Vec<SqlValue> = vec![SqlValue::Integer(self.user)];
        let bind = |args: &mut Vec<SqlValue>, value: SqlValue| {
            args.push(value);
            format!("?{}", args.len())
        };
        if let Some(ids) = &scope {
            let mut list = String::from(" AND source_id IN (");
            for (i, id) in ids.iter().enumerate() {
                if i > 0 {
                    list.push(',');
                }
                list.push_str(&bind(&mut args, SqlValue::Text(id.clone())));
            }
            list.push(')');
            sql.push_str(&list);
        }
        if filter.unread {
            sql.push_str(" AND read_at IS NULL AND archived_at IS NULL");
        }
        match filter.archived {
            Archived::Hide if !filter.unread => sql.push_str(" AND archived_at IS NULL"),
            Archived::Only => sql.push_str(" AND archived_at IS NOT NULL"),
            _ => {}
        }
        if let Some(since) = filter.since {
            let p = bind(&mut args, SqlValue::Integer(since));
            sql.push_str(&format!(" AND received_at > {p}"));
        }
        if let Some(before) = &filter.before {
            let at = bind(&mut args, SqlValue::Integer(before.received_at));
            let id = bind(&mut args, SqlValue::Text(before.id.clone()));
            sql.push_str(&format!(
                " AND (received_at < {at} OR (received_at = {at} AND id < {id}))"
            ));
        }
        let text = filter
            .text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());
        if let Some(text) = text {
            let Some(query) = fts_query(text) else {
                return Ok(FeedPage {
                    messages: Vec::new(),
                    more: false,
                });
            };
            // Mail text lives in the mail index, everything else in the feed's own; one page
            // spans both, so a search does not have to say where a notification came from.
            let mails = bind(&mut args, SqlValue::Text(query.clone()));
            let users = bind(&mut args, SqlValue::Integer(self.user));
            let feeds = bind(&mut args, SqlValue::Text(query));
            let their_user = bind(&mut args, SqlValue::Integer(self.user));
            sql.push_str(&format!(
                " AND id IN (
                    SELECT m.id FROM messages_fts JOIN messages m ON m.pk = messages_fts.rowid
                      JOIN folders f ON f.id = m.folder_id
                     WHERE messages_fts MATCH {mails} AND m.user_id = {users}
                       AND f.kind NOT IN ('drafts', 'trash')
                    UNION
                    SELECT y.id FROM feed_messages_fts
                      JOIN feed_messages y ON y.pk = feed_messages_fts.rowid
                     WHERE feed_messages_fts MATCH {feeds} AND y.user_id = {their_user})"
            ));
        }
        let take = bind(&mut args, SqlValue::Integer(limit + 1));
        sql.push_str(&format!(" ORDER BY received_at DESC, id DESC LIMIT {take}"));
        let conn = self.conn();
        let mut stmt = conn.prepare(&sql)?;
        let mut messages = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), message_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let more = messages.len() as i64 > limit;
        messages.truncate(limit as usize);
        Ok(FeedPage { messages, more })
    }

    /// The message a page's `next` continues from, by its id.
    pub fn cursor_of(&self, id: &str) -> Result<Cursor> {
        let message = self.message(id)?;
        Ok(Cursor {
            received_at: message.received_at,
            id: message.id,
        })
    }

    /// Unread count across the feed, and per source.
    pub fn unread(&self) -> Result<(i64, Vec<(String, i64)>)> {
        let conn = self.conn();
        let total = conn.query_row(
            "SELECT count(*) FROM feed_messages
             WHERE user_id = ?1 AND read_at IS NULL AND archived_at IS NULL",
            [self.user],
            |r| r.get(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT source_id, count(*) FROM feed_messages
             WHERE user_id = ?1 AND read_at IS NULL AND archived_at IS NULL GROUP BY source_id",
        )?;
        let per = stmt
            .query_map([self.user], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok((total, per))
    }
}

/// The sources a query may read: the caller's scope narrowed to the one source it asked for.
/// `None` is every source; an empty list matches nothing.
fn source_scope(filter: &ListFilter) -> Option<Vec<String>> {
    match (&filter.sources, &filter.source) {
        (Some(scope), _) if !scope.is_empty() => Some(match &filter.source {
            Some(one) if scope.contains(one) => vec![one.clone()],
            Some(_) => Vec::new(),
            None => scope.clone(),
        }),
        (_, Some(one)) => Some(vec![one.clone()]),
        _ => None,
    }
}

fn not_found(what: &str, id: &str) -> crate::types::Error {
    err(ErrorCode::NotFound, format!("no {what} with id {id}"))
}

fn source_row(r: &Row) -> rusqlite::Result<Source> {
    let config: String = r.get(6)?;
    Ok(Source {
        id: r.get(0)?,
        user_id: r.get(1)?,
        kind: r.get(2)?,
        name: r.get(3)?,
        icon: r.get(4)?,
        color: r.get(5)?,
        config: serde_json::from_str(&config).unwrap_or(Value::Null),
        min_priority: r.get(7)?,
        enabled: r.get(8)?,
        created_at: r.get(9)?,
        has_token: r.get(10)?,
    })
}

fn message_row(r: &Row) -> rusqlite::Result<FeedMessage> {
    let tags: String = r.get(14)?;
    Ok(FeedMessage {
        id: r.get(0)?,
        seq: r.get(1)?,
        source_id: r.get(2)?,
        external_id: r.get(3)?,
        sender: r.get(4)?,
        title: r.get(5)?,
        body_text: r.get(6)?,
        body_html: r.get(7)?,
        body_markdown: r.get(8)?,
        link: r.get(9)?,
        received_at: r.get(10)?,
        read_at: r.get(11)?,
        archived_at: r.get(12)?,
        priority: r.get(13)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        thread_id: r.get(15)?,
    })
}

fn truncate(s: &str, limit: usize) -> String {
    if s.len() <= limit {
        return s.to_string();
    }
    let mut end = limit;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn capped_tags(tags: &[String]) -> Value {
    Value::Array(
        tags.iter()
            .filter(|t| !t.trim().is_empty())
            .take(TAGS_LIMIT)
            .map(|t| Value::String(truncate(t.trim(), TAG_LIMIT)))
            .collect(),
    )
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0x20000..=0x2FAFF)
}

/// unicode61 keeps a run of CJK characters as one token; splitting them lets a search find any
/// character, as the mail index does.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{LOCAL_USER, Store};
    use crate::types::*;

    fn account(id: &str) -> Account {
        Account {
            id: id.into(),
            email: format!("{id}@test.local"),
            name: None,
            provider: None,
            username: format!("{id}@test.local"),
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
        }
    }

    fn message(uid: u32, seen: bool) -> crate::store::NewMessage {
        crate::store::NewMessage {
            uid,
            date: 1_700_000_000_000 + uid as i64,
            from: Some(Address {
                name: Some("Sender".into()),
                email: "sender@test.local".into(),
            }),
            to: vec![Address {
                name: None,
                email: "a@test.local".into(),
            }],
            cc: Vec::new(),
            subject: format!("subject {uid}"),
            snippet: format!("body {uid}"),
            message_id: Some(format!("<{uid}@test.local>")),
            in_reply_to: None,
            references: Vec::new(),
            seen,
            flagged: false,
            has_attachments: false,
            size: 10,
            body: None,
            index_text: None,
            gmail_thread: None,
        }
    }

    fn seeded(user: i64) -> (tempfile::TempDir, MailDb, Store) {
        let dir = tempfile::tempdir().unwrap();
        let db = MailDb::open(&dir.path().join("mail.sqlite")).unwrap();
        let store = db.user(user);
        store.insert_account(&account("a")).unwrap();
        store
            .set_folders("a", &[(FolderKind::Inbox, "INBOX".into())])
            .unwrap();
        (dir, db, store)
    }

    /// A mail insert, flag change and delete each reach the feed in the same transaction, and a
    /// draft never does.
    #[test]
    fn mail_is_projected() {
        let (_dir, db, store) = seeded(LOCAL_USER);
        let feed = db.feed(LOCAL_USER);
        let inbox = store.folder("a", FolderKind::Inbox).unwrap().unwrap();
        let ids: Vec<String> = store
            .insert_page(inbox.id, &[message(1, false), message(2, true)], None)
            .unwrap()
            .iter()
            .map(|s| s.id.clone())
            .collect();
        let sources = feed.sources().unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].kind, "mail");
        assert_eq!(sources[0].name, "a@test.local");
        assert_eq!(sources[0].account_id(), Some("a"));

        let first = feed.message(&ids[0]).unwrap();
        assert_eq!(first.source_id, sources[0].id);
        assert_eq!(first.sender.as_deref(), Some("Sender"));
        assert_eq!(first.title, store.summary(&ids[0]).unwrap().subject);
        assert_eq!(first.read_at, None);
        assert_eq!(
            feed.message(&ids[1]).unwrap().read_at,
            Some(1_700_000_000_002)
        );

        store
            .set_local_flags(
                &ids[0],
                Flags {
                    seen: Some(true),
                    flagged: None,
                },
            )
            .unwrap();
        assert!(feed.message(&ids[0]).unwrap().read_at.is_some());
        store
            .set_local_flags(
                &ids[0],
                Flags {
                    seen: Some(false),
                    flagged: None,
                },
            )
            .unwrap();
        assert_eq!(feed.message(&ids[0]).unwrap().read_at, None);

        store
            .set_folders("a", &[(FolderKind::Drafts, "Drafts".into())])
            .unwrap();
        let drafts = store.folder("a", FolderKind::Drafts).unwrap().unwrap();
        let draft: Vec<String> = store
            .insert_page(drafts.id, &[message(3, false)], None)
            .unwrap()
            .iter()
            .map(|s| s.id.clone())
            .collect();
        assert!(matches!(
            feed.message(&draft[0]),
            Err(e) if e.code == ErrorCode::NotFound
        ));

        store.delete_message(&ids[0]).unwrap();
        assert!(feed.message(&ids[0]).is_err());
        store.delete_account("a").unwrap();
        assert!(feed.sources().unwrap().is_empty());
        assert_eq!(feed.message(&ids[1]).unwrap_err().code, ErrorCode::NotFound);
    }

    /// A database written before the feed exists migrates in place: sources for existing accounts,
    /// one row per mail message, and a second open changes nothing.
    #[test]
    fn migration_projects_an_old_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mail.sqlite");
        {
            let db = MailDb::open(&path).unwrap();
            let store = db.user(LOCAL_USER);
            store.insert_account(&account("a")).unwrap();
            store
                .set_folders("a", &[(FolderKind::Inbox, "INBOX".into())])
                .unwrap();
            let inbox = store.folder("a", FolderKind::Inbox).unwrap().unwrap();
            store
                .insert_page(inbox.id, &[message(1, false), message(2, false)], None)
                .unwrap();
            drop(db);
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "DROP TRIGGER messages_feed_insert;
                 DROP TRIGGER messages_feed_seen;
                 DROP TRIGGER messages_feed_delete;
                 DROP TRIGGER accounts_source_insert;
                 DROP TRIGGER accounts_source_update;
                 DROP TRIGGER accounts_source_delete;
                 DROP TRIGGER feed_messages_fts_delete;
                 DROP TABLE feed_messages_fts;
                 DROP TABLE feed_messages;
                 DROP TABLE feed_meta;
                 DROP TABLE sources;",
            )
            .unwrap();
        }
        for _ in 0..2 {
            let db = MailDb::open(&path).unwrap();
            let feed = db.feed(LOCAL_USER);
            assert_eq!(feed.sources().unwrap().len(), 1);
            let ids: Vec<String> = db
                .user(LOCAL_USER)
                .list(&ListQuery {
                    folder: FolderKind::Inbox,
                    account: None,
                    before: None,
                    limit: 50,
                })
                .unwrap()
                .iter()
                .map(|s| s.id.clone())
                .collect();
            assert_eq!(ids.len(), 2);
            for id in ids {
                assert!(feed.message(&id).unwrap().title.starts_with("subject"));
            }
        }
    }

    /// Rebuilding restores a damaged projection from the mail tables alone.
    #[test]
    fn rebuild_restores_the_projection() {
        let (_dir, db, store) = seeded(LOCAL_USER);
        let inbox = store.folder("a", FolderKind::Inbox).unwrap().unwrap();
        store
            .insert_page(inbox.id, &[message(1, true), message(2, false)], None)
            .unwrap();
        let feed = db.feed(LOCAL_USER);
        let before = db.rebuild_feed().unwrap();
        assert_eq!(before.mail_messages, 2);
        assert_eq!(before.projected, 2);
        assert_eq!(feed.unread().unwrap().0, 1);

        Connection::open(_dir.path().join("mail.sqlite"))
            .unwrap()
            .execute("DELETE FROM feed_messages", [])
            .unwrap();
        let rebuilt = db.rebuild_feed().unwrap();
        assert_eq!(rebuilt.feed_messages, 2);
        assert_eq!(feed.unread().unwrap().0, 1);
        assert_eq!(db.rebuild_feed().unwrap(), rebuilt);
    }

    #[test]
    fn sources_are_per_user_and_tokens_rotate() {
        let dir = tempfile::tempdir().unwrap();
        let db = MailDb::open(&dir.path().join("mail.sqlite")).unwrap();
        let (one, two) = (db.feed(1), db.feed(2));
        let source = one
            .create_source(
                &NewSource {
                    kind: "webhook".into(),
                    name: "CI".into(),
                    ..Default::default()
                },
                Some(b"hash-one"),
            )
            .unwrap();
        assert!(source.has_token);
        assert_eq!(two.sources().unwrap().len(), 0);
        assert!(two.source(&source.id).is_err());
        assert!(db.source_for_token(b"hash-one").unwrap().is_some());
        assert!(db.source_for_token(b"hash-two").unwrap().is_none());

        let rotated = one.set_token_hash(&source.id, b"hash-two").unwrap();
        assert!(rotated.has_token);
        assert!(db.source_for_token(b"hash-one").unwrap().is_none());
        assert_eq!(
            db.source_for_token(b"hash-two").unwrap().unwrap().id,
            source.id
        );

        let renamed = one
            .update_source(
                &source.id,
                &SourcePatch {
                    name: Some("Builds".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(renamed.name, "Builds");
        one.delete_source(&source.id).unwrap();
        assert!(db.source_for_token(b"hash-two").unwrap().is_none());
        assert!(one.sources().unwrap().is_empty());
    }

    #[test]
    fn inserts_dedup_cap_and_mark_read() {
        let dir = tempfile::tempdir().unwrap();
        let db = MailDb::open(&dir.path().join("mail.sqlite")).unwrap();
        let feed = db.feed(1);
        let source = feed
            .create_source(
                &NewSource {
                    kind: "ntfy".into(),
                    name: "ntfy".into(),
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        let m = NewFeedMessage {
            source_id: source.id.clone(),
            external_id: Some("abc".into()),
            title: "Deploy".into(),
            body_text: Some("x".repeat(BODY_LIMIT + 10)),
            priority: 42,
            tags: (0..30).map(|i| format!("tag{i}")).collect(),
            received_at: 1,
            ..Default::default()
        };
        let Inserted::New(stored) = feed.insert(&m).unwrap() else {
            panic!("first insert must store");
        };
        assert_eq!(stored.priority, 10);
        assert_eq!(stored.tags.len(), TAGS_LIMIT);
        assert_eq!(stored.body_text.unwrap().len(), BODY_LIMIT);
        let Inserted::Duplicate(again) = feed.insert(&m).unwrap() else {
            panic!("a repeated external id must not store");
        };
        assert_eq!(again.id, stored.id);
        assert_eq!(feed.unread().unwrap().0, 1);
        assert!(feed.set_read(&stored.id, true).unwrap().read_at.is_some());
        assert_eq!(feed.unread().unwrap().0, 0);
        assert!(db.feed(2).message(&stored.id).is_err());
    }
}
