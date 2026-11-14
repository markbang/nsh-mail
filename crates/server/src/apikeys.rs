//! API keys: how an agent reads the notification inbox. A key (`nb_…`) is shown once; only its
//! SHA-256 is stored, next to the scope it may use and the sources it may see.

use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::{StatusCode, header, request::Parts},
};
use mail_core::types::ErrorCode;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    api::{ApiError, ApiResult},
    crypto,
    db::Db,
};

/// Keys are stamped like the feed (ms since the epoch), so the app shows one timeline.
fn now() -> i64 {
    mail_core::feed::now_ms()
}

/// Every key starts with this, so a leaked one is recognisable and a client can tell a key from a
/// session token by looking at it.
pub const PREFIX: &str = "nb_";
/// Characters of the token kept for the list; the rest is not stored anywhere.
const PREFIX_LEN: usize = 8;

/// A key as its owner sees it. `scopes` holds `read` and, for keys that may change the inbox,
/// `write`; every key can read.
#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    pub id: String,
    /// Who owns it; never serialized.
    #[serde(skip)]
    pub user_id: i64,
    pub name: String,
    pub prefix: String,
    pub scopes: Vec<String>,
    /// Empty: every source.
    pub source_ids: Vec<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

impl Key {
    pub fn can_write(&self) -> bool {
        self.scopes.iter().any(|s| s == "write")
    }

    /// The sources it may see; `None` is every source.
    pub fn sources(&self) -> Option<Vec<String>> {
        (!self.source_ids.is_empty()).then(|| self.source_ids.clone())
    }
}

/// Who an `nb_…` key belongs to, resolved once per request.
#[derive(Clone, Debug)]
pub struct Agent {
    pub user_id: i64,
    pub key_id: String,
    pub write: bool,
    /// The sources it may see; `None` is every source of its owner.
    pub sources: Option<Vec<String>>,
}

impl Agent {
    pub fn may_see(&self, source_id: &str) -> bool {
        match &self.sources {
            Some(sources) => sources.iter().any(|s| s == source_id),
            None => true,
        }
    }

    pub fn require_write(&self) -> ApiResult<()> {
        if self.write {
            return Ok(());
        }
        Err(ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::Auth,
            "this API key may only read; create one with the write scope to mark notifications \
             read or archive them",
        ))
    }
}

impl FromRequestParts<Arc<AppState>> for Agent {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> ApiResult<Self> {
        let token = bearer(parts).ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                ErrorCode::Auth,
                "an API key is required: send `Authorization: Bearer nb_…` or `X-API-Key`",
            )
        })?;
        resolve(state, &token)
    }
}

/// The scope behind a token, checked for revocation and expiry. One place, so the HTTP extractor
/// and the MCP transport cannot disagree about what a key may do.
pub fn resolve(state: &AppState, token: &str) -> ApiResult<Agent> {
    let key = state
        .db
        .api_key(&crypto::sha256(token.as_bytes()))?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                ErrorCode::Auth,
                "unknown API key: it was never issued here, or it was deleted",
            )
        })?;
    if key.revoked_at.is_some() {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::Auth,
            "this API key was revoked; create another in Settings → API keys",
        ));
    }
    if key.expires_at.is_some_and(|at| at <= now()) {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::Auth,
            "this API key expired; create another in Settings → API keys",
        ));
    }
    state.db.touch_api_key(&key.id)?;
    Ok(Agent {
        user_id: key.user_id,
        key_id: key.id.clone(),
        write: key.can_write(),
        sources: key.sources(),
    })
}

/// `Authorization: Bearer nb_…` or `X-API-Key: nb_…`.
pub fn bearer(parts: &Parts) -> Option<String> {
    let from_header = parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .map(str::trim)
        .filter(|t| t.starts_with(PREFIX))
        .map(str::to_string);
    from_header.or_else(|| {
        parts
            .headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|t| t.starts_with(PREFIX))
            .map(str::to_string)
    })
}

/// A freshly made token: shown to its owner once, and kept only as `prefix` (for the list) and
/// `hash`.
pub struct Issued {
    pub token: String,
    pub prefix: String,
    pub hash: Vec<u8>,
}

pub fn new_token() -> Issued {
    let token = format!("{PREFIX}{}", crypto::new_token());
    Issued {
        prefix: token.chars().take(PREFIX_LEN).collect(),
        hash: crypto::sha256(token.as_bytes()),
        token,
    }
}

/// What a new key may do.
pub struct NewKey<'a> {
    pub name: &'a str,
    pub scopes: &'a [&'a str],
    pub source_ids: &'a [String],
    pub expires_at: Option<i64>,
}

impl Db {
    pub fn create_api_key(
        &self,
        user_id: i64,
        issued: &Issued,
        new: &NewKey,
    ) -> anyhow::Result<Key> {
        let key = Key {
            id: format!("key_{}", mail_core::oauth::random_token(9)),
            user_id,
            name: new.name.trim().to_string(),
            prefix: issued.prefix.clone(),
            scopes: new.scopes.iter().map(|s| s.to_string()).collect(),
            source_ids: new.source_ids.to_vec(),
            created_at: now(),
            expires_at: new.expires_at,
            last_used_at: None,
            revoked_at: None,
        };
        self.with(|c| {
            c.execute(
                "INSERT INTO api_keys (id, user_id, name, hash, prefix, scopes, source_ids,
                                       created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    key.id,
                    key.user_id,
                    key.name,
                    issued.hash,
                    key.prefix,
                    key.scopes.join(","),
                    serde_json::to_string(&key.source_ids).unwrap_or_else(|_| "[]".into()),
                    key.created_at,
                    key.expires_at
                ],
            )
            .map(drop)
        })?;
        Ok(key)
    }

    pub fn api_keys(&self, user_id: i64) -> anyhow::Result<Vec<Key>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, user_id, name, prefix, scopes, source_ids, created_at, expires_at,
                        last_used_at, revoked_at
                 FROM api_keys WHERE user_id = ?1 ORDER BY created_at, id",
            )?;
            stmt.query_map([user_id], key_row)?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
    }

    /// The key behind a token's hash; revoked and expired ones are still returned so the caller
    /// can say which it is.
    pub fn api_key(&self, hash: &[u8]) -> anyhow::Result<Option<Key>> {
        self.with(|c| {
            c.query_row(
                "SELECT id, user_id, name, prefix, scopes, source_ids, created_at, expires_at,
                        last_used_at, revoked_at
                 FROM api_keys WHERE hash = ?1",
                [hash],
                key_row,
            )
            .optional()
        })
    }

    pub fn revoke_api_key(&self, user_id: i64, id: &str) -> anyhow::Result<bool> {
        self.with(|c| {
            c.execute(
                "UPDATE api_keys SET revoked_at = ?3 WHERE user_id = ?1 AND id = ?2
                   AND revoked_at IS NULL",
                rusqlite::params![user_id, id, now()],
            )
            .map(|n| n > 0)
        })
    }

    /// One statement, and a write at most a minute: an agent polling every second must not turn
    /// every request into a database write.
    pub fn touch_api_key(&self, id: &str) -> anyhow::Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE api_keys SET last_used_at = ?2
                 WHERE id = ?1 AND (last_used_at IS NULL OR last_used_at < ?2 - 60000)",
                rusqlite::params![id, now()],
            )
            .map(drop)
        })
    }
}

fn key_row(r: &rusqlite::Row) -> rusqlite::Result<Key> {
    let scopes: String = r.get(4)?;
    let source_ids: String = r.get(5)?;
    Ok(Key {
        id: r.get(0)?,
        user_id: r.get(1)?,
        name: r.get(2)?,
        prefix: r.get(3)?,
        scopes: scopes
            .split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        source_ids: serde_json::from_str(&source_ids).unwrap_or_default(),
        created_at: r.get(6)?,
        expires_at: r.get(7)?,
        last_used_at: r.get(8)?,
        revoked_at: r.get(9)?,
    })
}
