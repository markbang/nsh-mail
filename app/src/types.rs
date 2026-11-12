//! The values `nsh-mail/native` exchanges with TSX (docs/design.md §3). Each
//! mirrors `mail_core::types` field for field, with the same JSON shape (a
//! serde tag becomes a `type` / `kind` field, a flattened struct its
//! fields), so the real backends convert one into the other mechanically.

use nsh_core::prelude::*;

/// Where mail comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum Mode {
    /// Not chosen yet: onboarding.
    #[default]
    None,
    /// On this device, straight to the mail servers.
    Local,
    /// Through an nsh-mail server.
    Cloud,
}

/// A failure: a stable `code` (`ERR_AUTH`, `ERR_NETWORK`, ...) and a message for people.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct ErrorInfo {
    /// `ERR_AUTH`, `ERR_NETWORK`, `ERR_TLS`, `ERR_PROTOCOL`, `ERR_NOT_FOUND`, `ERR_INVALID_INPUT`, `ERR_UNSUPPORTED`, ...
    pub code: String,
    /// What went wrong.
    pub message: String,
}

/// The state of the mail engine.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct Status {
    /// Where mail comes from.
    pub mode: Mode,
    /// The server's URL in cloud mode.
    pub server: Option<String>,
    /// A sync is running.
    pub syncing: bool,
    /// The last sync error, until a sync succeeds.
    pub error: Option<ErrorInfo>,
}

/// A mail folder, by role.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PropEnum)]
pub enum Folder {
    /// The inbox.
    #[default]
    Inbox,
    /// Sent mail.
    Sent,
    /// Drafts.
    Drafts,
    /// Archive ("All Mail" on Gmail).
    Archive,
    /// Trash.
    Trash,
}

/// How a connection is encrypted. Plain text is never allowed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum Security {
    /// TLS from the start (993, 465).
    #[default]
    Tls,
    /// STARTTLS on a plain port (143, 587).
    #[prop(rename = "starttls")]
    StartTls,
}

/// A server to connect to.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct Endpoint {
    /// Host name.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Encryption.
    pub security: Security,
}

/// The providers that sign in with OAuth.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum OAuthProvider {
    /// Google.
    #[default]
    Google,
    /// Microsoft.
    Microsoft,
}

/// How an account signs in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum AuthKind {
    /// An app password.
    #[default]
    Password,
    /// OAuth (`provider`).
    #[prop(rename = "oauth")]
    OAuth,
}

/// How an account signs in: `{ kind: "password" }` or `{ kind: "oauth", provider }`.
#[derive(Debug, Clone, Copy, Default, PartialEq, JsObject)]
pub struct Auth {
    /// Password or OAuth.
    pub kind: AuthKind,
    /// The OAuth provider.
    pub provider: Option<OAuthProvider>,
}

/// What `detect(email)` found: what the add-account form starts with.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct ServerSettings {
    /// The preset's id (`gmail`, `outlook`, `qq`, ...); absent when it came from autoconfig.
    pub provider: Option<String>,
    /// The provider's name ("QQ Mail").
    pub display_name: Option<String>,
    /// The login name.
    pub username: String,
    /// IMAP.
    pub imap: Endpoint,
    /// SMTP.
    pub smtp: Endpoint,
    /// `oauth`: the provider works only through `addOAuth`.
    pub auth: Auth,
    /// The provider wants an app password (授权码) rather than the login password.
    pub app_password: bool,
    /// Where the provider explains app passwords.
    pub help_url: Option<String>,
}

/// What adding an IMAP account takes.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct ImapInput {
    /// The address.
    pub email: String,
    /// The name mail is sent with.
    pub name: Option<String>,
    /// The login name (the address when left out).
    pub username: Option<String>,
    /// The app password.
    pub password: String,
    /// IMAP.
    pub imap: Endpoint,
    /// SMTP.
    pub smtp: Endpoint,
    /// The preset's id, as `detect` gave it.
    pub provider: Option<String>,
}

/// A mail account.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct Account {
    /// Stable id.
    pub id: String,
    /// The address.
    pub email: String,
    /// The name mail is sent with.
    pub name: Option<String>,
    /// The preset's id (`gmail`, `outlook`, `qq`, ...), when known.
    pub provider: Option<String>,
    /// The login name.
    pub username: String,
    /// How it signs in.
    pub auth: Auth,
    /// IMAP.
    pub imap: Endpoint,
    /// SMTP.
    pub smtp: Endpoint,
}

/// A name and an address.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct Address {
    /// The display name, when there is one.
    pub name: Option<String>,
    /// The address.
    pub email: String,
}

/// A message in a list.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct Summary {
    /// Stable id.
    pub id: String,
    /// The account it is in.
    pub account_id: String,
    /// The folder it is in.
    pub folder: Folder,
    /// The sender.
    pub from: Option<Address>,
    /// The recipients.
    pub to: Vec<Address>,
    /// The subject.
    pub subject: String,
    /// The start of the plain text.
    pub snippet: String,
    /// When it was sent (ms since the epoch).
    pub date: f64,
    /// Read.
    pub seen: bool,
    /// Flagged.
    pub flagged: bool,
    /// It has attachments.
    pub has_attachments: bool,
}

/// An attachment's metadata.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct Attachment {
    /// Its position among the message's attachments.
    pub index: u32,
    /// The file name.
    pub filename: Option<String>,
    /// Its MIME type.
    pub mime_type: String,
    /// Its size in bytes.
    pub size: f64,
    /// Shown inside the body rather than attached.
    pub inline: bool,
    /// The `Content-ID` an inline part is referred to by.
    pub content_id: Option<String>,
}

/// A whole message: a summary's fields and the rest.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct Message {
    /// Stable id.
    pub id: String,
    /// The account it is in.
    pub account_id: String,
    /// The folder it is in.
    pub folder: Folder,
    /// The sender.
    pub from: Option<Address>,
    /// The recipients.
    pub to: Vec<Address>,
    /// The subject.
    pub subject: String,
    /// The start of the plain text.
    pub snippet: String,
    /// When it was sent (ms since the epoch).
    pub date: f64,
    /// Read.
    pub seen: bool,
    /// Flagged.
    pub flagged: bool,
    /// It has attachments.
    pub has_attachments: bool,
    /// Copied recipients.
    pub cc: Vec<Address>,
    /// Where replies go, when not to the sender.
    pub reply_to: Vec<Address>,
    /// The `Message-ID` header.
    pub message_id: Option<String>,
    /// The `In-Reply-To` header.
    pub in_reply_to: Option<String>,
    /// The `References` header's ids.
    pub references: Vec<String>,
    /// The plain-text body.
    pub text: Option<String>,
    /// The HTML body, sanitized; remote images have no `src`, their URLs are `remoteImages`, in order.
    pub html: Option<String>,
    /// The URLs of the remote images taken out of `html`.
    pub remote_images: Vec<String>,
    /// Attachments.
    pub attachments: Vec<Attachment>,
}

/// Flags to change; a field left out stays as it is.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct Flags {
    /// Read.
    pub seen: Option<bool>,
    /// Flagged.
    pub flagged: Option<bool>,
}

/// A message to send.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct Draft {
    /// The account it is sent from.
    pub account_id: String,
    /// Recipients.
    pub to: Vec<Address>,
    /// Copied recipients.
    #[prop(default)]
    pub cc: Vec<Address>,
    /// Hidden recipients.
    #[prop(default)]
    pub bcc: Vec<Address>,
    /// The subject.
    pub subject: String,
    /// The plain-text body.
    pub text: String,
    /// An HTML body beside the text.
    pub html: Option<String>,
    /// The `Message-ID` replied to.
    pub in_reply_to: Option<String>,
    /// The thread's `References`.
    #[prop(default)]
    pub references: Vec<String>,
}

/// A page of a folder.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct ListQuery {
    /// The folder.
    pub folder: Folder,
    /// One account's id; every account's when left out (the unified view).
    pub account: Option<String>,
    /// The id of the previous page's last message: the next page.
    pub before: Option<String>,
    /// At most this many.
    pub limit: u32,
}

/// What a [`MailEvent`] reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum MailEventType {
    /// A message arrived (`summary`).
    NewMail,
    /// Messages in `folder` of `accountId` changed: lists showing it reload.
    #[default]
    Changed,
    /// A sync of `accountId` started or ended (`syncing`, `error`).
    SyncState,
    /// Something failed (`error`).
    Error,
}

/// Something the mail engine reports to `Mail.on`'s listeners.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct MailEvent {
    /// What happened.
    pub r#type: MailEventType,
    /// The message that arrived (`newMail`).
    pub summary: Option<Summary>,
    /// The account concerned.
    pub account_id: Option<String>,
    /// The folder that changed (`changed`).
    pub folder: Option<Folder>,
    /// Whether a sync runs (`syncState`).
    pub syncing: Option<bool>,
    /// What failed (`syncState`, `error`).
    pub error: Option<ErrorInfo>,
}

impl MailEvent {
    pub fn new_mail(summary: Summary) -> Self {
        Self {
            r#type: MailEventType::NewMail,
            account_id: Some(summary.account_id.clone()),
            summary: Some(summary),
            ..Self::default()
        }
    }

    pub fn changed(account_id: Option<String>, folder: Option<Folder>) -> Self {
        Self {
            account_id,
            folder,
            ..Self::default()
        }
    }

    pub fn sync_state(account_id: Option<String>, syncing: bool, error: Option<ErrorInfo>) -> Self {
        Self {
            r#type: MailEventType::SyncState,
            account_id,
            syncing: Some(syncing),
            error,
            ..Self::default()
        }
    }
}
