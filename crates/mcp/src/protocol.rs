//! JSON-RPC 2.0 / MCP methods, the tool catalogue and tool dispatch.

use serde_json::{Value, json};

use crate::api::{ListQuery, MAX_LIMIT, Message, Notify, Page, Source};

pub const SERVER_NAME: &str = "nsh-notify";
/// Newest first; the server answers with whatever it and we share.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
const LIST_BODY_CHARS: usize = 280;
const GET_BODY_CHARS: usize = 32_000;

const INSTRUCTIONS: &str = "The notification inbox: mail, webhooks, chat bots and push services all \
land in one feed. list_unread shows what is new, search looks across everything, get reads one \
notification in full, list_sources says where messages come from, and mark_read takes a handled \
notification out of the unread list. Prefer list_unread over guessing; ids come only from this server.";

/// The tool list, as `tools/list` returns it. Descriptions are written for an
/// LLM: when to use the tool, what the arguments mean, what comes back.
pub fn tools() -> Value {
    json!([
        {
            "name": "list_unread",
            "description": "List unread notifications, newest first, across every source the API key \
                can see: mail, webhooks (GitHub, GitLab, Sentry, Grafana, Uptime Kuma), Gotify, ntfy and \
                chat bots. Use it to check what is new. Bodies are shortened here; call get with an id \
                for the full text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Only messages from this source, by its id (src_…). Get ids from list_sources."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_LIMIT,
                        "default": 20,
                        "description": "How many messages to return (default 20, at most 50)."
                    }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "search",
            "description": "Search every notification by text — sender, title and body — across all \
                sources, archived ones included, newest first. Use it when the user asks about \
                something rather than for what is new.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "q": {
                        "type": "string",
                        "description": "Words to look for, as the inbox's search field takes them."
                    },
                    "source": {
                        "type": "string",
                        "description": "Restrict the search to one source id (src_…)."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_LIMIT,
                        "default": 20,
                        "description": "How many matches to return (default 20, at most 50)."
                    }
                },
                "required": ["q"],
                "additionalProperties": false
            }
        },
        {
            "name": "get",
            "description": "Read one notification in full by id: sender, title, complete text body, \
                link, tags, priority and timestamps. Get the id from list_unread or search.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The notification's id, as returned by list_unread or search."
                    }
                },
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "mark_read",
            "description": "Mark one notification as read so it leaves the unread list. Needs an API \
                key with the write scope; calling it twice is harmless. Use it once the notification \
                has actually been handled.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The notification's id."
                    }
                },
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "list_sources",
            "description": "List the notification sources this API key can read — id, kind (mail, \
                webhook, gotify, ntfy, telegram, …), name, whether it is enabled and its minimum \
                priority. Use the ids to filter list_unread and search.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }
    ])
}

/// Answers one JSON-RPC message. `None` means nothing is owed (a notification,
/// or a reply from the client).
pub async fn dispatch(backend: &dyn Notify, request: &Value) -> Option<Value> {
    let method = request.get("method").and_then(Value::as_str);
    // A response from the client, or a message without a method: no answer.
    let method = method?;
    let id = request.get("id").cloned()?;
    let params = request.get("params").cloned().unwrap_or(Value::Null);

    let response = match method {
        "initialize" => result(&id, initialize(&params)),
        "ping" => result(&id, json!({})),
        "tools/list" => result(&id, json!({ "tools": tools() })),
        "tools/call" => match call(backend, &params).await {
            Ok(value) => result(&id, value),
            Err(CallError::Invalid(message)) => error(&id, -32602, &message),
            Err(CallError::Failed(message)) => tool_error(&id, &message),
        },
        _ => error(&id, -32601, &format!("method `{method}` is not supported")),
    };
    Some(response)
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|v| PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS
    })
}

enum CallError {
    /// The caller got the request wrong: unknown tool, missing argument.
    Invalid(String),
    /// The tool ran and the inbox answered with an error; the text is the
    /// server's own message.
    Failed(String),
}

/// `tools/call`: the tool's JSON as text, or `isError` with the server's own
/// message so the model can correct itself instead of retrying blindly.
async fn call(backend: &dyn Notify, params: &Value) -> Result<Value, CallError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| CallError::Invalid("tools/call needs a `name`".into()))?;
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    let text = match name {
        "list_unread" => {
            let mut query = ListQuery::unread();
            query.source = arg_string(&args, "source");
            query.limit = arg_limit(&args);
            let page = backend
                .messages(query)
                .await
                .map_err(|e| CallError::Failed(e.to_string()))?;
            page_json(page, LIST_BODY_CHARS, "unread")
        }
        "search" => {
            let q = arg_string(&args, "q")
                .ok_or_else(|| CallError::Invalid("search needs `q`".into()))?;
            let mut query = ListQuery::search(q);
            query.source = arg_string(&args, "source");
            query.limit = arg_limit(&args);
            let page = backend
                .messages(query)
                .await
                .map_err(|e| CallError::Failed(e.to_string()))?;
            page_json(page, LIST_BODY_CHARS, "matching")
        }
        "get" => {
            let id = arg_string(&args, "id")
                .ok_or_else(|| CallError::Invalid("get needs `id`".into()))?;
            let message = backend
                .message(id)
                .await
                .map_err(|e| CallError::Failed(e.to_string()))?;
            clip(message, GET_BODY_CHARS)
        }
        "mark_read" => {
            let id = arg_string(&args, "id")
                .ok_or_else(|| CallError::Invalid("mark_read needs `id`".into()))?;
            let message = backend
                .mark_read(id)
                .await
                .map_err(|e| CallError::Failed(e.to_string()))?;
            let mut value = clip(message, 0);
            value["read"] = json!(true);
            value
        }
        "list_sources" => {
            let sources = backend
                .sources()
                .await
                .map_err(|e| CallError::Failed(e.to_string()))?;
            sources_json(sources)
        }
        other => return Err(CallError::Invalid(format!("unknown tool `{other}`"))),
    };
    Ok(json!({ "content": [{ "type": "text", "text": text.to_string() }] }))
}

fn result(id: &Value, value: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": value })
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// A tool that failed: a normal result carrying `isError`, per the MCP spec.
pub fn tool_error(id: &Value, message: &str) -> Value {
    result(
        id,
        json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
    )
}

fn arg_string(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn arg_limit(args: &Value) -> u32 {
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20);
    limit.clamp(1, MAX_LIMIT as u64) as u32
}

fn page_json(page: Page, body_chars: usize, what: &str) -> Value {
    let count = page.messages.len();
    let messages: Vec<Value> = page
        .messages
        .into_iter()
        .map(|m| clip(m, body_chars))
        .collect();
    let mut out = json!({ "count": count, "messages": messages });
    if let Some(next) = page.next {
        out["next"] = json!(next);
        out["note"] = json!(format!(
            "More {what} notifications match; raise `limit` (at most {MAX_LIMIT}) or handle these first."
        ));
    }
    out
}

fn sources_json(sources: Vec<Source>) -> Value {
    let count = sources.len();
    let list: Vec<Value> = sources
        .into_iter()
        .filter_map(|s| serde_json::to_value(s).ok())
        .collect();
    json!({ "count": count, "sources": list })
}

/// Turns a message into the JSON the model sees; `body_chars` caps the body,
/// marking it when clipped. `0` drops the body (for write receipts).
fn clip(message: Message, body_chars: usize) -> Value {
    let mut value = serde_json::to_value(&message).unwrap_or_else(|_| json!({ "id": message.id }));
    let Some(body) = message.body_text else {
        return value;
    };
    if body_chars == 0 {
        value.as_object_mut().map(|o| o.remove("body_text"));
        return value;
    }
    let mut chars = body.chars();
    let short: String = chars.by_ref().take(body_chars).collect();
    if chars.next().is_some() {
        value["body_text"] = json!(format!("{short}…"));
        value["truncated"] = json!(true);
    }
    value
}
