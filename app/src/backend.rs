//! What `Mail` asks of a mail source. `LocalBackend` (mail-core, native
//! only) and `CloudBackend` (the server's REST API) implement it; until they
//! land, `DemoBackend` stands in for both.

use std::{fmt, future::Future, pin::Pin, rc::Rc};

use nsh_core::prelude::JsError;

use crate::types::*;

/// A backend call: it runs on the engine thread, so it need not be `Send`.
pub type MailFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, MailError>> + 'a>>;

/// Where a backend reports what changes on its own (new mail, sync state).
pub type Events = Rc<dyn Fn(MailEvent)>;

pub trait Backend {
    /// Opens the store, restores the session and starts syncing.
    fn start(&self, events: Events) -> MailFuture<'_, ()>;
    /// Ends the session and forgets what it kept on this device.
    fn sign_out(&self) -> MailFuture<'_, ()>;
    fn accounts(&self) -> MailFuture<'_, Vec<Account>>;
    fn detect(&self, email: String) -> MailFuture<'_, Option<ServerSettings>>;
    /// Connects to IMAP and SMTP before saving the account.
    fn add_imap(&self, input: ImapInput) -> MailFuture<'_, Account>;
    fn add_oauth(&self, provider: OAuthProvider) -> MailFuture<'_, Account>;
    fn remove_account(&self, id: String) -> MailFuture<'_, ()>;
    fn list(&self, query: ListQuery) -> MailFuture<'_, Vec<Summary>>;
    fn get(&self, id: String) -> MailFuture<'_, Message>;
    fn set_flags(&self, id: String, flags: Flags) -> MailFuture<'_, ()>;
    fn send(&self, draft: Draft) -> MailFuture<'_, ()>;
    fn search(&self, text: String, limit: u32) -> MailFuture<'_, Vec<Summary>>;
    fn refresh(&self) -> MailFuture<'_, ()>;
}

/// A failure TSX can branch on: `code` is stable, `message` is for people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailError {
    pub code: &'static str,
    pub message: String,
}

impl MailError {
    pub const UNSUPPORTED: &'static str = "ERR_UNSUPPORTED";
    pub const NO_SESSION: &'static str = "ERR_NO_SESSION";
    pub const INVALID: &'static str = "ERR_INVALID_INPUT";
    pub const AUTH: &'static str = "ERR_AUTH";
    pub const NETWORK: &'static str = "ERR_NETWORK";
    pub const NOT_FOUND: &'static str = "ERR_NOT_FOUND";

    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(Self::INVALID, message)
    }

    pub fn not_found(what: &str) -> Self {
        Self::new(Self::NOT_FOUND, format!("{what} not found"))
    }
}

impl fmt::Display for MailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl From<MailError> for ErrorInfo {
    fn from(e: MailError) -> Self {
        ErrorInfo {
            code: e.code.to_owned(),
            message: e.message,
        }
    }
}

impl From<MailError> for JsError {
    fn from(e: MailError) -> Self {
        JsError::Error(e.message).with_code(e.code)
    }
}
