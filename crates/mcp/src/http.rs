//! Streamable HTTP transport: a stateless `POST /mcp` that speaks JSON-RPC and
//! answers `application/json`. A `GET` SSE stream is not offered, and there are
//! no sessions, so no `Mcp-Session-Id` is issued.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::api::{Notify, NotifyApi};
use crate::protocol;

/// Builds the backend for one request from the request's own API key; the key
/// is `None` when the caller sent none (a mounting server may have already
/// authenticated and use a backend that ignores it).
pub type BackendFactory = Arc<dyn Fn(Option<&str>) -> Arc<dyn Notify> + Send + Sync>;

#[derive(Clone)]
struct Web {
    make: BackendFactory,
    /// Standalone mode: requests must carry exactly this key.
    require_key: Option<String>,
}

/// The router a server mounts: `app.merge(notify_mcp::router(make))`. It owns
/// `POST /mcp` only and does no authentication of its own — the mounting server
/// runs its usual API-key check and the factory can then ignore the header.
pub fn router(make: BackendFactory) -> Router {
    routes(Web {
        make,
        require_key: None,
    })
}

/// The standalone transport behind `--http`: each request's Bearer key becomes
/// the key for its REST calls; with `key` set, only that key is accepted.
pub fn client_router(server: impl Into<String>, key: Option<String>, timeout: Duration) -> Router {
    let server = server.into();
    let http = reqwest::Client::new();
    let fallback = key.clone();
    let make: BackendFactory = Arc::new(move |header_key: Option<&str>| {
        let key = header_key.map(str::to_string).or_else(|| fallback.clone());
        Arc::new(NotifyApi::with_client(
            server.clone(),
            key.unwrap_or_default(),
            http.clone(),
            timeout,
        ))
    });
    routes(Web {
        make,
        require_key: key,
    })
}

fn routes(web: Web) -> Router {
    Router::new()
        .route(
            "/mcp",
            post(post_mcp)
                .get(method_not_allowed)
                .delete(method_not_allowed),
        )
        .with_state(web)
}

async fn post_mcp(State(web): State<Web>, headers: HeaderMap, body: Bytes) -> Response {
    let key = api_key(&headers);
    if let Some(required) = &web.require_key {
        match &key {
            Some(sent) if sent == required => {}
            _ => return unauthorized(),
        }
    }
    let backend = (web.make)(key.as_deref());

    let request: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(e) => {
            let error = json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": format!("invalid JSON: {e}") },
            });
            return (StatusCode::BAD_REQUEST, Json(error)).into_response();
        }
    };

    match request {
        Value::Array(items) => {
            let mut answers = Vec::with_capacity(items.len());
            for item in &items {
                if let Some(answer) = protocol::dispatch(backend.as_ref(), item).await {
                    answers.push(answer);
                }
            }
            if answers.is_empty() {
                StatusCode::ACCEPTED.into_response()
            } else {
                Json(Value::Array(answers)).into_response()
            }
        }
        one => match protocol::dispatch(backend.as_ref(), &one).await {
            Some(answer) => Json(answer).into_response(),
            None => StatusCode::ACCEPTED.into_response(),
        },
    }
}

async fn method_not_allowed() -> Response {
    let mut response = StatusCode::METHOD_NOT_ALLOWED.into_response();
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static("POST"));
    response
}

fn api_key(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        let token = value.trim();
        let token = token
            .strip_prefix("Bearer ")
            .or_else(|| token.strip_prefix("bearer "))
            .unwrap_or(token)
            .trim();
        if !token.is_empty() {
            return Some(token.to_string());
        }
    }
    headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(json!({
            "code": "ERR_AUTH",
            "message": "a Bearer API key (nb_…) is required",
        })),
    )
        .into_response()
}

/// Runs a router on `addr` until the process ends.
pub async fn serve(listener: tokio::net::TcpListener, router: Router) -> std::io::Result<()> {
    axum::serve(listener, router).await
}

pub async fn bind(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::bind(addr).await
}
