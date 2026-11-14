//! The notification inbox's values (docs/design/notification-inbox.md §1 and
//! §3): sources, the unified feed and API keys. A backend that cannot serve
//! them answers with [`unavailable`]; `DemoBackend` serves them in memory
//! until the server's endpoints land (the wiring worker's part).

use mail_core::types::*;
use serde::{Deserialize, Serialize};

use crate::types::Ts;

/// Where a notification comes from.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Mail,
    Webhook,
    Gotify,
    Ntfy,
    Telegram,
    Discord,
    Slack,
    Feishu,
    Dingtalk,
    Wecom,
}

impl SourceKind {
    /// Takes an ingest URL token: the URL is shown once, when it is created.
    pub fn ingest(self) -> bool {
        matches!(self, Self::Webhook | Self::Gotify | Self::Ntfy)
    }
}

/// A place notifications come from. A mail account is a source of kind `mail`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub kind: SourceKind,
    pub name: String,
    /// An icon from the app's set; the kind's when left out.
    pub icon: Option<String>,
    /// A theme colour (`chart-1` … `chart-5`), never a raw hex.
    pub color: String,
    /// Paused sources keep their history and take nothing new.
    pub enabled: bool,
    /// Notifications below it are stored but not pushed (0–10, Gotify's scale).
    pub min_priority: u8,
    /// When it was added (ms since the epoch).
    pub created_at: i64,
}

/// One row of the feed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    pub source_id: String,
    pub sender: String,
    pub title: String,
    /// Plain text; HTML is sanitized by the server.
    pub body: String,
    /// Opened in the system browser.
    pub link: Option<String>,
    /// When the service accepted it (ms since the epoch).
    pub received_at: i64,
    /// 0–10, as Gotify and ntfy use it.
    pub priority: u8,
    pub tags: Vec<String>,
    pub read: bool,
    pub archived: bool,
    /// Mail only: the conversation it belongs to.
    pub thread_id: Option<String>,
}

/// A page of the feed, and the counts its filter chips show.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeedPage {
    /// Newest first.
    pub items: Vec<Notification>,
    /// Unread notifications per source, over the whole feed.
    pub counts: std::collections::HashMap<String, u32>,
    /// Unread notifications in the feed.
    pub unread: u32,
    /// Older notifications wait beyond this page.
    pub more: bool,
}

/// What the feed asks for.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeedQuery {
    /// One source's id; every source when left out.
    pub source: Option<String>,
    /// Words to find in sender, title, body or tags.
    pub text: String,
    /// Only unread notifications.
    pub unread: bool,
    /// The id of the previous page's last notification: the next page.
    pub before: Option<String>,
    pub limit: u32,
}

/// What adding a source takes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NewSource {
    pub kind: SourceKind,
    pub name: String,
    pub icon: Option<String>,
    pub color: String,
    pub min_priority: u8,
}

/// A source as created or rotated, with the ingest URL its token makes: the
/// only time the URL is shown.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreatedSource {
    pub source: Source,
    pub url: Option<String>,
}

/// An agent's key to the feed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    /// The key's first characters, so it can be recognised in the list.
    pub prefix: String,
    pub read: bool,
    pub write: bool,
    /// Empty: every source.
    pub source_ids: Vec<String>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub expires_at: Option<i64>,
}

/// What creating a key takes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NewApiKey {
    pub name: String,
    pub write: bool,
    /// Empty: every source.
    pub source_ids: Vec<String>,
    /// Days until it expires; never when left out.
    pub expires_days: Option<u32>,
}

/// A key as created, with its token: the only time the token is shown.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreatedApiKey {
    pub key: ApiKey,
    pub token: String,
}

/// The failure a backend without the notification inbox reports.
pub fn unsupported(what: &'static str) -> Error {
    Error::new(ErrorCode::Unsupported, what)
}

/// The TypeScript of these values, declared by `types::Js` with the mail ones.
pub const DECLARATIONS: &[(&str, &str)] = &[
    (
        "SourceKind",
        "/** Where a notification comes from. */\n\
         export type SourceKind =\n  | \"mail\" | \"webhook\" | \"gotify\" | \"ntfy\"\n  \
         | \"telegram\" | \"discord\" | \"slack\" | \"feishu\" | \"dingtalk\" | \"wecom\";",
    ),
    (
        "Source",
        "/** A place notifications come from: a mail account, a webhook, a bot. */\n\
         export interface Source {\n  id: string;\n  kind: SourceKind;\n  name: string;\n  \
         /** An icon from the app's set; the kind's when null. */\n  icon?: string | null;\n  \
         /** A theme colour (`chart-1` … `chart-5`). */\n  color: string;\n  \
         /** Paused sources keep their history and take nothing new. */\n  enabled: boolean;\n  \
         /** Notifications below it are stored but not pushed (0–10). */\n  minPriority: number;\n  \
         /** When it was added (ms since the epoch). */\n  createdAt: number;\n}",
    ),
    (
        "Notification",
        "/** One row of the feed: a mail message or anything else that reached the inbox. */\n\
         export interface Notification {\n  id: string;\n  sourceId: string;\n  sender: string;\n  title: string;\n  \
         body: string;\n  link?: string | null;\n  /** When the service accepted it (ms since the epoch). */\n  \
         receivedAt: number;\n  /** 0–10, as Gotify and ntfy use it. */\n  priority: number;\n  tags: string[];\n  \
         read: boolean;\n  archived: boolean;\n  /** Mail only: its conversation. */\n  threadId?: string | null;\n}",
    ),
    (
        "FeedPage",
        "/** A page of the feed, and the counts its filter chips show. */\n\
         export interface FeedPage {\n  items: Notification[];\n  /** Unread notifications per source, over the whole feed. */\n  \
         counts: Record<string, number>;\n  /** Unread notifications in the feed. */\n  unread: number;\n  \
         /** Older notifications wait beyond this page. */\n  more: boolean;\n}",
    ),
    (
        "FeedQuery",
        "/** What the feed asks for. */\n\
         export interface FeedQuery {\n  /** One source's id; every source when null. */\n  source?: string | null;\n  \
         /** Words to find in sender, title, body or tags. */\n  text: string;\n  unread: boolean;\n  \
         /** The id of the previous page's last notification: the next page. */\n  before?: string | null;\n  limit: number;\n}",
    ),
    (
        "NewSource",
        "/** What adding a source takes. */\n\
         export interface NewSource {\n  kind: SourceKind;\n  name: string;\n  icon?: string | null;\n  color: string;\n  minPriority: number;\n}",
    ),
    (
        "CreatedSource",
        "/** A source as created or rotated, with the ingest URL its token makes: shown only here. */\n\
         export interface CreatedSource {\n  source: Source;\n  url?: string | null;\n}",
    ),
    (
        "ApiKey",
        "/** An agent's key to the feed. */\n\
         export interface ApiKey {\n  id: string;\n  name: string;\n  /** Its first characters, to recognise it in a list. */\n  prefix: string;\n  \
         read: boolean;\n  write: boolean;\n  /** Empty: every source. */\n  sourceIds: string[];\n  createdAt: number;\n  \
         lastUsedAt?: number | null;\n  expiresAt?: number | null;\n}",
    ),
    (
        "NewApiKey",
        "/** What creating a key takes. */\n\
         export interface NewApiKey {\n  name: string;\n  write: boolean;\n  /** Empty: every source. */\n  sourceIds: string[];\n  \
         /** Days until it expires; never when null. */\n  expiresDays?: number | null;\n}",
    ),
    (
        "CreatedApiKey",
        "/** A key as created, with its token: shown only here. */\n\
         export interface CreatedApiKey {\n  key: ApiKey;\n  token: string;\n}",
    ),
];

macro_rules! named {
    ($($ty:ty => $name:literal),* $(,)?) => {$(
        impl Ts for $ty {
            fn ts() -> String {
                $name.into()
            }
        }
    )*};
}

named! {
    SourceKind => "SourceKind",
    Source => "Source",
    Notification => "Notification",
    FeedPage => "FeedPage",
    FeedQuery => "FeedQuery",
    NewSource => "NewSource",
    CreatedSource => "CreatedSource",
    ApiKey => "ApiKey",
    NewApiKey => "NewApiKey",
    CreatedApiKey => "CreatedApiKey",
}
