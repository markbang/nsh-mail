//! Sources: the places notifications come from. A mail account is a `kind = mail` source created
//! by its account (see `mail_core::feed`); every other kind is created here, and the HTTP kinds get
//! an ingest URL whose token is shown once and only stored hashed.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use mail_core::feed::{NewFeedMessage, NewSource, Source, SourcePatch, now_ms};
use mail_core::secrets::SecretStore;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AppState,
    api::{ApiResult, AuthUser},
    crypto, ingest,
};

/// A source as the app sees it: the stored fields plus whether a webhook secret is set. Secrets
/// are never returned.
fn view(state: &AppState, source: &Source) -> Value {
    let mut value = serde_json::to_value(source).unwrap_or(Value::Null);
    let has_secret = state
        .notify_secrets
        .get(&ingest::secret_key(&source.id))
        .ok()
        .flatten()
        .is_some();
    value["hasSecret"] = json!(has_secret);
    value
}

fn with_token(state: &AppState, source: &Source, token: String) -> Value {
    json!({
        "source": view(state, source),
        "token": token,
        "ingestUrl": ingest_url(&state.config.public_url, &source.kind, &token),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSource {
    #[serde(flatten)]
    source: NewSource,
    /// Write-only: the HMAC key a webhook's `X-Hub-Signature-256` is checked against.
    #[serde(default)]
    secret: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSource {
    #[serde(flatten)]
    patch: SourcePatch,
    /// Empty string clears it.
    #[serde(default)]
    secret: Option<String>,
}

/// The shipped presets the settings UI offers; each is a mapping the user can then edit.
pub async fn list_presets() -> ApiResult<impl IntoResponse> {
    Ok(Json(ingest::presets::list()))
}

pub async fn list_sources(
    State(s): State<Arc<AppState>>,
    user: AuthUser,
) -> ApiResult<impl IntoResponse> {
    let sources: Vec<Value> = s
        .mail_db
        .feed(user.id)
        .sources()?
        .iter()
        .map(|source| view(&s, source))
        .collect();
    Ok(Json(json!({ "sources": sources })))
}

pub async fn get_source(
    State(s): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let source = s.mail_db.feed(user.id).source(&id)?;
    Ok(Json(view(&s, &source)))
}

pub async fn create_source(
    State(s): State<Arc<AppState>>,
    user: AuthUser,
    Json(input): Json<CreateSource>,
) -> ApiResult<impl IntoResponse> {
    let feed = s.mail_db.feed(user.id);
    let (token, hash) = match mail_core::feed::TOKEN_KINDS.contains(&input.source.kind.as_str()) {
        true => {
            let token = crypto::new_token();
            let hash = crypto::sha256(token.as_bytes());
            (Some(token), Some(hash))
        }
        false => (None, None),
    };
    let source = feed.create_source(&input.source, hash.as_deref())?;
    if let Some(secret) = input.secret.as_deref().filter(|s| !s.is_empty()) {
        s.notify_secrets
            .set(&ingest::secret_key(&source.id), secret)?;
    }
    let body = match token {
        Some(token) => with_token(&s, &source, token),
        None => json!({ "source": view(&s, &source) }),
    };
    Ok((StatusCode::CREATED, Json(body)).into_response())
}

pub async fn update_source(
    State(s): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(input): Json<UpdateSource>,
) -> ApiResult<impl IntoResponse> {
    let source = s.mail_db.feed(user.id).update_source(&id, &input.patch)?;
    if let Some(secret) = input.secret {
        let key = ingest::secret_key(&id);
        if secret.is_empty() {
            s.notify_secrets.delete(&key)?;
        } else {
            s.notify_secrets.set(&key, &secret)?;
        }
    }
    Ok(Json(view(&s, &source)))
}

pub async fn delete_source(
    State(s): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    s.mail_db.feed(user.id).delete_source(&id)?;
    s.notify_secrets.delete(&ingest::secret_key(&id))?;
    Ok(StatusCode::NO_CONTENT)
}

/// A new token, and the URL to send to; the old token stops working at once.
pub async fn rotate_token(
    State(s): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let token = crypto::new_token();
    let hash = crypto::sha256(token.as_bytes());
    let source = s.mail_db.feed(user.id).set_token_hash(&id, &hash)?;
    Ok(Json(json!({ "source": source, "token": token,
                "ingestUrl": ingest_url(&s.config.public_url, &source.kind, &token) })))
}

/// Stores a test message through the same path as real ingestion, so the app can show what a
/// source looks like before anything real arrives.
pub async fn test_source(
    State(s): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let feed = s.mail_db.feed(user.id);
    let source = feed.source(&id)?;
    let stored = feed.insert(&NewFeedMessage {
        source_id: source.id,
        title: format!("Test message from {}", source.name),
        body_text: Some(
            "This is a test message. Notifications sent to this source show up here.".into(),
        ),
        received_at: now_ms(),
        tags: vec!["test".into()],
        ..Default::default()
    })?;
    Ok(Json(json!({ "message": stored.message() })))
}

fn ingest_url(public_url: &str, kind: &str, token: &str) -> String {
    let base = public_url.trim_end_matches('/');
    match kind {
        "webhook" => format!("{base}/hooks/{token}"),
        "gotify" => format!("{base}/message?token={token}"),
        _ => format!("{base}/{token}"),
    }
}
