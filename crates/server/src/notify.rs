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
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    AppState,
    agent::{Caller, SourceList},
    api::{ApiError, ApiResult, AuthUser},
    crypto, ingest,
};

/// A source as its owner sees it: the stored fields plus whether a webhook secret is set (the
/// token and the secret themselves are only ever answered once). An agent's key gets it without
/// `config`, which holds the ingest mapping and may name a bot's credentials.
#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceView {
    id: String,
    kind: String,
    name: String,
    icon: Option<String>,
    color: Option<String>,
    #[schema(value_type = Option<Object>)]
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<Value>,
    min_priority: i64,
    enabled: bool,
    created_at: i64,
    /// An ingest URL exists for this source.
    has_token: bool,
    /// A webhook signing secret is set.
    has_secret: bool,
}

fn view(state: &AppState, source: &Source, for_key: bool) -> SourceView {
    let has_secret = state
        .notify_secrets
        .get(&ingest::secret_key(&source.id))
        .ok()
        .flatten()
        .is_some();
    SourceView {
        id: source.id.clone(),
        kind: source.kind.clone(),
        name: source.name.clone(),
        icon: source.icon.clone(),
        color: source.color.clone(),
        config: (!for_key).then(|| source.config.clone()),
        min_priority: source.min_priority,
        enabled: source.enabled,
        created_at: source.created_at,
        has_token: source.has_token,
        has_secret,
    }
}

fn with_token(state: &AppState, source: &Source, token: String) -> Value {
    json!({
        "source": view(state, source, false),
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

#[utoipa::path(
    get,
    path = "/v1/sources",
    tag = "sources",
    security(("bearer" = [])),
    description = "Every source the caller may see. An API key sees only its own sources, \
                   without `config`.",
    responses(
        (status = 200, description = "`{sources}`", body = crate::agent::SourceList),
        (status = 401, description = "No valid session or API key"),
    )
)]
pub async fn list_sources(
    State(s): State<Arc<AppState>>,
    caller: Caller,
) -> ApiResult<Json<SourceList>> {
    let sources: Vec<SourceView> = s
        .mail_db
        .feed(caller.user_id())
        .sources()?
        .iter()
        .filter(|source| caller.may_see(&source.id))
        .map(|source| view(&s, source, caller.is_key()))
        .collect();
    Ok(Json(SourceList { sources }))
}

#[utoipa::path(
    get,
    path = "/v1/sources/{id}",
    tag = "sources",
    security(("bearer" = [])),
    params(("id" = String, Path, description = "Source id (src_…)")),
    responses(
        (status = 200, description = "The source", body = SourceView),
        (status = 404, description = "No such source, or outside the API key's sources"),
    )
)]
pub async fn get_source(
    State(s): State<Arc<AppState>>,
    caller: Caller,
    Path(id): Path<String>,
) -> ApiResult<Json<SourceView>> {
    let source = s.mail_db.feed(caller.user_id()).source(&id)?;
    if !caller.may_see(&id) {
        return Err(ApiError::not_found());
    }
    Ok(Json(view(&s, &source, caller.is_key())))
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
        None => json!({ "source": view(&s, &source, false) }),
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
    Ok(Json(view(&s, &source, false)))
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
