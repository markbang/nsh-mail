use sha2::{Digest, Sha256};

pub fn sha256(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

/// 32 random bytes (base64url), handed out once; only the SHA-256 is stored.
pub fn new_token() -> String {
    mail_core::oauth::random_token(32)
}
