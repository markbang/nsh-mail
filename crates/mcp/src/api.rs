//! Client for the notification inbox REST API (`/v1`) and the backend trait the
//! tools talk to.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
pub const MAX_LIMIT: u32 = 50;

/// One entry of the unified feed. Every field but `id` is optional so a server
/// that adds or drops a field does not break an installed agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    // The server writes camelCase like the rest of `/v1`; the aliases keep a snake_case server
    // (the shape first written down in the design) readable too.
    #[serde(default, alias = "sourceId", skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, alias = "bodyText", skip_serializing_if = "Option::is_none")]
    pub body_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// Milliseconds or RFC 3339: the tool output passes it through untouched.
    #[serde(default, alias = "receivedAt", skip_serializing_if = "Option::is_none")]
    pub received_at: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Value>,
    #[serde(default, alias = "readAt", skip_serializing_if = "Option::is_none")]
    pub read_at: Option<Value>,
    #[serde(default, alias = "archivedAt", skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<Value>,
    #[serde(default, alias = "threadId", skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(
        default,
        alias = "minPriority",
        skip_serializing_if = "Option::is_none"
    )]
    pub min_priority: Option<i64>,
}

/// One page of the feed. `next` is the opaque cursor for the following page.
#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ListQuery {
    pub unread: bool,
    pub q: Option<String>,
    pub source: Option<String>,
    pub limit: u32,
    pub cursor: Option<String>,
}

impl ListQuery {
    pub fn unread() -> Self {
        Self {
            unread: true,
            q: None,
            source: None,
            limit: 20,
            cursor: None,
        }
    }

    pub fn search(q: impl Into<String>) -> Self {
        Self {
            unread: false,
            q: Some(q.into()),
            source: None,
            limit: 20,
            cursor: None,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    /// No answer at all: DNS, connection, TLS, or the timeout below.
    Timeout(Duration),
    Unreachable(String),
    /// The server answered with a non-2xx status; `message` is its own text.
    Status {
        status: u16,
        code: String,
        message: String,
    },
    Decode(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Timeout(d) => write!(
                f,
                "the server did not answer within {}s; check --server and the network",
                d.as_secs_f32()
            ),
            Error::Unreachable(e) => write!(f, "cannot reach the server: {e}"),
            Error::Status {
                status,
                code,
                message,
            } => {
                let hint = match *status {
                    401 => " — check the API key (nb_…) and that it is not revoked",
                    403 => " — this API key lacks the scope or the source for that action",
                    404 => " — no such message or source",
                    429 => " — rate limited, retry later",
                    _ => "",
                };
                write!(f, "HTTP {status} {code}: {message}{hint}")
            }
            Error::Decode(e) => write!(f, "unexpected answer from the server: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// The tools' view of the inbox: an HTTP client here, the server's own store
/// when the server mounts the router.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'static>>;

pub trait Notify: Send + Sync + 'static {
    fn messages(&self, query: ListQuery) -> BoxFuture<Page>;
    fn message(&self, id: String) -> BoxFuture<Message>;
    fn mark_read(&self, id: String) -> BoxFuture<Message>;
    fn sources(&self) -> BoxFuture<Vec<Source>>;
}

#[derive(Clone)]
pub struct NotifyApi {
    base: String,
    key: String,
    http: reqwest::Client,
    timeout: Duration,
}

impl NotifyApi {
    pub fn new(server: impl Into<String>, key: impl Into<String>) -> Self {
        Self::with_client(server, key, reqwest::Client::new(), DEFAULT_TIMEOUT)
    }

    pub fn with_client(
        server: impl Into<String>,
        key: impl Into<String>,
        http: reqwest::Client,
        timeout: Duration,
    ) -> Self {
        let base = server.into().trim_end_matches('/').to_string();
        Self {
            base,
            key: key.into(),
            http,
            timeout,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
    ) -> Result<T, Error> {
        let text = self.send(req).await?;
        serde_json::from_str(&text).map_err(|e| Error::Decode(e.to_string()))
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<String, Error> {
        let req = req
            .bearer_auth(&self.key)
            .header("accept", "application/json")
            .timeout(self.timeout);
        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                Error::Timeout(self.timeout)
            } else {
                Error::Unreachable(e.to_string())
            }
        })?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| {
            if e.is_timeout() {
                Error::Timeout(self.timeout)
            } else {
                Error::Unreachable(e.to_string())
            }
        })?;
        if status.is_success() {
            return Ok(text);
        }
        let (code, message) = error_body(&text);
        Err(Error::Status {
            status: status.as_u16(),
            code,
            message,
        })
    }

    /// One page of the feed. `cursor` continues from a previous `Page::next`.
    pub async fn list(&self, query: &ListQuery) -> Result<Page, Error> {
        let mut params: Vec<(&str, String)> = Vec::new();
        if query.unread {
            params.push(("unread", "1".to_string()));
        }
        if let Some(q) = &query.q {
            params.push(("q", q.clone()));
        }
        if let Some(source) = &query.source {
            params.push(("source", source.clone()));
        }
        if let Some(cursor) = &query.cursor {
            params.push(("cursor", cursor.clone()));
        }
        params.push(("limit", query.limit.clamp(1, MAX_LIMIT).to_string()));
        let req = self.http.get(self.url("/v1/feed")).query(&params);
        let value: Value = self.request(req).await?;
        page_from_value(value)
    }

    pub async fn get(&self, id: &str) -> Result<Message, Error> {
        let req = self.http.get(self.url(&format!("/v1/feed/{id}")));
        self.request(req).await
    }

    pub async fn mark_read(&self, id: &str) -> Result<Message, Error> {
        let req = self.http.post(self.url(&format!("/v1/feed/{id}/read")));
        let text = self.send(req).await?;
        if text.trim().is_empty() {
            return Ok(Message {
                id: id.to_string(),
                source_id: None,
                sender: None,
                title: None,
                body_text: None,
                link: None,
                received_at: None,
                priority: None,
                tags: Vec::new(),
                read_at: Some(Value::String(now())),
                archived_at: None,
                thread_id: None,
            });
        }
        serde_json::from_str(&text).map_err(|e| Error::Decode(e.to_string()))
    }

    pub async fn sources(&self) -> Result<Vec<Source>, Error> {
        let value: Value = self.request(self.http.get(self.url("/v1/sources"))).await?;
        let items = match value {
            Value::Array(items) => items,
            Value::Object(mut map) => map
                .remove("sources")
                .or_else(|| map.remove("items"))
                .and_then(|v| v.as_array().cloned())
                .ok_or_else(|| Error::Decode("no `sources` array in GET /v1/sources".into()))?,
            _ => return Err(Error::Decode("GET /v1/sources did not return JSON".into())),
        };
        items
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(|e| Error::Decode(e.to_string())))
            .collect()
    }
}

impl Notify for NotifyApi {
    fn messages(&self, query: ListQuery) -> BoxFuture<Page> {
        let this = self.clone();
        Box::pin(async move { this.list(&query).await })
    }

    fn message(&self, id: String) -> BoxFuture<Message> {
        let this = self.clone();
        Box::pin(async move { this.get(&id).await })
    }

    fn mark_read(&self, id: String) -> BoxFuture<Message> {
        let this = self.clone();
        Box::pin(async move { this.mark_read(&id).await })
    }

    fn sources(&self) -> BoxFuture<Vec<Source>> {
        let this = self.clone();
        Box::pin(async move { this.sources().await })
    }
}

/// The server's error shape is `{code, message}`; anything else is passed on as
/// text so a reverse proxy's HTML page still reaches the model.
fn error_body(text: &str) -> (String, String) {
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) {
        let code = map
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("ERR_UNKNOWN")
            .to_string();
        let message = map
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| text.trim().chars().take(300).collect());
        return (code, message);
    }
    let message: String = text.trim().chars().take(300).collect();
    (
        "ERR_UNKNOWN".to_string(),
        if message.is_empty() {
            "no message".to_string()
        } else {
            message
        },
    )
}

/// Accepts `{messages, nextCursor}` (what the server sends), the snake_case
/// spelling, a wrapper with `items`/`next`, and a bare array, so an older server
/// still works.
fn page_from_value(value: Value) -> Result<Page, Error> {
    match value {
        Value::Array(items) => Ok(Page {
            messages: messages_from(items)?,
            next: None,
        }),
        Value::Object(mut map) => {
            let items = map
                .remove("messages")
                .or_else(|| map.remove("items"))
                .and_then(|v| v.as_array().cloned())
                .ok_or_else(|| Error::Decode("no `messages` array in the feed".into()))?;
            let next = ["nextCursor", "next_cursor", "next", "cursor"]
                .iter()
                .find_map(|k| map.get(*k).and_then(Value::as_str))
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            Ok(Page {
                messages: messages_from(items)?,
                next,
            })
        }
        _ => Err(Error::Decode("GET /v1/feed did not return JSON".into())),
    }
}

fn messages_from(items: Vec<Value>) -> Result<Vec<Message>, Error> {
    items
        .into_iter()
        .map(|v| serde_json::from_value(v).map_err(|e| Error::Decode(e.to_string())))
        .collect()
}

fn now() -> String {
    // Seconds since the epoch is enough for the "marked read" receipt; the
    // server's own value replaces it whenever it answers with the message.
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs().to_string(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server writes camelCase like the rest of `/v1`; a snake_case server parses too.
    #[test]
    fn feed_json_in_both_cases() {
        for doc in [
            serde_json::json!({ "id": "msg_1", "sourceId": "src_1", "bodyText": "disk full",
                                "receivedAt": 1_700_000_000_000i64, "readAt": null }),
            serde_json::json!({ "id": "msg_1", "source_id": "src_1", "body_text": "disk full",
                                "received_at": "2026-11-14T12:00:00Z", "read_at": null }),
        ] {
            let message: Message = serde_json::from_value(doc).unwrap();
            assert_eq!(message.source_id.as_deref(), Some("src_1"));
            assert_eq!(message.body_text.as_deref(), Some("disk full"));
            assert!(message.received_at.is_some());
        }
        let source: Source = serde_json::from_value(serde_json::json!({
            "id": "src_1", "kind": "webhook", "name": "CI", "enabled": true, "minPriority": 3
        }))
        .unwrap();
        assert_eq!(source.min_priority, Some(3));
    }
}
