use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, anyhow};
use mail_core::{
    Engine, EngineOptions, MailDb, OAuthApps, Result, secrets::EncryptedFileStore, types::MailEvent,
};
use tokio::sync::{
    Mutex,
    broadcast::{self, error::RecvError},
};

use crate::push::Push;

/// One mail-core `Engine` per user over the shared mail tables in `server.sqlite`, with
/// `users/<id>.secrets.json` for passwords and OAuth tokens (AES-256-GCM under
/// `NSHMAIL_MASTER_KEY`). Opened lazily, then kept for the process lifetime so its sync and IDLE
/// loops keep running.
pub struct Mailboxes {
    db: MailDb,
    users_dir: PathBuf,
    master_key: [u8; 32],
    oauth: OAuthApps,
    max_attachments: u64,
    push: Arc<Push>,
    open: Mutex<HashMap<i64, Engine>>,
}

impl Mailboxes {
    pub fn new(
        db: MailDb,
        users_dir: PathBuf,
        master_key: [u8; 32],
        oauth: OAuthApps,
        max_attachments: u64,
        push: Arc<Push>,
    ) -> Self {
        Mailboxes {
            db,
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
            db: self.db.clone(),
            user_id,
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

    /// Users with a mail account; their sync loops start at boot.
    pub fn existing_users(&self) -> Result<Vec<i64>> {
        self.db.users()
    }
}

/// Moves each old per-user `users/<id>.sqlite` into the shared database, then renames it to
/// `<id>.sqlite.migrated` (kept, never deleted). Any file that fails to import or to verify stops
/// the server with the file untouched, since serving that user without their mail would look like
/// data loss. Safe to rerun: a file already imported is verified again and only renamed.
pub fn import_legacy(db: &MailDb, users_dir: &Path) -> anyhow::Result<()> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(users_dir)
        .with_context(|| format!("cannot read {}", users_dir.display()))?
    {
        let path = entry?.path();
        let id = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".sqlite"))
            .and_then(|n| n.parse::<i64>().ok());
        if let Some(id) = id {
            files.push((id, path));
        }
    }
    files.sort();
    for (user_id, path) in files {
        let counts = db.import_legacy(user_id, &path).map_err(|e| {
            anyhow!(
                "cannot move {} into server.sqlite: {}. The file is left as it was and nothing of it \
                 was imported. Restore it from a backup, or move it out of {} to start without \
                 this user's cached mail (it is downloaded again from the mail server).",
                path.display(),
                e.message,
                users_dir.display()
            )
        })?;
        let target = retire(&path)?;
        tracing::info!(
            user_id,
            accounts = counts.accounts,
            folders = counts.folders,
            messages = counts.messages,
            indexed = counts.indexed,
            "imported {} into server.sqlite and renamed it {}",
            path.display(),
            target.display()
        );
    }
    Ok(())
}

/// Renames `<id>.sqlite` (and any `-wal`/`-shm` left beside it) to the first free
/// `<id>.sqlite.migrated[.N]`, never over an existing file.
fn retire(path: &Path) -> anyhow::Result<PathBuf> {
    let base = path.as_os_str().to_owned();
    let target = (1..)
        .map(|n| {
            let mut t = base.clone();
            t.push(if n == 1 {
                ".migrated".to_string()
            } else {
                format!(".migrated.{n}")
            });
            PathBuf::from(t)
        })
        .find(|t| !t.exists())
        .expect("an unused name");
    for suffix in ["-wal", "-shm"] {
        let mut from = base.clone();
        from.push(suffix);
        let from = PathBuf::from(from);
        if from.exists() {
            let mut to = target.as_os_str().to_owned();
            to.push(suffix);
            std::fs::rename(&from, PathBuf::from(to))
                .with_context(|| format!("cannot rename {}", from.display()))?;
        }
    }
    std::fs::rename(path, &target).with_context(|| {
        format!(
            "imported {} but cannot rename it to {}",
            path.display(),
            target.display()
        )
    })?;
    Ok(target)
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
