use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, anyhow, bail};
use mail_core::secrets::EncryptedFileStore;

/// Moves every `users/<id>.secrets.json` entry still under `old` to `current`. All files are
/// read first; if any entry opens with neither key, the server refuses to start and no file is
/// written. Each rewritten file is copied to `.bak-old-key`, replaced in one rename, reopened
/// with `current` alone and compared with the plaintext before the copy is removed. Files
/// already under `current` are left alone, so this is a no-op once rotation is done.
pub fn rotate_secrets(
    users_dir: &Path,
    current: [u8; 32],
    old: Option<[u8; 32]>,
) -> anyhow::Result<()> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(users_dir)
        .with_context(|| format!("cannot read {}", users_dir.display()))?
    {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".secrets.json"))
        {
            files.push(path);
        }
    }
    files.sort();

    let mut plans = Vec::new();
    let mut failures = Vec::new();
    for path in files {
        let store = EncryptedFileStore::with_key(path.clone(), current);
        match store.read_all(old) {
            Ok((entries, used_old)) => plans.push((path, store, entries, used_old)),
            Err(e) => failures.push(format!("{}: {}", path.display(), e.message)),
        }
    }
    if !failures.is_empty() {
        let hint = if old.is_some() {
            "Neither NSHMAIL_MASTER_KEY nor NSHMAIL_MASTER_KEY_OLD opens them: set \
             NSHMAIL_MASTER_KEY_OLD to the key they were written with"
        } else {
            "If NSHMAIL_MASTER_KEY was changed, set NSHMAIL_MASTER_KEY_OLD to the previous key so \
             they are re-encrypted at start; otherwise restore the previous NSHMAIL_MASTER_KEY"
        };
        bail!(
            "stored mail credentials do not decrypt: {}. {hint}. No file was changed.",
            failures.join("; ")
        );
    }

    let (mut entries_total, mut rotated_entries, mut rotated_files) = (0, 0, 0);
    for (path, store, entries, used_old) in &plans {
        entries_total += entries.len();
        let backup = backup_path(path);
        if *used_old == 0 {
            if backup.exists() {
                retire_stale_backup(&backup, entries, current, old);
            }
            continue;
        }
        std::fs::copy(path, &backup)
            .and_then(|_| std::fs::File::open(&backup)?.sync_all())
            .with_context(|| format!("cannot copy {} to {}", path.display(), backup.display()))?;
        store.write_all(entries).map_err(|e| {
            anyhow!(
                "cannot rewrite {}: {}. The original is in {}",
                path.display(),
                e.message,
                backup.display()
            )
        })?;
        sync_dir(users_dir);
        let verified = EncryptedFileStore::with_key(path.clone(), current)
            .read_all(None)
            .ok()
            .is_some_and(|(again, _)| &again == entries);
        if !verified {
            std::fs::rename(&backup, path).with_context(|| {
                format!(
                    "{} failed verification after re-encryption and cannot be restored: copy {} \
                     back over it",
                    path.display(),
                    backup.display()
                )
            })?;
            bail!(
                "{} did not read back correctly after re-encryption; restored the original. \
                 Nothing else was lost; check the disk and restart",
                path.display()
            );
        }
        std::fs::remove_file(&backup)
            .with_context(|| format!("cannot remove {}", backup.display()))?;
        rotated_entries += used_old;
        rotated_files += 1;
    }

    if old.is_some() {
        tracing::info!(
            files = plans.len(),
            secrets = entries_total,
            rotated_files,
            rotated_secrets = rotated_entries,
            "master key rotation done: every stored secret was verified under NSHMAIL_MASTER_KEY; \
             NSHMAIL_MASTER_KEY_OLD can be removed"
        );
    } else {
        tracing::info!(
            files = plans.len(),
            secrets = entries_total,
            "stored secrets verified under NSHMAIL_MASTER_KEY"
        );
    }
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".bak-old-key");
    PathBuf::from(p)
}

/// A copy left by a rotation interrupted after its rename: removed once it holds exactly what the
/// rotated file holds, else kept for a person to look at.
fn retire_stale_backup(
    backup: &Path,
    entries: &BTreeMap<String, String>,
    current: [u8; 32],
    old: Option<[u8; 32]>,
) {
    let same = EncryptedFileStore::with_key(backup.to_path_buf(), current)
        .read_all(old)
        .ok()
        .is_some_and(|(b, _)| &b == entries);
    if same && std::fs::remove_file(backup).is_ok() {
        tracing::info!("removed {} left by an earlier rotation", backup.display());
    } else {
        tracing::warn!(
            "{} is left from an interrupted key rotation and does not match the current file \
             under the configured keys; check it and remove it",
            backup.display()
        );
    }
}

fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
}
