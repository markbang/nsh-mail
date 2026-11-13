use std::{collections::HashMap, path::PathBuf, sync::Arc};

use mail_core::{
    Engine, EngineOptions, OAuthApps, Result, secrets::EncryptedFileStore, types::MailEvent,
};
use tokio::sync::{
    Mutex,
    broadcast::{self, error::RecvError},
};

use crate::push::Push;

/// One mail-core `Engine` per user: `users/<id>.sqlite` for mail, `users/<id>.secrets.json` for
/// passwords and OAuth tokens (AES-256-GCM under `NSHMAIL_MASTER_KEY`). Opened lazily, then kept
/// for the process lifetime so its sync and IDLE loops keep running.
pub struct Mailboxes {
    users_dir: PathBuf,
    master_key: [u8; 32],
    oauth: OAuthApps,
    max_attachments: u64,
    push: Arc<Push>,
    open: Mutex<HashMap<i64, Engine>>,
}

impl Mailboxes {
    pub fn new(
        users_dir: PathBuf,
        master_key: [u8; 32],
        oauth: OAuthApps,
        max_attachments: u64,
        push: Arc<Push>,
    ) -> Self {
        Mailboxes {
            users_dir,
            master_key,
            oauth,
            max_attachments,
            push,
            open: Mutex::default(),
        }
    }

    pub async fn get(&self, user_id: i64) -> Result<Engine> {
        let mut open = self.open.lock().await;
        if let Some(engine) = open.get(&user_id) {
            return Ok(engine.clone());
        }
        let secrets = EncryptedFileStore::with_key(
            self.users_dir.join(format!("{user_id}.secrets.json")),
            self.master_key,
        );
        let engine = Engine::open_with(EngineOptions {
            db_path: self.users_dir.join(format!("{user_id}.sqlite")),
            secrets: Arc::new(secrets),
            oauth: self.oauth.clone(),
            max_attachments: self.max_attachments,
        })?;
        // Subscribe before `run` so no NewMail from the first loops is missed.
        tokio::spawn(push_new_mail(
            self.push.clone(),
            user_id,
            engine.subscribe(),
        ));
        engine.run().await?;
        open.insert(user_id, engine.clone());
        Ok(engine)
    }

    /// Users that have a mail store; their sync loops start at boot.
    pub fn existing_users(&self) -> std::io::Result<Vec<i64>> {
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(&self.users_dir)? {
            let name = entry?.file_name();
            if let Some(id) = name
                .to_str()
                .and_then(|n| n.strip_suffix(".sqlite"))
                .and_then(|n| n.parse().ok())
            {
                ids.push(id);
            }
        }
        Ok(ids)
    }
}

async fn push_new_mail(push: Arc<Push>, user_id: i64, mut rx: broadcast::Receiver<MailEvent>) {
    loop {
        match rx.recv().await {
            Ok(MailEvent::NewMail { summary }) => {
                let push = push.clone();
                tokio::spawn(async move { push.new_mail(user_id, &summary).await });
            }
            Ok(_) | Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}
