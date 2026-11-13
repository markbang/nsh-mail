//! What `Mail` asks of a mail source. `LocalBackend` (mail-core, native
//! only) and `CloudBackend` (the server's REST API) implement it.

use std::{collections::HashMap, future::Future, pin::Pin, rc::Rc};

use mail_core::types::*;
use nsh_core::prelude::JsError;

/// A backend call: it runs on the engine thread, so it need not be `Send`.
pub type MailFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + 'a>>;

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
    /// Unread messages in each account's inbox.
    fn unread(&self) -> MailFuture<'_, HashMap<AccountId, u32>>;
    fn refresh(&self) -> MailFuture<'_, ()>;
}

pub fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}

/// The rejection TSX gets: an `Error` whose `code` is the error's (`ERR_AUTH`, ...).
pub fn js(e: Error) -> JsError {
    let code = serde_json::to_value(e.code)
        .ok()
        .and_then(|c| c.as_str().map(str::to_owned))
        .unwrap_or_else(|| "ERR_INTERNAL".into());
    JsError::Error(e.message).with_code(&code)
}
