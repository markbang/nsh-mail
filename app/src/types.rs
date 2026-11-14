//! The values `nsh-mail/native` exchanges with TSX are `mail_core::types`
//! (docs/design.md §3). They cross as their serde JSON in [`Js`], and
//! [`DECLARATIONS`] is that JSON's TypeScript.

use mail_core::types::*;
use nsh_core::{
    JsonValue,
    element::{JsHost, JsIn, JsOwned, JsValue, PropError, TsTypes},
};
use serde::{Serialize, de::DeserializeOwned};

/// A `mail_core` value crossing to or from TSX as its JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct Js<T>(pub T);

/// The TypeScript type of a value's JSON.
pub trait Ts {
    fn ts() -> String;
}

macro_rules! ts {
    ($($ty:ty => $name:literal),* $(,)?) => {$(
        impl Ts for $ty {
            fn ts() -> String {
                $name.into()
            }
        }
    )*};
}

ts! {
    Account => "Account",
    Address => "Address",
    Attachment => "Attachment",
    Backfill => "Backfill",
    Draft => "Draft",
    Flags => "Flags",
    FolderKind => "Folder",
    ImapInput => "ImapInput",
    ListQuery => "ListQuery",
    MailEvent => "MailEvent",
    Message => "Message",
    Outgoing => "Outgoing",
    SendAt => "SendAt",
    Signature => "Signature",
    OAuthProvider => "OAuthProvider",
    ServerSettings => "ServerSettings",
    Status => "Status",
    Summary => "Summary",
    SyncProgress => "SyncProgress",
    Thread => "Thread",
    Upload => "Upload",
}

impl<T: Ts> Ts for Vec<T> {
    fn ts() -> String {
        format!("{}[]", T::ts())
    }
}

impl Ts for std::collections::HashMap<String, u32> {
    fn ts() -> String {
        "Record<string, number>".into()
    }
}

impl<T: Ts> Ts for Option<T> {
    fn ts() -> String {
        format!("{} | null", T::ts())
    }
}

impl<T: Ts + Serialize + DeserializeOwned + 'static> JsValue for Js<T> {
    fn ts_type(types: &mut TsTypes) -> String {
        for (name, declaration) in DECLARATIONS.iter().chain(crate::notify::DECLARATIONS) {
            types.declare(name, |_| (*declaration).to_owned());
        }
        T::ts()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        let json = JsonValue::from_js(value)?;
        serde_json::from_value(json.clone())
            .map(Js)
            .map_err(|e| PropError(format!("expected {}: {e}, got {json}", T::ts())))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        serde_json::to_value(&self.0)
            .map_err(|e| PropError(e.to_string()))?
            .to_js(host)
    }
}

/// The JSON of `mail_core::types` in TypeScript; field names, tags and
/// optional fields follow their serde attributes.
const DECLARATIONS: &[(&str, &str)] = &[
    (
        "Mode",
        "/** Where mail comes from: not chosen yet, this device, or an nsh-mail server. */\n\
         export type Mode = \"none\" | \"local\" | \"cloud\";",
    ),
    (
        "ErrorInfo",
        "/** A failure: a stable `code` (`ERR_AUTH`, `ERR_NETWORK`, ...) and a message for people. */\n\
         export interface ErrorInfo {\n  code: string;\n  message: string;\n}",
    ),
    (
        "Status",
        "/** The state of the mail engine. */\n\
         export interface Status {\n  mode: Mode;\n  /** The server's URL in cloud mode. */\n  server?: string | null;\n  \
         /** The server account signed in, in cloud mode. */\n  user?: string | null;\n  \
         /** A sync is running. */\n  syncing: boolean;\n  /** How far the running sync has come. */\n  progress?: SyncProgress | null;\n  \
         /** The last sync error, until a sync succeeds. */\n  error?: ErrorInfo | null;\n}",
    ),
    (
        "SyncProgress",
        "/** How far a sync has come: folders finished and messages stored so far. */\n\
         export interface SyncProgress {\n  foldersDone: number;\n  foldersTotal: number;\n  messages: number;\n}",
    ),
    (
        "Backfill",
        "/** What loading older mail did. */\n\
         export interface Backfill {\n  /** Messages added to the store. */\n  inserted: number;\n  \
         /** Older mail is still on the server. */\n  pending: boolean;\n}",
    ),
    (
        "Folder",
        "/** A mail folder, by role. */\n\
         export type Folder = \"inbox\" | \"sent\" | \"drafts\" | \"archive\" | \"trash\";",
    ),
    (
        "Security",
        "/** How a connection is encrypted: TLS from the start (993, 465) or STARTTLS (143, 587). Never plain text. */\n\
         export type Security = \"tls\" | \"starttls\";",
    ),
    (
        "Endpoint",
        "/** A server to connect to. */\n\
         export interface Endpoint {\n  host: string;\n  port: number;\n  security: Security;\n}",
    ),
    (
        "OAuthProvider",
        "/** The providers that sign in with OAuth. */\n\
         export type OAuthProvider = \"google\" | \"microsoft\";",
    ),
    (
        "Auth",
        "/** How an account signs in. */\n\
         export type Auth = { kind: \"password\" } | { kind: \"oauth\"; provider: OAuthProvider };",
    ),
    (
        "ServerSettings",
        "/** What `detect(email)` found: what the add-account form starts with. */\n\
         export interface ServerSettings {\n  /** The preset's id (`gmail`, `qq`, ...); null when it came from autoconfig. */\n  \
         provider?: string | null;\n  /** The provider's name (\"QQ Mail\"). */\n  displayName?: string | null;\n  username: string;\n  \
         imap: Endpoint;\n  smtp: Endpoint;\n  /** `oauth`: the provider works only through `addOAuth`. */\n  auth: Auth;\n  \
         /** The provider wants an app password (授权码) rather than the login password. */\n  appPassword: boolean;\n  \
         /** Where the provider explains app passwords. */\n  helpUrl?: string | null;\n}",
    ),
    (
        "ImapInput",
        "/** What adding an IMAP account takes. */\n\
         export interface ImapInput {\n  email: string;\n  /** The name mail is sent with. */\n  name?: string | null;\n  \
         /** The login name (the address when left out). */\n  username?: string | null;\n  /** The app password. */\n  password: string;\n  \
         imap: Endpoint;\n  smtp: Endpoint;\n  /** The preset's id, as `detect` gave it. */\n  provider?: string | null;\n}",
    ),
    (
        "Account",
        "/** A mail account. */\n\
         export interface Account {\n  id: string;\n  email: string;\n  name?: string | null;\n  \
         /** The preset's id (`gmail`, `outlook`, `qq`, ...), when known. */\n  provider?: string | null;\n  username: string;\n  \
         auth: Auth;\n  imap: Endpoint;\n  smtp: Endpoint;\n  signature: Signature;\n}",
    ),
    (
        "Signature",
        "/** What the composer adds under what the user writes. */\n\
         export interface Signature {\n  /** Plain text with `**bold**`, `*italic*` and `[links](https://…)`; empty: none. */\n  text: string;\n  \
         newMessages: boolean;\n  /** Replies and forwards. */\n  replies: boolean;\n}",
    ),
    (
        "Address",
        "/** A name and an address. */\n\
         export interface Address {\n  name?: string | null;\n  email: string;\n}",
    ),
    (
        "Summary",
        "/** A message in a list. */\n\
         export interface Summary {\n  id: string;\n  accountId: string;\n  folder: Folder;\n  from?: Address | null;\n  to: Address[];\n  \
         subject: string;\n  /** The start of the plain text. */\n  snippet: string;\n  /** When it was sent (ms since the epoch). */\n  date: number;\n  \
         seen: boolean;\n  flagged: boolean;\n  hasAttachments: boolean;\n  \
         /** Its conversation (`Mail.thread`); one conversation can span accounts. */\n  threadId: string;\n}",
    ),
    (
        "Thread",
        "/** A conversation in a list: its messages in the listed folder's view, each message once. */\n\
         export interface Thread {\n  id: string;\n  /** The first message's subject. */\n  subject: string;\n  \
         /** The newest message. */\n  latest: Summary;\n  count: number;\n  unread: number;\n  \
         /** Who wrote, oldest first, each once. */\n  participants: Address[];\n  hasAttachments: boolean;\n  flagged: boolean;\n}",
    ),
    (
        "Attachment",
        "/** An attachment's metadata. */\n\
         export interface Attachment {\n  /** Its position among the message's attachments. */\n  index: number;\n  filename?: string | null;\n  \
         mimeType: string;\n  /** Bytes. */\n  size: number;\n  /** Shown inside the body rather than attached. */\n  inline: boolean;\n  \
         contentId?: string | null;\n}",
    ),
    (
        "Message",
        "/** A whole message: a summary's fields and the rest. */\n\
         export interface Message extends Summary {\n  cc: Address[];\n  replyTo: Address[];\n  messageId?: string | null;\n  \
         inReplyTo?: string | null;\n  references: string[];\n  text?: string | null;\n  \
         /** Sanitized, `null` without an HTML part. Inline images are `data:` URIs; remote URLs stay, blocked by the reader until asked. */\n  html?: string | null;\n  \
         /** The remote URLs `html` loads (images, CSS). */\n  remoteImages: string[];\n  attachments: Attachment[];\n}",
    ),
    (
        "Flags",
        "/** Flags to change; a field left out stays as it is. */\n\
         export interface Flags {\n  seen?: boolean | null;\n  flagged?: boolean | null;\n}",
    ),
    (
        "Draft",
        "/** A message to send. */\n\
         export interface Draft {\n  accountId: string;\n  to: Address[];\n  cc?: Address[];\n  bcc?: Address[];\n  subject: string;\n  \
         text: string;\n  html?: string | null;\n  /** The `Message-ID` replied to. */\n  inReplyTo?: string | null;\n  \
         /** The thread's `References`. */\n  references?: string[];\n  attachments?: DraftAttachment[];\n}",
    ),
    (
        "DraftAttachment",
        "/** A file a draft sends: one `Mail.stage` kept, or an attachment of a stored message (a forward keeps them). */\n\
         export type DraftAttachment =\n  | { kind: \"upload\"; id: string }\n  | { kind: \"forward\"; messageId: string; index: number };",
    ),
    (
        "Upload",
        "/** A staged file. */\n\
         export interface Upload {\n  id: string;\n  filename: string;\n  mimeType: string;\n  /** Bytes. */\n  size: number;\n}",
    ),
    (
        "ListQuery",
        "/** A page of a folder. */\n\
         export interface ListQuery {\n  folder: Folder;\n  /** One account's id; every account's when left out (the unified view). */\n  \
         account?: string | null;\n  /** The id of the previous page's last message (or conversation): the next page. */\n  before?: string | null;\n  limit: number;\n}",
    ),
    (
        "MailEvent",
        "/** Something the mail engine reports to `Mail.on`'s listeners. */\n\
         export type MailEvent =\n  | { type: \"newMail\"; summary: Summary }\n  \
         | { type: \"changed\"; accountId: string; folder: Folder }\n  \
         | { type: \"syncState\"; accountId: string; syncing: boolean; error: ErrorInfo | null }\n  \
         | { type: \"syncProgress\"; accountId: string; progress: SyncProgress }\n  \
         | { type: \"error\"; accountId: string | null; error: ErrorInfo }\n  \
         | { type: \"outbox\"; accountId: string; id: string; subject: string; state: OutboxState; late: boolean; error: ErrorInfo | null };",
    ),
    (
        "SendAt",
        "/** When a queued message goes: after `delay` seconds (the undo window) or at `sendAt` (ms since the epoch). */\n\
         export type SendAt = { delay: number } | { sendAt: number };",
    ),
    (
        "OutboxState",
        "/** What happened to a message in the outbox. */\n\
         export type OutboxState = \"queued\" | \"cancelled\" | \"sent\" | \"failed\";",
    ),
    (
        "Outgoing",
        "/** A message in the outbox. */\n\
         export interface Outgoing {\n  id: string;\n  /** Its attachments are all uploads, described by `files`. */\n  draft: Draft;\n  \
         /** When it goes (ms since the epoch). */\n  sendAt: number;\n  /** The user picked the time; otherwise it waits out the undo delay. */\n  scheduled: boolean;\n  \
         files: Upload[];\n  /** Why it was not sent; it then waits for the user. */\n  error?: ErrorInfo | null;\n}",
    ),
];
