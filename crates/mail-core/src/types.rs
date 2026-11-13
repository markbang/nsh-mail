//! Serde types shared with clients (`nsh-mail/native`, the server REST API, the web CloudBackend).
//! Field names serialize in camelCase to match the TSX interface in docs/design.md §3.

use serde::{Deserialize, Serialize};
use std::fmt;

pub type AccountId = String;
pub type MessageId = String;
pub type ThreadId = String;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum FolderKind {
    Inbox,
    Sent,
    Drafts,
    Archive,
    Trash,
}

impl FolderKind {
    pub const ALL: [FolderKind; 5] = [
        FolderKind::Inbox,
        FolderKind::Sent,
        FolderKind::Drafts,
        FolderKind::Archive,
        FolderKind::Trash,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            FolderKind::Inbox => "inbox",
            FolderKind::Sent => "sent",
            FolderKind::Drafts => "drafts",
            FolderKind::Archive => "archive",
            FolderKind::Trash => "trash",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// Transport security. There is deliberately no plaintext variant.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    Tls,
    StartTls,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub security: Security,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum OAuthProvider {
    Google,
    Microsoft,
}

impl OAuthProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            OAuthProvider::Google => "google",
            OAuthProvider::Microsoft => "microsoft",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Auth {
    Password,
    OAuth { provider: OAuthProvider },
}

/// Result of `detect(email)`: what the add-account form should be prefilled with.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerSettings {
    /// Preset id (`gmail`, `outlook`, `qq`, ...) or `None` when it came from autoconfig.
    pub provider: Option<String>,
    pub display_name: Option<String>,
    pub username: String,
    pub imap: Endpoint,
    pub smtp: Endpoint,
    /// `OAuth` means the provider only works through `addOAuth`.
    pub auth: Auth,
    /// The provider requires an app-specific password (or 授权码) instead of the login password.
    pub app_password: bool,
    pub help_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImapInput {
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    /// Defaults to `email`.
    #[serde(default)]
    pub username: Option<String>,
    pub password: String,
    pub imap: Endpoint,
    pub smtp: Endpoint,
    #[serde(default)]
    pub provider: Option<String>,
}

impl fmt::Debug for ImapInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapInput")
            .field("email", &self.email)
            .field("username", &self.username)
            .field("imap", &self.imap)
            .field("smtp", &self.smtp)
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: AccountId,
    pub email: String,
    pub name: Option<String>,
    pub provider: Option<String>,
    pub username: String,
    pub auth: Auth,
    pub imap: Endpoint,
    pub smtp: Endpoint,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Address {
    #[serde(default)]
    pub name: Option<String>,
    pub email: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: MessageId,
    pub account_id: AccountId,
    pub folder: FolderKind,
    pub from: Option<Address>,
    pub to: Vec<Address>,
    pub subject: String,
    pub snippet: String,
    /// Unix time in milliseconds.
    pub date: i64,
    pub seen: bool,
    pub flagged: bool,
    pub has_attachments: bool,
    /// The conversation it belongs to; spans accounts when they share messages.
    #[serde(default)]
    pub thread_id: ThreadId,
}

/// A conversation as a list shows it: the messages of one thread in the listed view, copies of one
/// message (same `Message-ID`) counted once.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: ThreadId,
    /// The first message's subject.
    pub subject: String,
    pub latest: Summary,
    pub count: u32,
    pub unread: u32,
    /// Senders, oldest first, each once.
    pub participants: Vec<Address>,
    pub has_attachments: bool,
    pub flagged: bool,
}

impl Thread {
    /// The row of a conversation from its messages, oldest first (see [`conversation`]).
    pub fn of(id: ThreadId, messages: &[Summary]) -> Option<Thread> {
        let latest = messages.last()?.clone();
        let mut participants: Vec<Address> = Vec::new();
        for from in messages.iter().filter_map(|m| m.from.as_ref()) {
            if !participants
                .iter()
                .any(|p| p.email.eq_ignore_ascii_case(&from.email))
            {
                participants.push(from.clone());
            }
        }
        Some(Thread {
            id,
            subject: messages[0].subject.clone(),
            count: messages.len() as u32,
            unread: messages.iter().filter(|m| !m.seen).count() as u32,
            participants,
            has_attachments: messages.iter().any(|m| m.has_attachments),
            flagged: messages.iter().any(|m| m.flagged),
            latest,
        })
    }
}

/// Whether a message in `folder` belongs to a conversation listed in `view`: Trash and Drafts
/// list only their own messages; every other folder lists the whole conversation (the replies in
/// Sent beside the inbox's), without what is in Trash or Drafts.
pub fn in_view(folder: FolderKind, view: FolderKind) -> bool {
    match view {
        FolderKind::Trash | FolderKind::Drafts => folder == view,
        _ => !matches!(folder, FolderKind::Trash | FolderKind::Drafts),
    }
}

/// A thread's messages (in the order they were stored) oldest first, each message once: copies sharing a `Message-ID` (Gmail's
/// All Mail beside the inbox, mail between one's own accounts) keep the inbox's, else the first
/// folder of [`FolderKind::ALL`].
pub fn conversation(mut messages: Vec<(Summary, Option<String>)>) -> Vec<Summary> {
    let rank = |f: FolderKind| FolderKind::ALL.iter().position(|k| *k == f).unwrap_or(0);
    // Stable: messages sent within the same second keep the order they came in.
    messages.sort_by_key(|(m, _)| m.date);
    let mut out: Vec<(Summary, Option<String>)> = Vec::with_capacity(messages.len());
    for (m, mid) in messages {
        match mid
            .as_ref()
            .and_then(|mid| out.iter_mut().find(|(_, o)| o.as_ref() == Some(mid)))
        {
            Some((kept, _)) if rank(m.folder) < rank(kept.folder) => *kept = m,
            Some(_) => {}
            None => out.push((m, mid)),
        }
    }
    out.into_iter().map(|(m, _)| m).collect()
}

/// A subject without its reply and forward prefixes (`Re:`, `Fwd:`, `AW:`, `回复：`, `[list]`…),
/// trimmed and lowercased, and whether it had a reply or forward prefix.
pub fn base_subject(subject: &str) -> (String, bool) {
    const PREFIXES: [&str; 12] = [
        "re", "fw", "fwd", "aw", "wg", "sv", "vs", "antw", "回复", "回覆", "答复", "转发",
    ];
    let mut s = subject.trim();
    let mut replied = false;
    loop {
        if let Some(rest) = s.strip_prefix('[').and_then(|r| r.split_once(']')) {
            s = rest.1.trim_start();
            continue;
        }
        let word: String = s.chars().take_while(|c| c.is_alphabetic()).collect();
        let after = &s[word.len()..];
        // `Re[2]:`, `Re(2):`
        let after = after
            .strip_prefix(|c| c == '[' || c == '(')
            .and_then(|r| r.split_once([']', ')']))
            .filter(|(n, _)| n.chars().all(|c| c.is_ascii_digit()))
            .map_or(after, |(_, r)| r);
        let lower = word.to_lowercase();
        match after.trim_start().strip_prefix([':', '：']) {
            Some(rest) if PREFIXES.contains(&lower.as_str()) => {
                s = rest.trim_start();
                replied = true;
            }
            _ => break,
        }
    }
    (s.to_lowercase(), replied)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    /// Position among the message's attachments; stable for a given message.
    pub index: u32,
    pub filename: Option<String>,
    pub mime_type: String,
    pub size: u64,
    pub inline: bool,
    pub content_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    #[serde(flatten)]
    pub summary: Summary,
    pub cc: Vec<Address>,
    pub reply_to: Vec<Address>,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub text: Option<String>,
    /// Sanitized HTML, `None` when the message has no HTML part. Inline (`cid:`) images are
    /// `data:` URIs; remote URLs stay, for the reader to block until the user asks.
    pub html: Option<String>,
    /// The remote URLs `html` loads (images, CSS).
    pub remote_images: Vec<String>,
    pub attachments: Vec<Attachment>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub account_id: AccountId,
    pub to: Vec<Address>,
    #[serde(default)]
    pub cc: Vec<Address>,
    #[serde(default)]
    pub bcc: Vec<Address>,
    pub subject: String,
    pub text: String,
    #[serde(default)]
    pub html: Option<String>,
    /// Message-ID of the message being replied to.
    #[serde(default)]
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub references: Vec<String>,
    #[serde(default)]
    pub attachments: Vec<DraftAttachment>,
}

/// A file a draft sends.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DraftAttachment {
    /// A file staged before sending: `Mail.stage` on the client, `POST /v1/uploads` on the server.
    Upload { id: String },
    /// An attachment of a stored message, as a forward keeps it.
    Forward { message_id: MessageId, index: u32 },
}

/// A staged file, ready to be named by a draft.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Upload {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
}

/// The default cap on a message's attachments, in bytes before encoding: what Gmail and Outlook accept.
pub const MAX_ATTACHMENTS: u64 = 25 * 1024 * 1024;

/// A flag patch: `None` leaves the flag unchanged.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    #[serde(default)]
    pub seen: Option<bool>,
    #[serde(default)]
    pub flagged: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub folder: FolderKind,
    /// `None` = all accounts (unified view).
    #[serde(default)]
    pub account: Option<AccountId>,
    /// Cursor: id of the last message (or thread, for threads) of the previous page.
    #[serde(default)]
    pub before: Option<MessageId>,
    pub limit: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MailEvent {
    NewMail {
        summary: Summary,
    },
    /// Messages in this folder were added, removed or changed: reload lists showing it.
    Changed {
        account_id: AccountId,
        folder: FolderKind,
    },
    SyncState {
        account_id: AccountId,
        syncing: bool,
        error: Option<Error>,
    },
    Error {
        account_id: Option<AccountId>,
        error: Error,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    None,
    Local,
    Cloud,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub mode: Mode,
    #[serde(default)]
    pub server: Option<String>,
    /// The server account signed in, in cloud mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    pub syncing: bool,
    #[serde(default)]
    pub error: Option<Error>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// Wrong password, revoked token, or the server refused the login.
    #[serde(rename = "ERR_AUTH")]
    Auth,
    #[serde(rename = "ERR_NETWORK")]
    Network,
    /// TLS handshake or certificate failure.
    #[serde(rename = "ERR_TLS")]
    Tls,
    /// The server answered something we cannot use.
    #[serde(rename = "ERR_PROTOCOL")]
    Protocol,
    #[serde(rename = "ERR_NOT_FOUND")]
    NotFound,
    #[serde(rename = "ERR_INVALID_INPUT")]
    InvalidInput,
    /// The attachments are larger than the limit.
    #[serde(rename = "ERR_TOO_LARGE")]
    TooLarge,
    #[serde(rename = "ERR_UNSUPPORTED")]
    Unsupported,
    /// OAuth client id missing from the build, or the OAuth flow failed.
    #[serde(rename = "ERR_OAUTH")]
    OAuth,
    #[serde(rename = "ERR_SECRETS")]
    Secrets,
    #[serde(rename = "ERR_STORAGE")]
    Storage,
    #[serde(rename = "ERR_INTERNAL")]
    Internal,
}

/// The one error type of the crate; serializable so clients get `{ code, message }`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T, E = Error> = std::result::Result<T, E>;
