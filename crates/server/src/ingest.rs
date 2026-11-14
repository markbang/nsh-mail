#![allow(clippy::result_large_err)] // a refusal is an axum `Response`; boxing it changes nothing

//! Ingestion: one normalise → sanitize → dedup → insert path behind three compatible front ends
//! (our webhook, Gotify, ntfy). Every front end ends in [`store`], so priority, caps, dedup and
//! the feed event a stream needs are the same for all of them.

use std::{
    collections::HashMap,
    convert::Infallible,
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::{
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::stream;
use mail_core::feed::{FeedMessage, NewFeedMessage, Source, now_ms, sanitize_html};
use mail_core::secrets::SecretStore;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{AppState, crypto, events::FeedEvent};

/// Anything larger is refused before it is parsed: notifications are small.
pub const BODY_LIMIT: usize = 256 * 1024;
pub const RATE_PER_MINUTE: f64 = 60.0;
pub const RATE_BURST: f64 = 120.0;
/// How many messages a stalled ntfy client may miss before it has to catch up from `since=`.
const REPLAY_LIMIT: u32 = 200;

/// A token bucket per source: 60 messages a minute, 120 at once, so a burst of retries from a
/// sender that lost its connection still lands.
#[derive(Default)]
pub struct RateLimits(Mutex<HashMap<String, Bucket>>);

struct Bucket {
    tokens: f64,
    at: Instant,
}

impl RateLimits {
    pub fn check(&self, source_id: &str) -> bool {
        let mut limits = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let bucket = limits.entry(source_id.to_string()).or_insert(Bucket {
            tokens: RATE_BURST,
            at: now,
        });
        let refilled = now.duration_since(bucket.at).as_secs_f64() * RATE_PER_MINUTE / 60.0;
        bucket.tokens = (bucket.tokens + refilled).min(RATE_BURST);
        bucket.at = now;
        if bucket.tokens < 1.0 {
            return false;
        }
        bucket.tokens -= 1.0;
        true
    }
}

/// What a front end turns its request into before the shared insert path.
#[derive(Default)]
struct Normalised {
    message: NewFeedMessage,
}

fn refused(status: StatusCode, message: &str) -> Response {
    (status, axum::Json(json!({ "error": message }))).into_response()
}

/// Resolves the URL token to a live source, and the two answers every front end gives: nothing
/// found for a token that is not ours, and a paused source is not a mistake of the sender.
fn source_for(
    state: &AppState,
    token: &str,
    kind: &str,
    limits: &RateLimits,
) -> Result<Source, Response> {
    let found = state
        .mail_db
        .source_for_token(&crypto::sha256(token.as_bytes()))
        .map_err(|e| refused(StatusCode::INTERNAL_SERVER_ERROR, &e.message))?;
    let Some(source) = found else {
        return Err(refused(StatusCode::UNAUTHORIZED, "unknown token"));
    };
    if source.kind != kind {
        return Err(refused(
            StatusCode::UNAUTHORIZED,
            &format!("this token belongs to a {} source", source.kind),
        ));
    }
    if !source.enabled {
        return Err(refused(StatusCode::FORBIDDEN, "this source is paused"));
    }
    if !limits.check(&source.id) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            axum::Json(json!({ "error": "too many messages; retry in a second" })),
        )
            .into_response());
    }
    Ok(source)
}

/// Stores a normalised message and tells the app about it. Duplicates (the sender's retry) answer
/// the id that is already there.
fn store(state: &AppState, source: &Source, mut n: Normalised) -> Result<FeedMessage, Response> {
    if let Some(html) = &n.message.body_html {
        n.message.body_html = Some(sanitize_html(html));
    }
    let inserted = state
        .mail_db
        .feed(source.user_id)
        .insert(&n.message)
        .map_err(|e| refused(StatusCode::INTERNAL_SERVER_ERROR, &e.message))?;
    let message = inserted.message().clone();
    let _ = state.feed_events.send(FeedEvent::Message {
        message: Box::new(message.clone()),
    });
    Ok(message)
}

#[derive(Deserialize)]
pub struct IngestQuery {
    token: Option<String>,
    format: Option<String>,
    since: Option<i64>,
    title: Option<String>,
    message: Option<String>,
    priority: Option<String>,
    tags: Option<String>,
}

/// `POST /hooks/{token}`: our own webhook, JSON or form, mapped through the source's preset or
/// its own mapping.
pub async fn webhook(
    State(state): State<std::sync::Arc<AppState>>,
    Path(token): Path<String>,
    Query(query): Query<IngestQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match webhook_inner(state, token, query, headers, body).await {
        Ok(message) => (
            StatusCode::ACCEPTED,
            axum::Json(json!({ "id": message.id })),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

async fn webhook_inner(
    state: std::sync::Arc<AppState>,
    token: String,
    query: IngestQuery,
    headers: HeaderMap,
    body: Bytes,
) -> Result<FeedMessage, Response> {
    let source = source_for(&state, &token, "webhook", &state.rate_limits)?;
    verify_signature(&state, &source, &headers, &body)?;
    let doc = parse_body(&headers, &body, query.format.as_deref())
        .map_err(|e| refused(StatusCode::BAD_REQUEST, &e))?;
    let mapping = Mapping::of(&source.config);
    let n = mapping.normalise(&doc, &source);
    store(&state, &source, n)
}

/// GitHub-style `X-Hub-Signature-256` when the source has a secret.
fn verify_signature(
    state: &AppState,
    source: &Source,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), Response> {
    let Ok(Some(secret)) = state.notify_secrets.get(&secret_key(&source.id)) else {
        return Ok(());
    };
    let Some(header) = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
    else {
        return Err(refused(
            StatusCode::UNAUTHORIZED,
            "this source requires an X-Hub-Signature-256 header",
        ));
    };
    let Some(given) = header.strip_prefix("sha256=") else {
        return Err(refused(StatusCode::UNAUTHORIZED, "signature is not sha256"));
    };
    let expected = hmac_sha256(secret.as_bytes(), body);
    let given = given.trim().to_ascii_lowercase();
    let matches = given.len() == expected.len() * 2
        && given
            .as_bytes()
            .chunks(2)
            .zip(&expected)
            .all(|(pair, byte)| {
                u8::from_str_radix(&String::from_utf8_lossy(pair), 16) == Ok(*byte)
            });
    if !matches {
        return Err(refused(
            StatusCode::UNAUTHORIZED,
            "signature does not match the body",
        ));
    }
    Ok(())
}

/// Gotify's `POST /message?token=`: its JSON in, its JSON out.
pub async fn gotify(
    State(state): State<std::sync::Arc<AppState>>,
    Query(query): Query<IngestQuery>,
    body: Bytes,
) -> Response {
    match gotify_inner(state, query, body).await {
        Ok(message) => {
            let date = rfc3339(message.received_at);
            let body = json!({
                "id": message.seq,
                "appid": 1,
                "message": message.body_text.unwrap_or_default(),
                "title": message.title,
                "priority": message.priority,
                "date": date,
            });
            (StatusCode::OK, axum::Json(body)).into_response()
        }
        Err(e) => e.into_response(),
    }
}

async fn gotify_inner(
    state: std::sync::Arc<AppState>,
    query: IngestQuery,
    body: Bytes,
) -> Result<FeedMessage, Response> {
    let token = query
        .token
        .as_deref()
        .ok_or_else(|| refused(StatusCode::UNAUTHORIZED, "missing token"))?;
    let source = source_for(&state, token, "gotify", &state.rate_limits)?;
    let doc: Value = serde_json::from_slice(&body)
        .map_err(|e| refused(StatusCode::BAD_REQUEST, &format!("body is not JSON: {e}")))?;
    let text = |key: &str| doc.get(key).and_then(Value::as_str).map(str::to_string);
    let mut message = NewFeedMessage {
        source_id: source.id.clone(),
        title: text("title").unwrap_or_else(|| "(no title)".into()),
        body_text: text("message"),
        priority: doc
            .get("priority")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, 10),
        received_at: now_ms(),
        payload: Some(body.slice(..body.len().min(64 * 1024)).to_vec()),
        ..Default::default()
    };
    if let Some(html) = doc
        .get("extras")
        .and_then(|e| e.get("client::display"))
        .and_then(|d| d.get("contentType"))
        .and_then(Value::as_str)
        .filter(|c| c.contains("html"))
    {
        let _ = html;
        message.body_html = text("message");
        message.body_text = None;
    }
    store(&state, &source, Normalised { message })
}

/// `GET /{topic}`: ntfy has no such endpoint, and the web client's files sit at the same level
/// (`/app.<hash>.js`, `/nsh.<hash>.wasm`), so a GET here serves the site. The topic route would
/// otherwise answer 405 to every asset request and blank the page.
pub async fn web_asset(
    State(state): State<std::sync::Arc<AppState>>,
    Path(topic): Path<String>,
) -> Response {
    let Some(dir) = &state.config.web_dir else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    if topic.is_empty() || topic.starts_with('.') || topic.contains(['/', '\\']) {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let Ok(bytes) = tokio::fs::read(dir.join(&topic)).await else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    let cache = if topic.ends_with(".html") {
        "no-cache"
    } else {
        "public, max-age=3600"
    };
    (
        [
            (header::CONTENT_TYPE, content_type(&topic)),
            (header::CACHE_CONTROL, cache),
        ],
        bytes,
    )
        .into_response()
}

/// Enough for the web build's files; anything else is served as bytes.
fn content_type(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "webmanifest" => "application/json",
        "wasm" => "application/wasm",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" | "map" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// ntfy's `PUT|POST /{topic}`, the topic being the source token.
pub async fn ntfy_publish(
    State(state): State<std::sync::Arc<AppState>>,
    Path(topic): Path<String>,
    Query(query): Query<IngestQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match ntfy_inner(state, topic, query, headers, body).await {
        Ok(message) => {
            let body = json!({
                "id": message.id,
                "time": message.received_at / 1000,
                "event": "message",
                "topic": "",
                "message": message.body_text.unwrap_or_default(),
                "title": message.title,
                "priority": ntfy_priority(message.priority),
                "tags": message.tags,
            });
            (StatusCode::OK, axum::Json(body)).into_response()
        }
        Err(e) => e.into_response(),
    }
}

async fn ntfy_inner(
    state: std::sync::Arc<AppState>,
    topic: String,
    query: IngestQuery,
    headers: HeaderMap,
    body: Bytes,
) -> Result<FeedMessage, Response> {
    let source = source_for(&state, &topic, "ntfy", &state.rate_limits)?;
    let text = String::from_utf8_lossy(&body).trim().to_string();
    // ntfy clients may publish JSON instead of plain text.
    let json: Option<Value> = serde_json::from_str(&text).ok().filter(Value::is_object);
    let from_json = |key: &str| {
        json.as_ref()
            .and_then(|j| j.get(key))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let message_text = query
        .message
        .clone()
        .or_else(|| header_opt(&headers, "x-message"))
        .or_else(|| from_json("message"))
        .unwrap_or_else(|| text.clone());
    let message = NewFeedMessage {
        source_id: source.id.clone(),
        title: query
            .title
            .clone()
            .or_else(|| header_opt(&headers, "x-title"))
            .or_else(|| header_opt(&headers, "title"))
            .or_else(|| from_json("title"))
            .unwrap_or_else(|| "Notification".into()),
        body_text: Some(message_text),
        link: header_opt(&headers, "click").or_else(|| from_json("click")),
        priority: query
            .priority
            .clone()
            .or_else(|| header_opt(&headers, "priority"))
            .and_then(|p| p.parse::<f64>().ok())
            .map(|p| {
                if p <= 5.0 {
                    ntfy_to_priority(p as i64)
                } else {
                    p.round() as i64
                }
            })
            .unwrap_or(2),
        tags: query
            .tags
            .clone()
            .or_else(|| header_opt(&headers, "tags"))
            .map(|t| {
                t.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        received_at: now_ms(),
        payload: Some(body.slice(..body.len().min(64 * 1024)).to_vec()),
        ..Default::default()
    };
    store(&state, &source, Normalised { message })
}

fn header_opt(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// `GET /{topic}/json`: ntfy's newline-delimited JSON, catching up from `since=` and then
/// following the feed, per source.
pub async fn ntfy_stream(
    State(state): State<std::sync::Arc<AppState>>,
    Path(topic): Path<String>,
    Query(query): Query<IngestQuery>,
) -> Response {
    let source = match source_for(&state, &topic, "ntfy", &state.rate_limits) {
        Ok(source) => source,
        Err(response) => return response,
    };
    let replay = match state.mail_db.feed(source.user_id).since(
        Some(&source.id),
        query.since.unwrap_or(0),
        REPLAY_LIMIT,
    ) {
        Ok(messages) => messages,
        Err(e) => return refused(StatusCode::INTERNAL_SERVER_ERROR, &e.message),
    };
    let rx = state.feed_events.subscribe();
    let pending: std::collections::VecDeque<String> = replay.iter().filter_map(ndjson).collect();
    let source_id = source.id.clone();
    let lines = stream::unfold((pending, rx), move |(mut pending, mut rx)| {
        let source_id = source_id.clone();
        async move {
            loop {
                if let Some(line) = pending.pop_front() {
                    return Some((Ok::<_, Infallible>(Bytes::from(line)), (pending, rx)));
                }
                match tokio::time::timeout(Duration::from_secs(30), rx.recv()).await {
                    Ok(Ok(FeedEvent::Message { message })) if message.source_id == source_id => {
                        if let Some(line) = ndjson(&message) {
                            return Some((Ok(Bytes::from(line)), (pending, rx)));
                        }
                    }
                    Ok(Ok(_)) => continue,
                    // A client that fell behind reconnects with `since=` and catches up.
                    Ok(Err(_)) => return None,
                    Err(_) => return Some((Ok(Bytes::from("\n")), (pending, rx))),
                }
            }
        }
    });
    (
        [
            (header::CONTENT_TYPE, "application/x-ndjson"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(lines),
    )
        .into_response()
}

fn ndjson(m: &FeedMessage) -> Option<String> {
    let line = json!({
        "id": m.id,
        "time": m.received_at / 1000,
        "event": "message",
        "topic": "",
        "message": m.body_text.clone().unwrap_or_default(),
        "title": m.title,
        "priority": ntfy_priority(m.priority),
        "tags": m.tags,
    })
    .to_string();
    Some(format!("{line}\n"))
}

fn ntfy_priority(priority: i64) -> i64 {
    ((priority as f64 / 2.0).round() as i64).clamp(1, 5)
}

fn ntfy_to_priority(ntfy: i64) -> i64 {
    ntfy.clamp(1, 5) * 2
}

/// One field of a source's mapping: JSON path, `a || b` fallback chain, or `{{ template }}`.
struct Mapping(Value);

impl Mapping {
    fn of(config: &Value) -> Self {
        let preset = config.get("preset").and_then(Value::as_str);
        let own = config.get("mapping").cloned().unwrap_or(Value::Null);
        let base = preset.and_then(presets::get).unwrap_or(Value::Null);
        Mapping(match (base, own) {
            (Value::Object(mut base), Value::Object(own)) => {
                for (k, v) in own {
                    base.insert(k, v);
                }
                Value::Object(base)
            }
            (Value::Object(base), _) => Value::Object(base),
            (_, own) => own,
        })
    }

    fn field(&self, name: &str) -> Option<&str> {
        self.0.get(name)?.as_str()
    }

    fn normalise(&self, doc: &Value, source: &Source) -> Normalised {
        let text = |name: &str| self.field(name).and_then(|e| resolve(e, doc));
        let node = |name: &str| self.field(name).and_then(|e| resolve_node(e, doc));
        let title = text("title")
            .or_else(|| defaults::title(doc))
            .unwrap_or_else(|| "(no title)".into());
        let body_text = text("body").or_else(|| defaults::body(doc));
        let body_html = text("bodyHtml");
        let mut priority = node("priority")
            .map(|v| priority_of(&v))
            .or_else(|| defaults::priority(doc))
            .unwrap_or(0);
        priority = priority.clamp(0, 10);
        let tags = node("tags").map(|v| tags_of(&v)).unwrap_or_default();
        Normalised {
            message: NewFeedMessage {
                source_id: source.id.clone(),
                external_id: text("externalId").or_else(|| defaults::external_id(doc)),
                sender: text("sender").or_else(|| defaults::sender(doc)),
                title,
                body_text,
                body_html,
                body_markdown: None,
                link: text("link").or_else(|| defaults::link(doc)),
                received_at: now_ms(),
                priority,
                tags,
                payload: Some(
                    doc.to_string()
                        .into_bytes()
                        .into_iter()
                        .take(mail_core::feed::PAYLOAD_LIMIT)
                        .collect(),
                ),
            },
        }
    }
}

mod defaults {
    use serde_json::Value;

    pub fn first(doc: &Value, keys: &[&str]) -> Option<String> {
        keys.iter()
            .find_map(|k| doc.get(*k).and_then(text_of))
            .filter(|s| !s.trim().is_empty())
    }

    pub fn title(doc: &Value) -> Option<String> {
        first(doc, &["title", "subject", "message", "text", "name"])
    }

    pub fn body(doc: &Value) -> Option<String> {
        let mut rest = doc.clone();
        if let Value::Object(map) = &mut rest {
            map.remove("title");
            map.remove("subject");
        }
        let pretty = serde_json::to_string_pretty(&rest).ok()?;
        (!pretty.trim().is_empty() && pretty != "{}").then_some(pretty)
    }

    pub fn sender(doc: &Value) -> Option<String> {
        [
            "sender",
            "from",
            "author",
            "app",
            "service",
            "user",
            "repository",
        ]
        .iter()
        .find_map(|k| doc.get(*k).and_then(text_of))
    }

    pub fn link(doc: &Value) -> Option<String> {
        first(doc, &["url", "link", "html_url", "permalink"])
    }

    pub fn external_id(doc: &Value) -> Option<String> {
        first(doc, &["id", "event_id", "guid", "uuid", "fingerprint"])
    }

    pub fn priority(doc: &Value) -> Option<i64> {
        doc.get("priority")
            .and_then(Value::as_i64)
            .or_else(|| doc.get("level").and_then(priority_of_level))
            .or_else(|| doc.get("severity").and_then(priority_of_level))
    }

    pub fn priority_of_level(v: &Value) -> Option<i64> {
        match v.as_str()?.to_ascii_lowercase().as_str() {
            "fatal" | "critical" => Some(8),
            "error" | "alerting" => Some(6),
            "warning" | "warn" => Some(4),
            "info" | "notice" | "ok" | "resolved" => Some(2),
            "debug" | "trace" => Some(0),
            _ => None,
        }
    }

    /// Text of a node: strings as they are, numbers and bools rendered, objects/arrays as JSON.
    pub fn text_of(v: &Value) -> Option<String> {
        match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            Value::Null => None,
            other => serde_json::to_string(other).ok(),
        }
    }
}

fn priority_of(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => s
            .trim()
            .parse::<i64>()
            .ok()
            .or_else(|| defaults::priority_of_level(v))
            .unwrap_or(0),
        _ => 0,
    }
}

fn tags_of(v: &Value) -> Vec<String> {
    match v {
        Value::Array(items) => items
            .iter()
            .filter_map(defaults::text_of)
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect(),
        other => defaults::text_of(other)
            .map(|s| {
                s.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// A field expression: `a || b` takes the first that resolves, `{{ path }}` interpolates, anything
/// else is a literal. Paths are JSON paths without filters: `.a.b[0].c`, `$['a']`.
fn resolve(expr: &str, doc: &Value) -> Option<String> {
    resolve_node(expr, doc)
        .and_then(|v| defaults::text_of(&v))
        .filter(|s| !s.trim().is_empty())
}

pub fn resolve_node(expr: &str, doc: &Value) -> Option<Value> {
    let expr = expr.trim();
    if expr.contains("{{") {
        return Some(Value::String(interpolate(expr, doc)));
    }
    let candidates = split_fallbacks(expr);
    if candidates.len() > 1 {
        return candidates
            .into_iter()
            .find_map(|c| resolve_node(c, doc).filter(|v| !is_empty(v)));
    }
    if looks_like_path(expr) && !expr.parse::<f64>().is_ok() && expr != "true" && expr != "false" {
        path_get(expr, doc)
    } else {
        Some(Value::String(expr.to_string()))
    }
}

fn interpolate(template: &str, doc: &Value) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(after);
            return out;
        };
        out.push_str(&resolve(&after[..end], doc).unwrap_or_default());
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// `a || b || c`, split outside `{{ }}`.
fn split_fallbacks(expr: &str) -> Vec<&str> {
    let bytes = expr.as_bytes();
    let (mut parts, mut depth, mut start, mut i) = (Vec::new(), 0usize, 0usize, 0usize);
    while i < bytes.len() {
        if expr[i..].starts_with("{{") {
            depth += 1;
            i += 2;
        } else if expr[i..].starts_with("}}") {
            depth = depth.saturating_sub(1);
            i += 2;
        } else if depth == 0 && expr[i..].starts_with("||") {
            parts.push(&expr[start..i]);
            i += 2;
            start = i;
        } else {
            i += 1;
        }
    }
    parts.push(&expr[start..]);
    parts
}

fn is_empty(v: &Value) -> bool {
    matches!(v, Value::Null) || v.as_str().is_some_and(|s| s.trim().is_empty())
}

fn looks_like_path(s: &str) -> bool {
    let bare = s.strip_prefix('$').unwrap_or(s).trim_start_matches('.');
    !bare.is_empty()
        && !s.contains(' ')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.[]'\"$-".contains(c))
        && bare
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
}

fn path_get(expr: &str, doc: &Value) -> Option<Value> {
    let (mut node, mut rest) = (doc, expr.trim());
    if let Some(stripped) = rest.strip_prefix('$') {
        rest = stripped;
    }
    rest = rest.trim_start_matches('.');
    while !rest.is_empty() {
        if let Some(key) = rest.strip_prefix('[') {
            let end = key.find(']')?;
            let key = key[..end].trim().trim_matches(['\'', '"']);
            node = match key.parse::<usize>() {
                Ok(index) => node.get(index)?,
                Err(_) => node.get(key)?,
            };
            rest = key[end..].trim_start_matches(']').trim_start_matches('.');
            continue;
        }
        let (key, remaining) = match rest.find(['.', '[']) {
            Some(at) => (&rest[..at], &rest[at..]),
            None => (rest, ""),
        };
        node = node.get(key.trim())?;
        rest = remaining.trim_start_matches('.');
    }
    Some(node.clone())
}

/// `text=…` bodies, as some webhooks send, become a flat object.
fn form_value(body: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(body).map_err(|_| "body is not UTF-8".to_string())?;
    let mut map = serde_json::Map::new();
    for pair in text.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(percent_decode(key), Value::String(percent_decode(value)));
    }
    Ok(Value::Object(map))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(byte) => {
                    out.push(byte);
                    i += 3;
                }
                Err(_) => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_body(headers: &HeaderMap, body: &[u8], format: Option<&str>) -> Result<Value, String> {
    let kind = format
        .map(|f| f.to_ascii_lowercase())
        .or_else(|| {
            headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_ascii_lowercase())
        })
        .unwrap_or_default();
    if kind.contains("form") {
        return form_value(body);
    }
    if body.is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    serde_json::from_slice(body).map_err(|e| format!("body is neither JSON nor a form: {e}"))
}

pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner);
    outer.finalize().into()
}

/// RFC 3339 from milliseconds, for Gotify's `date`.
fn rfc3339(ms: i64) -> String {
    let seconds = ms.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let time = seconds.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant's algorithm), valid for any date this database holds.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

pub(crate) fn secret_key(source_id: &str) -> String {
    format!("source/{source_id}")
}

pub mod presets {
    use serde_json::{Value, json};

    /// Shipped mappings, editable per source: the settings UI lists them and copies one into a
    /// source's `config.mapping`.
    pub const ALL: [(&str, &str); 5] = [
        ("github", "GitHub"),
        ("gitlab", "GitLab"),
        ("sentry", "Sentry"),
        ("grafana", "Grafana"),
        ("uptime-kuma", "Uptime Kuma"),
    ];

    pub fn list() -> Value {
        Value::Array(
            ALL.iter()
                .map(|(name, label)| {
                    let mut preset = get(name).unwrap_or(Value::Null);
                    preset["name"] = json!(name);
                    preset["label"] = json!(label);
                    preset
                })
                .collect(),
        )
    }

    pub fn get(name: &str) -> Option<Value> {
        let mapping = match name {
            "github" => json!({
                "title": "{{repository.full_name}}: {{issue.title || pull_request.title || release.name || workflow_run.name || head_commit.message || action}}",
                "body": "{{issue.body || pull_request.body || release.body || workflow_run.conclusion || repository.description || action}}",
                "sender": "$.sender.login || $.pusher.name",
                "link": "$.issue.html_url || $.pull_request.html_url || $.release.html_url || $.workflow_run.html_url || $.repository.html_url",
                "externalId": "$.issue.id || $.pull_request.id || $.release.id || $.workflow_run.id || $.after",
                "priority": "3",
                "tags": "$.action"
            }),
            "gitlab" => json!({
                "title": "{{object_kind}}: {{object_attributes.name || object_attributes.title || project.name}}",
                "body": "$.object_attributes.description || $.object_attributes.status || $.message",
                "sender": "$.user.name || $.user.username",
                "link": "$.object_attributes.url || $.project.web_url",
                "externalId": "$.object_attributes.id || $.object_kind",
                "priority": "3",
                "tags": "$.object_kind"
            }),
            "sentry" => json!({
                "title": "{{project_name}}: {{data.issue.title || issue.title || title}}",
                "body": "$.data.issue.culprit || $.message",
                "sender": "$.data.issue.project.name || $.project_name",
                "link": "$.data.issue.permalink || $.url",
                "externalId": "$.data.issue.id || $.id",
                "priority": "$.level",
                "tags": "$.data.issue.project.slug || $.project_slug"
            }),
            "grafana" => json!({
                "title": "$.title || $.ruleName",
                "body": "$.message || $.state",
                "sender": "$.receiver || $.ruleName",
                "link": "$.dashboardURL || $.panelURL || $.url",
                "externalId": "$.fingerprint",
                "priority": "$.state",
                "tags": "$.state"
            }),
            "uptime-kuma" => json!({
                "title": "$.msg || $.title",
                "body": "$.heartbeat.msg || $.monitor.name",
                "sender": "$.monitor.name",
                "link": "$.monitor.url",
                "externalId": "$.heartbeat.msg || $.monitor.name",
                "priority": "3",
                "tags": "$.monitor.name"
            }),
            _ => return None,
        };
        Some(mapping)
    }
}
