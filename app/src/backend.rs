//! What `Mail` asks of a mail source. `LocalBackend` (mail-core, native
//! only) and `CloudBackend` (the server's REST API) implement it.

use std::{cell::RefCell, collections::HashMap, future::Future, pin::Pin, rc::Rc};

use mail_core::types::*;
use nsh_core::prelude::JsError;

/// A backend call: it runs on the engine thread, so it need not be `Send`.
pub type MailFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + 'a>>;

/// Where a backend reports what changes on its own (new mail, sync state).
pub type Events = Rc<dyn Fn(MailEvent)>;

/// Hears how many bytes of a staged file reached where it is kept.
pub type Progress = Rc<dyn Fn(u64)>;

/// A file the user attached, as it came from the picker, a paste or a drop.
pub struct File {
    pub name: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

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
    /// The decoded bytes of attachment `index` of message `id`.
    fn attachment(&self, id: String, index: u32) -> MailFuture<'_, Vec<u8>>;
    fn set_flags(&self, id: String, flags: Flags) -> MailFuture<'_, ()>;
    /// Sends a draft; its `upload` attachments are files `stage` kept.
    fn send(&self, draft: Draft) -> MailFuture<'_, ()>;
    /// Keeps a file for a draft until it is sent or `unstage`d.
    fn stage(&self, file: File, progress: Progress) -> MailFuture<'_, Upload>;
    fn unstage(&self, id: String) -> MailFuture<'_, ()>;
    /// The most a message's attachments may add up to, in bytes.
    fn attachment_limit(&self) -> u64 {
        MAX_ATTACHMENTS
    }
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

/// Files staged in this app (local mode and the demo), by id.
#[derive(Default)]
pub struct Staged {
    files: RefCell<HashMap<String, Rc<File>>>,
    next: RefCell<u64>,
}

impl Staged {
    pub fn add(&self, file: File) -> Upload {
        let id = {
            let mut next = self.next.borrow_mut();
            *next += 1;
            format!("f{next}")
        };
        let upload = Upload {
            id: id.clone(),
            filename: file.name.clone(),
            mime_type: file.mime_type.clone(),
            size: file.data.len() as u64,
        };
        self.files.borrow_mut().insert(id, Rc::new(file));
        upload
    }

    pub fn remove(&self, id: &str) {
        self.files.borrow_mut().remove(id);
    }

    /// The files a draft's `upload`s name; they stay staged until sent.
    pub fn of(&self, draft: &Draft) -> Result<Vec<(String, Rc<File>)>> {
        let files = self.files.borrow();
        draft
            .attachments
            .iter()
            .filter_map(|a| match a {
                DraftAttachment::Upload { id } => Some(id),
                DraftAttachment::Forward { .. } => None,
            })
            .map(|id| {
                files
                    .get(id)
                    .map(|f| (id.clone(), f.clone()))
                    .ok_or_else(|| {
                        Error::new(
                            ErrorCode::NotFound,
                            "an attached file is gone; attach it again",
                        )
                    })
            })
            .collect()
    }

    /// Forgets a sent draft's files.
    pub fn sent(&self, draft: &Draft) {
        for a in &draft.attachments {
            if let DraftAttachment::Upload { id } = a {
                self.remove(id);
            }
        }
    }
}
