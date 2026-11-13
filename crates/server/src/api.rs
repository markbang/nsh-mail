use std::sync::{Arc, LazyLock};

use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::{
    Json,
    extract::{FromRequestParts, Path, Query, State},
    http::{HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

use mail_core::{
    Engine,
    types::{Draft, DraftAttachment, Error, ErrorCode, Flags, FolderKind, ImapInput, ListQuery},
};

use crate::{AppState, crypto, push::parse_subscription_keys};

/// Error bodies are `mail_core::types::Error` JSON (`{code, message}`), so clients share one decoder.
pub struct ApiError(StatusCode, Error);

impl ApiError {
    pub fn new(status: StatusCode, code: ErrorCode, msg: impl Into<String>) -> Self {
        ApiError(status, Error::new(code, msg))
    }
    fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ErrorCode::InvalidInput, msg)
    }
    fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::Auth,
            "missing or invalid session token; sign in again",
        )
    }
    fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, ErrorCode::NotFound, "not found")
    }
    fn conflict(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, ErrorCode::InvalidInput, msg)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!("internal error: {e:#}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            "internal server error",
        )
    }
}

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        let status = match e.code {
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::Auth
            | ErrorCode::InvalidInput
            | ErrorCode::OAuth
            | ErrorCode::Unsupported => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            ErrorCode::Network | ErrorCode::Tls | ErrorCode::Protocol => StatusCode::BAD_GATEWAY,
            ErrorCode::Secrets | ErrorCode::Storage | ErrorCode::Internal => {
                tracing::error!("mail backend error: {}", e.message);
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        ApiError(status, e)
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// Authenticated user, from `Authorization: Bearer <token>` or `?token=` (for WebSocket and OAuth start).
pub struct AuthUser {
    pub id: i64,
    token_hash: Vec<u8>,
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

pub fn user_for_token(state: &AppState, token: &str) -> ApiResult<AuthUser> {
    let token_hash = crypto::sha256(token.as_bytes());
    match state.db.session_user(&token_hash)? {
        Some(id) => Ok(AuthUser { id, token_hash }),
        None => Err(ApiError::unauthorized()),
    }
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> ApiResult<Self> {
        let header_token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::to_string);
        let token = match header_token {
            Some(t) => t,
            None => Query::<TokenQuery>::try_from_uri(&parts.uri)
                .ok()
                .and_then(|q| q.0.token)
                .ok_or_else(ApiError::unauthorized)?,
        };
        user_for_token(state, token.trim())
    }
}

#[derive(Deserialize)]
pub struct Credentials {
    email: String,
    password: String,
}

/// Verifying against a fixed hash for unknown emails keeps login timing uniform.
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    Argon2::default()
        .hash_password(b"nsh-mail-dummy")
        .expect("argon2")
        .to_string()
});

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| anyhow::anyhow!(e).into())
}

fn issue_session(state: &AppState, user_id: i64) -> ApiResult<Json<Value>> {
    let token = crypto::new_token();
    state
        .db
        .create_session(&crypto::sha256(token.as_bytes()), user_id)?;
    Ok(Json(json!({ "token": token })))
}

pub async fn signup(
    State(state): State<Arc<AppState>>,
    Json(c): Json<Credentials>,
) -> ApiResult<impl IntoResponse> {
    if !state.config.allow_signup {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::Unsupported,
            "sign-up is disabled on this server",
        ));
    }
    let email = c.email.trim().to_lowercase();
    if !email.contains('@') || email.len() > 254 {
        return Err(ApiError::bad_request("enter a valid email address"));
    }
    if c.password.chars().count() < 8 || c.password.len() > 1024 {
        return Err(ApiError::bad_request(
            "password must be at least 8 characters",
        ));
    }
    let hash = blocking(move || {
        Argon2::default()
            .hash_password(c.password.as_bytes())
            .map(|h| h.to_string())
    })
    .await?
    .map_err(|e| anyhow::anyhow!("argon2: {e}"))?;
    let Some(id) = state.db.create_user(&email, &hash)? else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::InvalidInput,
            "an account with this email already exists",
        ));
    };
    Ok((StatusCode::CREATED, issue_session(&state, id)?))
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(c): Json<Credentials>,
) -> ApiResult<Json<Value>> {
    let user = state.db.user_by_email(c.email.trim())?;
    let hash = user
        .as_ref()
        .map(|u| u.password_hash.clone())
        .unwrap_or_else(|| DUMMY_HASH.clone());
    let ok = blocking(move || {
        Argon2::default()
            .verify_password(c.password.as_bytes(), hash.as_str())
            .is_ok()
    })
    .await?;
    match user {
        Some(u) if ok => issue_session(&state, u.id),
        _ => Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::Auth,
            "wrong email or password",
        )),
    }
}

pub async fn logout(State(state): State<Arc<AppState>>, user: AuthUser) -> ApiResult<StatusCode> {
    state.db.delete_session(&user.token_hash)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(State(state): State<Arc<AppState>>, user: AuthUser) -> ApiResult<Json<Value>> {
    let u = state.db.user(user.id)?.ok_or_else(ApiError::unauthorized)?;
    Ok(Json(json!({
        "email": u.email,
        "hideContent": u.hide_content,
        "maxAttachmentBytes": state.config.max_attachments,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MePatch {
    hide_content: Option<bool>,
}

pub async fn update_me(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(p): Json<MePatch>,
) -> ApiResult<Json<Value>> {
    if let Some(h) = p.hide_content {
        state.db.set_hide_content(user.id, h)?;
    }
    me(State(state), user).await
}

async fn mail(state: &AppState, user: &AuthUser) -> ApiResult<Engine> {
    Ok(state.mailboxes.get(user.id).await?)
}

pub async fn list_accounts(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(mail(&s, &u).await?.accounts().await?))
}

pub async fn add_account(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Json(input): Json<ImapInput>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(mail(&s, &u).await?.add_imap_account(input).await?),
    ))
}

pub async fn delete_account(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    mail(&s, &u).await?.remove_account(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct DetectParams {
    email: String,
}

/// Provider presets and Mozilla autoconfig, for clients that cannot look them up (the web).
pub async fn detect(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Query(p): Query<DetectParams>,
) -> ApiResult<impl IntoResponse> {
    match mail(&s, &u).await?.detect(p.email.trim()).await {
        Ok(found) => Ok(Json(found)),
        Err(e) if e.code == ErrorCode::InvalidInput => Err(e.into()),
        // The user enters the servers instead.
        Err(e) => {
            tracing::info!("autoconfig lookup failed: {}", e.message);
            Ok(Json(None))
        }
    }
}

/// Syncs every account now (pull to refresh).
pub async fn sync(State(s): State<Arc<AppState>>, u: AuthUser) -> ApiResult<StatusCode> {
    mail(&s, &u).await?.sync_all().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ListParams {
    folder: Option<FolderKind>,
    account: Option<String>,
    before: Option<String>,
    limit: Option<u32>,
}

const MAX_LIMIT: u32 = 200;

pub async fn list_messages(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Query(p): Query<ListParams>,
) -> ApiResult<impl IntoResponse> {
    let query = ListQuery {
        folder: p.folder.unwrap_or(FolderKind::Inbox),
        account: p.account,
        before: p.before,
        limit: p.limit.unwrap_or(50).clamp(1, MAX_LIMIT),
    };
    Ok(Json(mail(&s, &u).await?.list(query).await?))
}

pub async fn get_message(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(mail(&s, &u).await?.get(&id).await?))
}

/// One attachment's bytes, always as a download: `Content-Security-Policy: sandbox` and
/// `nosniff` keep an HTML or SVG attachment from running as this origin's page.
pub async fn get_attachment(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Path((id, index)): Path<(String, u32)>,
) -> ApiResult<Response> {
    let (meta, bytes) = mail(&s, &u).await?.attachment(&id, index).await?;
    let mime = HeaderValue::from_str(&meta.mime_type)
        .ok()
        .filter(|_| {
            meta.mime_type
                .split_once('/')
                .is_some_and(|(t, s)| !t.is_empty() && !s.is_empty())
        })
        .unwrap_or(HeaderValue::from_static("application/octet-stream"));
    let name = meta
        .filename
        .unwrap_or_else(|| format!("attachment-{}", index + 1));
    let disposition = HeaderValue::from_str(&content_disposition(&name))
        .map_err(|e| anyhow::anyhow!("content disposition: {e}"))?;
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CONTENT_DISPOSITION, disposition),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
            (
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static("sandbox"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// `attachment` with an ASCII `filename` for old clients and the exact name as RFC 5987's
/// `filename*` (UTF-8, percent-encoded), which browsers prefer.
fn content_disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| match c {
            ' ' | '!'..='~' if !matches!(c, '"' | '\\') => c,
            _ => '_',
        })
        .collect();
    let mut encoded = String::with_capacity(name.len() * 3);
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

pub async fn patch_message(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Path(id): Path<String>,
    Json(f): Json<Flags>,
) -> ApiResult<StatusCode> {
    mail(&s, &u).await?.set_flags(&id, f).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct MoveTo {
    folder: FolderKind,
}

pub async fn move_message(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Path(id): Path<String>,
    Json(m): Json<MoveTo>,
) -> ApiResult<StatusCode> {
    mail(&s, &u).await?.move_to(&id, m.folder).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn send(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Json(d): Json<Draft>,
) -> ApiResult<StatusCode> {
    if d.to.is_empty() && d.cc.is_empty() && d.bcc.is_empty() {
        return Err(ApiError::bad_request("add at least one recipient"));
    }
    let staged = crate::uploads::staged(&s, &u, &d.attachments).await?;
    let attachments = d.attachments.clone();
    mail(&s, &u).await?.send_with(d, staged).await?;
    let sent = attachments.iter().filter_map(|a| match a {
        DraftAttachment::Upload { id } => Some(id.as_str()),
        DraftAttachment::Forward { .. } => None,
    });
    crate::uploads::remove(&s, &u, sent).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SearchParams {
    q: String,
    limit: Option<u32>,
}

pub async fn search(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Query(p): Query<SearchParams>,
) -> ApiResult<impl IntoResponse> {
    let limit = p.limit.unwrap_or(50).clamp(1, MAX_LIMIT);
    Ok(Json(mail(&s, &u).await?.search(p.q.trim(), limit).await?))
}

pub async fn unread(State(s): State<Arc<AppState>>, u: AuthUser) -> ApiResult<impl IntoResponse> {
    Ok(Json(mail(&s, &u).await?.unread().await?))
}

#[derive(Deserialize)]
pub struct SubscriptionKeys {
    p256dh: String,
    auth: String,
}

#[derive(Deserialize)]
pub struct Subscription {
    endpoint: String,
    keys: SubscriptionKeys,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum NewDevice {
    Fcm {
        token: String,
    },
    Webpush {
        subscription: Subscription,
        /// The VAPID public key the browser subscribed with; older clients leave it out.
        #[serde(rename = "vapidKey")]
        vapid_key: Option<String>,
    },
}

pub async fn add_device(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Json(d): Json<NewDevice>,
) -> ApiResult<impl IntoResponse> {
    let new_id = mail_core::oauth::random_token(16);
    let id = match d {
        NewDevice::Fcm { token } => {
            if token.trim().is_empty() {
                return Err(ApiError::bad_request("token is empty"));
            }
            s.db.upsert_device(u.id, &new_id, "fcm", token.trim(), None, None, None)?
        }
        NewDevice::Webpush {
            subscription: sub,
            vapid_key,
        } => {
            if vapid_key.is_some_and(|k| k != s.vapid_public_key) {
                return Err(ApiError::conflict(
                    "the push subscription was made for another server key; fetch \
                     /v1/push/vapid-key and subscribe again",
                ));
            }
            parse_subscription_keys(&sub.keys.p256dh, &sub.keys.auth)
                .map_err(|e| ApiError::bad_request(format!("invalid push subscription: {e}")))?;
            if !sub.endpoint.starts_with("https://") && !sub.endpoint.starts_with("http://") {
                return Err(ApiError::bad_request(
                    "invalid push subscription: endpoint must be a URL",
                ));
            }
            s.db.upsert_device(
                u.id,
                &new_id,
                "webpush",
                &sub.endpoint,
                Some(&sub.keys.p256dh),
                Some(&sub.keys.auth),
                Some(&s.vapid_public_key),
            )?
        }
    };
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub async fn delete_device(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    if s.db.delete_device(u.id, &id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

pub async fn vapid_key(State(s): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "key": s.vapid_public_key }))
}

pub async fn health(State(s): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    s.db.ping()?;
    Ok(Json(
        json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }),
    ))
}
