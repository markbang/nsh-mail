//! Where passwords and OAuth tokens live: the OS keyring, an AES-256-GCM encrypted file (Android,
//! Linux without Secret Service, the server), or memory (tests).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

use crate::error::err;
use crate::types::*;

/// Blocking key/value secret storage. The engine calls it from tokio's blocking pool.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

fn secrets_err(what: &str, e: impl std::fmt::Display) -> Error {
    err(ErrorCode::Secrets, format!("{what}: {e}"))
}

#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned())
    }
    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.to_string(), value.to_string());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
        Ok(())
    }
}

/// Windows Credential Manager / Linux Secret Service / macOS Keychain.
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
pub struct KeyringStore {
    service: String,
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
impl KeyringStore {
    /// Fails when the platform store is unavailable (e.g. no Secret Service in this Linux session).
    pub fn new(service: impl Into<String>) -> Result<Self> {
        if let Err(e) = keyring::Entry::store_status() {
            return Err(secrets_err("system keyring unavailable", e));
        }
        Ok(Self {
            service: service.into(),
        })
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, key).map_err(|e| secrets_err("keyring entry", e))
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
impl SecretStore for KeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(secrets_err("keyring read failed", e)),
        }
    }
    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.entry(key)?
            .set_password(value)
            .map_err(|e| secrets_err("keyring write failed", e))
    }
    fn delete(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(secrets_err("keyring delete failed", e)),
        }
    }
}

/// A JSON map of `key -> base64(nonce || AES-256-GCM(value))`, the key name bound as AAD.
/// Both the data file and the key file are created 0600.
pub struct EncryptedFileStore {
    path: PathBuf,
    cipher: Aes256Gcm,
    lock: Mutex<()>,
}

impl EncryptedFileStore {
    /// `dir/secrets.json`, keyed by `dir/secrets.key` (generated on first use).
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)
            .map_err(|e| secrets_err("cannot create the data directory", e))?;
        let key_path = dir.join("secrets.key");
        let key = match std::fs::read(&key_path) {
            Ok(k) if k.len() == 32 => k,
            Ok(_) => {
                return Err(err(
                    ErrorCode::Secrets,
                    format!("{} is corrupt (expected 32 bytes)", key_path.display()),
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut k = vec![0u8; 32];
                getrandom::fill(&mut k).map_err(|e| secrets_err("no OS randomness", e))?;
                write_private(&key_path, &k)?;
                k
            }
            Err(e) => return Err(secrets_err("cannot read the secrets key", e)),
        };
        Ok(Self::with_key(
            dir.join("secrets.json"),
            key.try_into().expect("32 bytes"),
        ))
    }

    /// For callers that manage the key themselves (the server's `NSHMAIL_MASTER_KEY`).
    pub fn with_key(path: PathBuf, key: [u8; 32]) -> Self {
        Self {
            path,
            cipher: Aes256Gcm::new(&Key::<Aes256Gcm>::from(key)),
            lock: Mutex::new(()),
        }
    }

    fn load(&self) -> Result<BTreeMap<String, String>> {
        match std::fs::read(&self.path) {
            Ok(b) => {
                serde_json::from_slice(&b).map_err(|e| secrets_err("secrets file is corrupt", e))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(secrets_err("cannot read the secrets file", e)),
        }
    }

    fn save(&self, map: &BTreeMap<String, String>) -> Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        write_private(&tmp, &serde_json::to_vec(map)?)?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| secrets_err("cannot write the secrets file", e))
    }
}

impl EncryptedFileStore {
    fn seal(&self, key: &str, value: &str) -> Result<String> {
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).map_err(|e| secrets_err("no OS randomness", e))?;
        let ct = self
            .cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: value.as_bytes(),
                    aad: key.as_bytes(),
                },
            )
            .map_err(|_| err(ErrorCode::Secrets, "encryption failed"))?;
        let mut sealed = nonce.to_vec();
        sealed.extend_from_slice(&ct);
        Ok(B64.encode(sealed))
    }

    /// Every entry decrypted, each with this store's key or else `old`, and how many needed `old`.
    /// Fails, naming how many entries open with neither key, before anything is returned.
    pub fn read_all(&self, old: Option<[u8; 32]>) -> Result<(BTreeMap<String, String>, usize)> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let old = old.map(|k| Aes256Gcm::new(&Key::<Aes256Gcm>::from(k)));
        let map = self.load()?;
        let (mut plain, mut used_old, mut failed) = (BTreeMap::new(), 0, 0);
        for (key, sealed) in &map {
            match open(&self.cipher, key, sealed) {
                Ok(v) => {
                    plain.insert(key.clone(), v);
                }
                Err(_) => match old.as_ref().map(|c| open(c, key, sealed)) {
                    Some(Ok(v)) => {
                        used_old += 1;
                        plain.insert(key.clone(), v);
                    }
                    _ => failed += 1,
                },
            }
        }
        if failed > 0 {
            return Err(err(
                ErrorCode::Secrets,
                format!(
                    "{failed} of {} secrets do not decrypt with the given key{}",
                    map.len(),
                    if old.is_some() { "s" } else { "" }
                ),
            ));
        }
        Ok((plain, used_old))
    }

    /// Replaces the whole file with `entries`, all sealed under this store's key, in one rename.
    pub fn write_all(&self, entries: &BTreeMap<String, String>) -> Result<()> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let map = entries
            .iter()
            .map(|(k, v)| Ok((k.clone(), self.seal(k, v)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        self.save(&map)
    }
}

fn open(cipher: &Aes256Gcm, key: &str, sealed: &str) -> Result<String> {
    let bytes = B64
        .decode(sealed)
        .map_err(|e| secrets_err("secrets file is corrupt", e))?;
    if bytes.len() < 12 {
        return Err(err(ErrorCode::Secrets, "secrets file is corrupt"));
    }
    let (nonce, ct) = bytes.split_at(12);
    let nonce =
        Nonce::try_from(nonce).map_err(|_| err(ErrorCode::Secrets, "secrets file is corrupt"))?;
    let plain = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: ct,
                aad: key.as_bytes(),
            },
        )
        .map_err(|_| {
            err(
                ErrorCode::Secrets,
                "cannot decrypt a secret: wrong key or tampered file",
            )
        })?;
    String::from_utf8(plain).map_err(|_| err(ErrorCode::Secrets, "secret is not UTF-8"))
}

impl SecretStore for EncryptedFileStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        match self.load()?.remove(key) {
            Some(sealed) => open(&self.cipher, key, &sealed).map(Some),
            None => Ok(None),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let sealed = self.seal(key, value)?;
        let mut map = self.load()?;
        map.insert(key.to_string(), sealed);
        self.save(&map)
    }

    fn delete(&self, key: &str) -> Result<()> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut map = self.load()?;
        if map.remove(key).is_some() {
            self.save(&map)?;
        }
        Ok(())
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts
        .open(path)
        .map_err(|e| secrets_err(&format!("cannot write {}", path.display()), e))?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| secrets_err(&format!("cannot write {}", path.display()), e))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Keyring,
    EncryptedFile,
}

/// The app's store: the OS keyring when it works, else the encrypted file in `data_dir`
/// (always on Android). The UI should tell the user when it fell back.
pub fn platform_store(data_dir: &Path) -> Result<(Arc<dyn SecretStore>, Backend)> {
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    if let Ok(k) = KeyringStore::new("nsh-mail") {
        // Secret Service can be present yet locked or broken; probe before trusting it.
        if k.get("probe").is_ok() {
            return Ok((Arc::new(k), Backend::Keyring));
        }
    }
    Ok((
        Arc::new(EncryptedFileStore::open(data_dir)?),
        Backend::EncryptedFile,
    ))
}
