use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, OsRng, Payload, rand_core::RngCore},
};
use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

pub fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    OsRng.fill_bytes(&mut b);
    b
}

pub fn b64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn sha256(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

/// 32 random bytes, handed out once; only the SHA-256 is stored.
pub fn new_token() -> String {
    b64url(&random::<32>())
}

/// AES-256-GCM under the master key. Stored form: 12-byte nonce || ciphertext+tag.
/// `aad` binds a ciphertext to its row, so values cannot be swapped between users.
#[derive(Clone)]
pub struct Cipher(Aes256Gcm);

impl Cipher {
    pub fn new(key: &[u8; 32]) -> Self {
        Cipher(Aes256Gcm::new(key.into()))
    }

    pub fn seal(&self, plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
        let nonce = random::<12>();
        let mut out = nonce.to_vec();
        out.extend(
            self.0
                .encrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: plaintext,
                        aad,
                    },
                )
                .expect("AES-GCM encryption cannot fail for in-memory buffers"),
        );
        out
    }

    pub fn open(&self, sealed: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
        if sealed.len() < 12 {
            return Err(anyhow!("stored secret is truncated"));
        }
        let (nonce, ct) = sealed.split_at(12);
        self.0
            .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad })
            .map_err(|_| anyhow!("cannot decrypt stored secret: NSHMAIL_MASTER_KEY differs from the one used to store it"))
    }
}
