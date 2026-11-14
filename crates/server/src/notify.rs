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
use serde::Serialize;
use serde_json::json;

use crate::{
    AppState,
    api::{ApiResult, AuthUser},
    crypto,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WithToken {
    source: Source,
    token: String,
    /// Where to send notifications with that token.
    ingest_url: String,
}

pub async fn list_sources(
    State(s): State<Arc<AppState>>,
    user: AuthUser,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        json!({ "sources": s.mail_db.feed(user.id).sources()? }),
    ))
}

pub async fn get_source(
    State(s): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(s.mail_db.feed(user.id).source(&id)?))
}

pub async fn create_source(
    State(s): State<Arc<AppState>>,
    user: AuthUser,
    Json(new): Json<NewSource>,
) -> ApiResult<impl IntoResponse> {
    let feed = s.mail_db.feed(user.id);
    let (token, hash) = match mail_core::feed::TOKEN_KINDS.contains(&new.kind.as_str()) {
        true => {
            let token = crypto::new_token();
            let hash = crypto::sha256(token.as_bytes());
            (Some(token), Some(hash))
        }
        false => (None, None),
    };
    let source = feed.create_source(&new, hash.as_deref())?;
    let body = match token {
        Some(token) => {
            let ingest_url = ingest_url(&s.config.public_url, &source.kind, &token);
            WithToken {
                source,
                token,
                ingest_url,
            }
        }
        None => {
            return Ok((StatusCode::CREATED, Json(json!({ "source": source }))).into_response());
        }
    };
    Ok((StatusCode::CREATED, Json(body)).into_response())
}

pub async fn update_source(
    State(s): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(patch): Json<SourcePatch>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(s.mail_db.feed(user.id).update_source(&id, &patch)?))
}

pub async fn delete_source(
    State(s): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    s.mail_db.feed(user.id).delete_source(&id)?;
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
