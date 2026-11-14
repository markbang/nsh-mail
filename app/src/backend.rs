//! What `Mail` asks of a mail source. `LocalBackend` (mail-core, native
//! only) and `CloudBackend` (the server's REST API) implement it.

use std::{cell::RefCell, collections::HashMap, future::Future, pin::Pin, rc::Rc};

use mail_core::types::*;
use nsh_core::prelude::JsError;

use crate::notify::{
    ApiKey, CreatedApiKey, CreatedSource, FeedPage, FeedQuery, NewApiKey, NewSource, Source,
};

/// The answer of a backend without a feature: `ERR_UNSUPPORTED` with `what`.
pub fn unavailable<T: 'static>(what: &'static str) -> MailFuture<'static, T> {
    Box::pin(async move { Err(crate::notify::unsupported(what)) })
}

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
    fn set_signature(&self, id: String, signature: Signature) -> MailFuture<'_, Account>;
    fn list(&self, query: ListQuery) -> MailFuture<'_, Vec<Summary>>;
    /// Loads a page of older mail the store does not hold yet.
    fn older(&self, folder: FolderKind, account: Option<String>) -> MailFuture<'_, Backfill>;
    /// Conversations, as `list` pages messages; `before` is a thread id.
    fn threads(&self, query: ListQuery) -> MailFuture<'_, Vec<Thread>>;
    /// A conversation's messages, oldest first.
    fn thread(&self, id: String) -> MailFuture<'_, Vec<Summary>>;
    /// Read or unread for the whole conversation; flagging flags its latest message.
    fn set_thread_flags(&self, id: String, flags: Flags) -> MailFuture<'_, ()>;
    /// Archive takes the conversation out of the inbox; Trash takes all of it.
    fn move_thread(&self, id: String, folder: FolderKind) -> MailFuture<'_, ()>;
    fn get(&self, id: String) -> MailFuture<'_, Message>;
    /// The decoded bytes of attachment `index` of message `id`.
    fn attachment(&self, id: String, index: u32) -> MailFuture<'_, Vec<u8>>;
    fn set_flags(&self, id: String, flags: Flags) -> MailFuture<'_, ()>;
    /// Moves a message to another folder of its account: Archive, Trash.
    fn move_to(&self, id: String, folder: FolderKind) -> MailFuture<'_, ()>;
    /// Sends a draft; its `upload` attachments are files `stage` kept.
    fn send(&self, draft: Draft) -> MailFuture<'_, ()>;
    /// Puts a draft in the outbox, to go after `when.delay` seconds (the undo
    /// window) or at `when.send_at`; its files go with it.
    fn queue(&self, draft: Draft, when: SendAt) -> MailFuture<'_, Outgoing>;
    /// The messages waiting to go, or that failed to.
    fn outbox(&self) -> MailFuture<'_, Vec<Outgoing>>;
    /// Takes a message out of the outbox before it goes; its files are staged
    /// again, under the ids its draft names.
    fn cancel(&self, id: String) -> MailFuture<'_, Outgoing>;
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

    // The notification inbox (docs/design/notification-inbox.md §3). A backend
    // without it answers `ERR_UNSUPPORTED`; `DemoBackend` serves it in memory.

    /// Every source, in the order they were added.
    fn sources(&self) -> MailFuture<'_, Vec<Source>> {
        unavailable("this build has no notification sources")
    }
    /// A page of the unified feed, and the counts its filter chips show.
    fn feed(&self, _query: FeedQuery) -> MailFuture<'_, FeedPage> {
        unavailable("this build has no notification feed")
    }
    /// Read or unread for every notification in `ids`.
    fn mark_read(&self, _ids: Vec<String>, _read: bool) -> MailFuture<'_, ()> {
        unavailable("this build has no notification feed")
    }
    /// Takes notifications out of the feed, keeping them for the record.
    fn archive(&self, _ids: Vec<String>) -> MailFuture<'_, ()> {
        unavailable("this build has no notification feed")
    }
    /// Adds a source; its ingest URL is in the answer, shown only here.
    fn create_source(&self, _input: NewSource) -> MailFuture<'_, CreatedSource> {
        unavailable("this build has no notification sources")
    }
    /// Pauses or resumes a source, renames it, sets its colour and the
    /// priority it pushes from.
    fn update_source(&self, _source: Source) -> MailFuture<'_, Source> {
        unavailable("this build has no notification sources")
    }
    fn delete_source(&self, _id: String) -> MailFuture<'_, ()> {
        unavailable("this build has no notification sources")
    }
    /// A new ingest URL for a source; the one before stops working.
    fn rotate_source_token(&self, _id: String) -> MailFuture<'_, CreatedSource> {
        unavailable("this build has no notification sources")
    }
    /// Sends a test notification through the source.
    fn test_source(&self, _id: String) -> MailFuture<'_, ()> {
        unavailable("this build has no notification sources")
    }
    fn api_keys(&self) -> MailFuture<'_, Vec<ApiKey>> {
        unavailable("this build has no API keys")
    }
    /// Creates a key; its token is in the answer, shown only here.
    fn create_api_key(&self, _input: NewApiKey) -> MailFuture<'_, CreatedApiKey> {
        unavailable("this build has no API keys")
    }
    fn revoke_api_key(&self, _id: String) -> MailFuture<'_, ()> {
        unavailable("this build has no API keys")
    }
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

/// Renames an outgoing message's upload `from` to `to` (`Staged::restore` gave it another id).
pub fn rename_upload(o: &mut Outgoing, from: &str, to: &str) {
    for a in &mut o.draft.attachments {
        if let DraftAttachment::Upload { id } = a
            && id == from
        {
            *id = to.to_owned();
        }
    }
    for f in &mut o.files {
        if f.id == from {
            f.id = to.to_owned();
        }
    }
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

    /// Stages a file again under `id`, or a new id when `id` is taken (it
    /// was staged in an earlier run). Returns the id it has.
    pub fn restore(&self, id: String, file: File) -> String {
        let id = if self.files.borrow().contains_key(&id) {
            self.add(file).id
        } else {
            self.files.borrow_mut().insert(id.clone(), Rc::new(file));
            id
        };
        id
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
