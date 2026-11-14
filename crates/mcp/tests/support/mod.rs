//! A stub of the notification inbox REST API, enough for the tools: one feed,
//! one message, mark read, sources, plus the failures the client must surface.

use std::collections::HashMap;

use axum::extract::{Path, Query};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

pub const KEY: &str = "nb_good";

/// Starts the stub on a random port and returns its base URL.
pub async fn serve() -> String {
    let app = Router::new()
        .route("/v1/messages", get(messages))
        .route("/v1/messages/{id}", get(one))
        .route("/v1/messages/{id}/read", post(read))
        .route("/v1/sources", get(sources));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

/// One message; `body` is long enough to exercise clipping in list results.
pub fn message(id: &str, title: &str, body: &str) -> Value {
    json!({
        "id": id,
        "source_id": "src_mail",
        "sender": "CI",
        "title": title,
        "body_text": body,
        "link": "https://example.com/build/1",
        "received_at": "2026-11-14T10:00:00Z",
        "priority": 7,
        "tags": ["ci", "deploy"],
        "read_at": null
    })
}

fn auth(headers: &HeaderMap) -> Option<Response> {
    let sent = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if sent == format!("Bearer {KEY}")
        || headers.get("x-api-key").and_then(|v| v.to_str().ok()) == Some(KEY)
    {
        return None;
    }
    Some(
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"code": "ERR_AUTH", "message": "invalid API key"})),
        )
            .into_response(),
    )
}

fn bad(code: &str, message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"code": code, "message": message})),
    )
        .into_response()
}

/// Three unread messages; `limit` pages them with the cursor `c2`.
async fn messages(headers: HeaderMap, Query(params): Query<HashMap<String, String>>) -> Response {
    if let Some(response) = auth(&headers) {
        return response;
    }
    let limit: u32 = match params.get("limit").and_then(|v| v.parse().ok()) {
        Some(limit) => limit,
        None => return bad("ERR_BAD_REQUEST", "limit is required"),
    };
    if !params.contains_key("q") && params.get("unread").map(String::as_str) != Some("1") {
        return bad(
            "ERR_BAD_REQUEST",
            "unread=1 is required for the unread feed",
        );
    }
    if params.get("q").map(String::as_str) == Some("slow") {
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    }
    if params
        .get("source")
        .map(String::as_str)
        .is_some_and(|s| s == "src_forbidden")
    {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"code": "ERR_SCOPE", "message": "this API key cannot read source src_forbidden"})),
        )
            .into_response();
    }

    if params.contains_key("q") {
        if params.get("source").map(String::as_str) != Some("src_mail") {
            return bad("ERR_BAD_REQUEST", "search did not carry source=src_mail");
        }
        let found = vec![message("msg_search", "Deploy finished", &"y".repeat(400))];
        return Json(json!({"messages": found})).into_response();
    }

    let all = vec![
        message("msg_1", "Build 1 failed", &"x".repeat(400)),
        message("msg_2", "Deploy finished", "All good."),
        message("msg_3", "New follower", "someone followed you"),
    ];
    let cursor = params.get("cursor").map(String::as_str);
    let start = match cursor {
        None => 0,
        Some("c2") => 2,
        Some(other) => return bad("ERR_BAD_REQUEST", &format!("unknown cursor {other}")),
    };
    let page: Vec<Value> = all.into_iter().skip(start).take(limit as usize).collect();
    let next = if start + page.len() < 3 {
        Some("c2")
    } else {
        None
    };
    let mut body = json!({ "messages": page });
    if let Some(next) = next {
        body["next_cursor"] = json!(next);
    }
    Json(body).into_response()
}

async fn one(headers: HeaderMap, Path(id): Path<String>) -> Response {
    if let Some(response) = auth(&headers) {
        return response;
    }
    if id == "missing" {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "ERR_NOT_FOUND", "message": "no such message: missing"})),
        )
            .into_response();
    }
    Json(message(&id, "Build 1 failed", &"full body ".repeat(10))).into_response()
}

async fn read(headers: HeaderMap, Path(id): Path<String>) -> Response {
    if let Some(response) = auth(&headers) {
        return response;
    }
    if id == "locked" {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"code": "ERR_SCOPE", "message": "this API key has no write scope"})),
        )
            .into_response();
    }
    let mut message = message(&id, "Build 1 failed", "body");
    message["read_at"] = json!("2026-11-14T11:00:00Z");
    Json(message).into_response()
}

async fn sources(headers: HeaderMap) -> Response {
    if let Some(response) = auth(&headers) {
        return response;
    }
    Json(json!({"sources": [
        {"id": "src_mail", "kind": "mail", "name": "Work mail", "enabled": true, "min_priority": 0},
        {"id": "src_ci", "kind": "webhook", "name": "GitHub Actions", "enabled": true, "min_priority": 4}
    ]}))
    .into_response()
}
