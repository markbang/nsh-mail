//! The agent API end to end: API keys, the feed, SSE and MCP over HTTP. Everything but the mail
//! row lives in `server.sqlite`; the mail test needs GreenMail (IMAPS 3993, SMTPS 3465), like
//! `api.rs`.

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use futures_util::{StreamExt, stream::BoxStream};
use nsh_mail_server::{Config, build, router};
use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

fn temp_dir() -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("nsh-mail-agent-{n}"));
    std::fs::create_dir_all(dir.join("users")).unwrap();
    dir
}

fn config(data_dir: PathBuf) -> Config {
    Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir,
        public_url: "https://mail.test".into(),
        master_key: [7; 32],
        master_key_old: None,
        web_dir: None,
        cors_origins: vec![],
        allow_signup: true,
        oauth: Default::default(),
        fcm_service_account: None,
        fcm_base_url: String::new(),
        vapid_private_key: None,
        vapid_subject: "mailto:test@example.com".into(),
        max_attachments: 25 * 1024 * 1024,
    }
}

struct Server {
    base: String,
    addr: SocketAddr,
    http: Client,
    data_dir: PathBuf,
}

async fn start() -> Server {
    let data_dir = temp_dir();
    let state = build(config(data_dir.clone())).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    Server {
        base: format!("http://{addr}/v1"),
        addr,
        http: Client::new(),
        data_dir,
    }
}

impl Server {
    async fn signup(&self) -> String {
        let email = format!(
            "user{}@test.local",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let res = self
            .http
            .post(format!("{}/auth/signup", self.base))
            .json(&json!({ "email": email, "password": "correct horse" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        res.json::<Value>().await.unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string()
    }

    async fn send(&self, token: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let res = self
            .http
            .post(format!("{}{path}", self.base))
            .bearer_auth(token)
            .json(&body.unwrap_or_else(|| json!({})))
            .send()
            .await
            .unwrap();
        let status = res.status();
        (status, body_of(res).await)
    }

    async fn delete(&self, token: &str, path: &str) -> StatusCode {
        self.http
            .delete(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status()
    }

    async fn get(&self, token: &str, path: &str) -> (StatusCode, Value) {
        let res = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        let status = res.status();
        (status, body_of(res).await)
    }

    /// A webhook source, with its ingest URL's token.
    async fn webhook(&self, token: &str, name: &str) -> String {
        let (status, body) = self
            .send(
                token,
                "/sources",
                Some(json!({ "kind": "webhook", "name": name })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["token"].as_str().unwrap().to_string()
    }

    async fn ingest(&self, token: &str, title: &str, body: &str) {
        let res = self
            .http
            .post(format!("http://{}/hooks/{token}", self.addr))
            .json(&json!({ "title": title, "message": body }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::ACCEPTED);
    }

    /// A key for the whole account (or `sources`), with the token only this call sees.
    async fn key(&self, session: &str, write: bool, sources: &[&str]) -> (String, String) {
        let (status, body) = self
            .send(
                session,
                "/keys",
                Some(json!({ "name": "test agent", "write": write, "sourceIds": sources })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        (
            body["key"]["id"].as_str().unwrap().to_string(),
            body["token"].as_str().unwrap().to_string(),
        )
    }

    fn sql(&self, statement: &str, args: &[&dyn rusqlite::ToSql]) {
        let db = rusqlite::Connection::open(self.data_dir.join("server.sqlite")).unwrap();
        db.execute(statement, args).unwrap();
    }

    fn sql_value<T: rusqlite::types::FromSql>(
        &self,
        statement: &str,
        args: &[&dyn rusqlite::ToSql],
    ) -> T {
        let db = rusqlite::Connection::open(self.data_dir.join("server.sqlite")).unwrap();
        db.query_row(statement, args, |r| r.get(0)).unwrap()
    }
}

/// One MCP request over the mounted transport.
async fn mcp(s: &Server, key: &str, body: Value) -> (StatusCode, Value) {
    let res = s
        .http
        .post(format!("http://{}/mcp", s.addr))
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .unwrap();
    (res.status(), body_of(res).await)
}

async fn body_of(res: reqwest::Response) -> Value {
    let text = res.text().await.unwrap();
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

/// One SSE event, as the wire carries it.
#[derive(Debug)]
struct Sse {
    event: String,
    id: Option<String>,
    data: String,
}

struct Events {
    stream: BoxStream<'static, Result<Vec<u8>, reqwest::Error>>,
    buffer: String,
}

impl Events {
    async fn next(&mut self) -> Option<Sse> {
        loop {
            while let Some(end) = self.buffer.find("\n\n") {
                let block: String = self.buffer.drain(..end + 2).collect();
                let mut event = Sse {
                    event: "message".into(),
                    id: None,
                    data: String::new(),
                };
                let mut data = Vec::new();
                let mut keep_alive = false;
                for line in block.lines() {
                    if line.starts_with(':') {
                        keep_alive = true;
                    } else if let Some(v) = line.strip_prefix("event: ") {
                        event.event = v.to_string();
                    } else if let Some(v) = line.strip_prefix("id: ") {
                        event.id = Some(v.to_string());
                    } else if let Some(v) = line.strip_prefix("data: ") {
                        data.push(v.to_string());
                    }
                }
                event.data = data.join("\n");
                if keep_alive && event.data.is_empty() {
                    return Some(Sse {
                        event: "keep-alive".into(),
                        id: None,
                        data: String::new(),
                    });
                }
                if !event.data.is_empty() {
                    return Some(event);
                }
            }
            let chunk =
                match tokio::time::timeout(Duration::from_secs(30), self.stream.next()).await {
                    Ok(Some(Ok(chunk))) => chunk,
                    _ => return None,
                };
            self.buffer.push_str(&String::from_utf8_lossy(&chunk));
        }
    }

    /// The next real event; keep-alives are skipped.
    async fn next_event(&mut self) -> Sse {
        loop {
            let event = self.next().await.expect("stream ended");
            if event.event != "keep-alive" {
                return event;
            }
        }
    }
}

async fn stream(s: &Server, key: &str, query: &str, last_event_id: Option<&str>) -> Events {
    let mut req = s.http.get(format!("http://{}/v1/stream{query}", s.addr));
    req = req.bearer_auth(key);
    if let Some(id) = last_event_id {
        req = req.header("Last-Event-ID", id);
    }
    let res = req.send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()[header::CONTENT_TYPE],
        "text/event-stream",
        "not an event stream"
    );
    Events {
        stream: Box::pin(res.bytes_stream().map(|chunk| chunk.map(|b| b.to_vec()))),
        buffer: String::new(),
    }
}

#[tokio::test]
async fn keys_narrow_to_their_sources_and_can_be_revoked() {
    let s = start().await;
    let session = s.signup().await;
    let ci = s.webhook(&session, "CI").await;
    let alerts = s.webhook(&session, "Alerts").await;

    // Two sources, one notification each, with the source ids as the feed reports them.
    s.ingest(&ci, "Build failed", "the build failed").await;
    s.ingest(&alerts, "Disk full", "disk /var is full").await;
    let (_, feed) = s.get(&session, "/feed").await;
    let messages = feed["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    let ci_source = messages
        .iter()
        .find(|m| m["title"] == "Build failed")
        .unwrap()["sourceId"]
        .as_str()
        .unwrap()
        .to_string();
    let alert_source = messages.iter().find(|m| m["title"] == "Disk full").unwrap()["sourceId"]
        .as_str()
        .unwrap()
        .to_string();

    let (read_key_id, read_key) = s.key(&session, false, &[&ci_source]).await;
    let (_, keys) = s.get(&session, "/keys").await;
    let listed = &keys["keys"][0];
    assert_eq!(listed["id"], read_key_id.as_str());
    assert_eq!(listed["prefix"], &read_key[..8]);
    assert_eq!(listed["scopes"], json!(["read"]));
    assert!(listed.get("token").is_none(), "the token is shown once");
    assert_eq!(listed["lastUsedAt"], Value::Null);

    // Only its source, and only what it may read.
    let (status, feed) = s.get(&read_key, "/feed").await;
    assert_eq!(status, StatusCode::OK);
    let titles: Vec<&str> = feed["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Build failed"]);
    assert_eq!(feed["unread"], 1);
    assert_eq!(feed["counts"][&ci_source], 1);
    assert!(feed["counts"].get(&alert_source).is_none());
    let (status, _) = s
        .get(&read_key, &format!("/feed?source={alert_source}"))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, all) = s.get(&session, "/feed").await;
    let alert_id = all["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["title"] == "Disk full")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, _) = s.get(&read_key, &format!("/feed/{alert_id}")).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a key must not learn about messages it cannot read"
    );
    let (status, body) = s
        .send(&read_key, &format!("/feed/{alert_id}/read"), None)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // Reading is always allowed; a read-only key cannot mark read.
    let (status, body) = s
        .send(
            &read_key,
            "/feed/read",
            Some(json!({ "ids": [alert_id.clone()] })),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    let (_, write_key) = s.key(&session, true, &[]).await;
    let (status, message) = s
        .send(&write_key, &format!("/feed/{alert_id}/read"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{message}");
    assert!(message["readAt"].as_i64().is_some());
    // The key's last use is recorded, once per minute at most.
    let used: i64 = s.sql_value(
        "SELECT count(*) FROM api_keys WHERE last_used_at IS NOT NULL",
        &[],
    );
    assert_eq!(used, 2, "every key that was used is stamped");
    let first: i64 = s.sql_value(
        "SELECT last_used_at FROM api_keys WHERE id = ?1",
        &[&read_key_id],
    );
    s.get(&read_key, "/feed").await;
    let again: i64 = s.sql_value(
        "SELECT last_used_at FROM api_keys WHERE id = ?1",
        &[&read_key_id],
    );
    assert_eq!(first, again, "a burst of requests is one write");

    // Expiry and revocation are checked per request.
    s.sql(
        "UPDATE api_keys SET expires_at = ?2 WHERE id = ?1",
        &[&read_key_id, &(mail_core::feed::now_ms() - 1)],
    );
    let (status, body) = s.get(&read_key, "/feed").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        body["message"].as_str().unwrap().contains("expired"),
        "{body}"
    );

    let (_, fresh) = s.key(&session, false, &[]).await;
    assert_eq!(
        s.delete(&session, "/keys/nope").await,
        StatusCode::NOT_FOUND
    );
    let (_, keys) = s.get(&session, "/keys").await;
    let id = keys["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["prefix"].as_str() == Some(&fresh[..8]))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        s.delete(&session, &format!("/keys/{id}")).await,
        StatusCode::NO_CONTENT
    );
    let (status, body) = s.get(&fresh, "/feed").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        body["message"].as_str().unwrap().contains("revoked"),
        "{body}"
    );
    assert_eq!(
        s.delete(&session, &format!("/keys/{id}")).await,
        StatusCode::NOT_FOUND
    );

    // A key never reads as a session.
    let (status, _) = s.get("nb_not-a-key", "/feed").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn feed_pages_searches_reads_and_archives() {
    let s = start().await;
    let session = s.signup().await;
    let hook = s.webhook(&session, "CI").await;
    for (title, body) in [
        ("Build failed on main", "the build failed after the merge"),
        ("Deploy done", "production is up"),
        ("Disk full", "disk /var is full"),
        ("Build failed on release", "the release build failed"),
    ] {
        s.ingest(&hook, title, body).await;
    }

    // Newest first, one page at a time.
    let (_, first) = s.get(&session, "/feed?limit=2").await;
    let ids: Vec<&str> = first["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(first["more"], true);
    assert_eq!(first["unread"], 4);
    let cursor = first["nextCursor"].as_str().unwrap().to_string();
    let (_, second) = s
        .get(&session, &format!("/feed?limit=2&cursor={cursor}"))
        .await;
    let second_ids: Vec<&str> = second["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(second["more"], false);
    assert!(second.get("nextCursor").is_none());
    let mut paged = [ids.clone(), second_ids.clone()].concat();
    paged.sort();
    paged.dedup();
    assert_eq!(paged.len(), 4, "paging repeats no message");

    // The same page, asked for by the last id; a bad cursor says so.
    let (_, by_id) = s
        .get(&session, &format!("/feed?limit=2&before={}", ids[1]))
        .await;
    let mut by_id_ids: Vec<&str> = by_id["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    by_id_ids.sort();
    let mut next_ids: Vec<&str> = second_ids.clone();
    next_ids.sort();
    assert_eq!(by_id_ids, next_ids, "the next page, asked for by id");
    let (status, body) = s.get(&session, "/feed?cursor=not-a-cursor").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // Search covers non-mail and mail text.
    let (_, found) = s.get(&session, "/feed?q=build").await;
    assert_eq!(found["messages"].as_array().unwrap().len(), 2);
    let (_, found) = s.get(&session, "/feed?q=disk").await;
    assert_eq!(found["messages"][0]["title"], "Disk full");
    let (_, none) = s.get(&session, "/feed?q=nothingmatchesthis").await;
    assert_eq!(none["messages"], json!([]));

    // `since` takes a message id, ms or RFC 3339.
    let latest = first["messages"][0]["receivedAt"].as_i64().unwrap();
    let newest_id = first["messages"][0]["id"].as_str().unwrap().to_string();
    let (_, after) = s.get(&session, &format!("/feed?since={newest_id}")).await;
    assert_eq!(after["messages"], json!([]), "nothing is newer than itself");
    let (_, after) = s.get(&session, &format!("/feed?since={latest}")).await;
    assert_eq!(after["messages"], json!([]));
    let (status, _) = s.get(&session, "/feed?since=yesterday").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // An RFC 3339 time, with the seconds it was sent in as its granularity.
    let (_, later) = s
        .get(
            &session,
            &format!("/feed?since={}", rfc3339(latest / 1000 + 60).unwrap()),
        )
        .await;
    assert_eq!(later["messages"], json!([]));
    let (_, before_stamp) = s
        .get(
            &session,
            &format!("/feed?since={}", rfc3339(latest / 1000 - 60).unwrap()),
        )
        .await;
    assert_eq!(before_stamp["messages"].as_array().unwrap().len(), 4);

    // Unread, read, and the archived view.
    let (_, unread) = s.get(&session, "/feed?unread=1&limit=1").await;
    let id = unread["messages"][0]["id"].as_str().unwrap().to_string();
    let (status, message) = s.send(&session, &format!("/feed/{id}/read"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(message["readAt"].as_i64().is_some());
    let (_, feed) = s.get(&session, "/feed?unread=1").await;
    assert_eq!(feed["messages"].as_array().unwrap().len(), 3);
    let (status, message) = s
        .send(
            &session,
            &format!("/feed/{id}/read"),
            Some(json!({ "read": false })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(message["readAt"], Value::Null);
    let (_, feed) = s.get(&session, "/feed?unread").await;
    assert_eq!(feed["unread"], 4, "a bare ?unread means true");

    // Batch read and archive, as the app's list does them.
    let all: Vec<String> = feed["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect();
    let (status, _) = s
        .send(
            &session,
            "/feed/read",
            Some(json!({ "ids": all[..2], "read": true })),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, feed) = s.get(&session, "/feed?unread=1").await;
    assert_eq!(feed["messages"].as_array().unwrap().len(), 2);
    let (status, _) = s
        .send(
            &session,
            "/feed/archive",
            Some(json!({ "ids": [all[0].clone()] })),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, feed) = s.get(&session, "/feed").await;
    assert_eq!(feed["messages"].as_array().unwrap().len(), 3);
    let (_, archived) = s.get(&session, "/feed?archived=1").await;
    assert_eq!(archived["messages"].as_array().unwrap().len(), 1);
    assert_eq!(archived["messages"][0]["id"], all[0].as_str());
    let (_, all) = s.get(&session, "/feed?archived=any").await;
    assert_eq!(all["messages"].as_array().unwrap().len(), 4);
    let (status, _) = s
        .send(
            &session,
            "/feed/archive",
            Some(json!({ "ids": vec!["msg_nope"] })),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Keys see the same feed, so an agent and the app cannot disagree.
    let (_, key) = s.key(&session, true, &[]).await;
    let (_, agent) = s.get(&key, "/feed?archived=any").await;
    assert_eq!(agent["messages"].as_array().unwrap().len(), 4);
}

/// RFC 3339 for a Unix second, without a date library.
fn rfc3339(secs: i64) -> Option<String> {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let (hour, minute, second) = (rest / 3_600, rest % 3_600 / 60, rest % 60);
    // Days to civil date, the inverse of Howard Hinnant's algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    ))
}

#[tokio::test(flavor = "multi_thread")]
async fn stream_filters_resumes_and_keeps_alive() {
    let s = start().await;
    let session = s.signup().await;
    let quiet = s.webhook(&session, "Quiet").await;
    let watched = s.webhook(&session, "Watched").await;
    let (_, feed) = s.get(&session, "/feed").await;
    assert_eq!(feed["messages"], json!([]));
    let (_, sources) = s.get(&session, "/sources").await;
    let watched_id = sources["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["name"] == "Watched")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, keep) = s.key(&session, true, &[&watched_id]).await;

    let mut events = stream(&s, &keep, "", None).await;
    s.ingest(&watched, "First", "first body").await;
    let first = events.next_event().await;
    assert_eq!(first.event, "message");
    let data: Value = serde_json::from_str(&first.data).unwrap();
    assert_eq!(data["title"], "First");
    let first_seq = first.id.clone().expect("an event id");
    assert_eq!(data["seq"].to_string(), first_seq);

    // A source outside the key's scope never reaches it.
    s.ingest(&quiet, "Elsewhere", "not mine").await;
    let read = s
        .send(
            &session,
            &format!("/feed/{}/read", data["id"].as_str().unwrap()),
            None,
        )
        .await
        .1;
    assert!(read["readAt"].as_i64().is_some());
    let read_event = events.next_event().await;
    assert_eq!(read_event.event, "read");
    let payload: Value = serde_json::from_str(&read_event.data).unwrap();
    assert_eq!(payload["id"], data["id"]);
    assert!(payload["readAt"].as_i64().is_some());
    drop(events);

    // What arrived while the stream was down is replayed from `Last-Event-ID`.
    s.ingest(&watched, "Second", "second body").await;
    s.ingest(&quiet, "Elsewhere again", "still not mine").await;
    let mut resumed = stream(&s, &keep, "?since=0", Some(&first_seq)).await;
    let replayed = resumed.next_event().await;
    let data: Value = serde_json::from_str(&replayed.data).unwrap();
    assert_eq!(
        data["title"], "Second",
        "the replay starts after Last-Event-ID, for this source only"
    );
    assert_eq!(replayed.event, "message");

    // A cold start can ask for everything with ?since=0.
    let mut all = stream(&s, &keep, "?since=0", None).await;
    assert_eq!(all.next_event().await.event, "message");

    // The WebSocket form sends the same events as JSON, over the same key.
    let mut request = format!("ws://{}/v1/stream?format=ws&since={first_seq}", s.addr)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {keep}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .unwrap_or_else(|e| panic!("websocket: {e}"));
    s.ingest(&watched, "Over ws", "third body").await;
    let change = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let frame = ws.next().await.unwrap().unwrap();
            let Ok(text) = frame.to_text() else { continue };
            let Ok(change) = serde_json::from_str::<Value>(text) else {
                continue;
            };
            if change["message"]["title"] == "Over ws" {
                return change;
            }
        }
    })
    .await
    .expect("no matching WebSocket frame");
    assert_eq!(change["type"], "message");
    assert!(change["message"]["seq"].as_i64().unwrap() > 0);
    assert!(
        change.get("userId").is_none(),
        "the stream never names its owner: {change}"
    );
    ws.close(None).await.ok();

    // Heartbeat: an idle stream still says it is alive every 15 s.
    let mut idle = stream(&s, &keep, "?since=0", None).await;
    let mut saw_keep_alive = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(40);
    while tokio::time::Instant::now() < deadline && !saw_keep_alive {
        match tokio::time::timeout(Duration::from_secs(20), idle.next()).await {
            Ok(Some(event)) => saw_keep_alive = event.event == "keep-alive",
            _ => break,
        }
    }
    assert!(saw_keep_alive, "no keep-alive within 20 s");
}

#[tokio::test]
async fn mcp_answers_tools_over_http_with_a_key() {
    let s = start().await;
    let session = s.signup().await;
    let hook = s.webhook(&session, "CI").await;
    s.ingest(&hook, "Build failed", "the build failed").await;
    let (_, key) = s.key(&session, true, &[]).await;

    let (status, handshake) = mcp(
        &s,
        &key,
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1" }
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{handshake}");
    assert_eq!(handshake["result"]["serverInfo"]["name"], "nsh-notify");

    let (_, tools) = mcp(
        &s,
        &key,
        json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/list"
        }),
    )
    .await;
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"list_unread"));

    let (_, answer) = mcp(
        &s,
        &key,
        json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "list_unread", "arguments": {} }
        }),
    )
    .await;
    let text = answer["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Build failed"), "{answer}");

    let (_, answer) = mcp(
        &s,
        &key,
        json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": { "name": "list_sources", "arguments": {} }
        }),
    )
    .await;
    assert!(
        answer["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("CI"),
        "{answer}"
    );

    // The key is required; a read-only key cannot mark read.
    let res = s
        .http
        .post(format!("http://{}/mcp", s.addr))
        .json(&json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/list" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    let (_, read_only) = s.key(&session, false, &[]).await;
    let res = s
        .http
        .post(format!("http://{}/mcp", s.addr))
        .bearer_auth(&read_only)
        .json(&json!({
            "jsonrpc": "2.0", "id": 6, "method": "tools/call",
            "params": { "name": "mark_read", "arguments": { "id": "msg_nope" } }
        }))
        .send()
        .await
        .unwrap();
    let answer = body_of(res).await;
    assert!(
        answer["result"]["isError"] == true || answer["error"].is_object(),
        "a read-only key is refused: {answer}"
    );
}

#[tokio::test]
async fn openapi_document_is_current() {
    let document = nsh_mail_server::agent::openapi_json();
    let value: Value = serde_json::from_str(&document).unwrap();
    assert!(value["paths"]["/v1/feed"].is_object());
    assert!(value["paths"]["/v1/stream"].is_object());
    assert!(value["paths"]["/v1/keys/{id}"].is_object());
    assert_eq!(
        value["components"]["securitySchemes"]["bearer"]["scheme"],
        "bearer"
    );
    assert_eq!(
        value["components"]["schemas"]["FeedMessage"]["properties"]["receivedAt"]["type"],
        "integer"
    );

    let committed = std::fs::read_to_string("../../docs/openapi.json").unwrap_or_default();
    assert_eq!(
        committed.trim(),
        document.trim(),
        "docs/openapi.json is stale: run `cargo run -p nsh-mail-server -- openapi > docs/openapi.json`"
    );
}

/// A mail notification archived twice: through the agent API (the engine moves the message) and
/// through the app's batch endpoint. Both leave the feed, and both move the mail to Archive.
#[tokio::test(flavor = "multi_thread")]
async fn a_mail_notification_archives_through_both_paths() {
    let s = start().await;
    mail_core::accept_invalid_certs_for_tests();
    let session = s.signup().await;
    let host = std::env::var("GREENMAIL_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let endpoint = |port: u16| json!({ "host": host, "port": port, "security": "tls" });
    let email = format!(
        "mailer{}@test.local",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let res = s
        .http
        .post(format!("{}/accounts", s.base))
        .bearer_auth(&session)
        .json(&json!({ "email": email, "password": "app-password-123",
                       "imap": endpoint(3993), "smtp": endpoint(3465) }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let account = body_of(res).await["id"].as_str().unwrap().to_string();

    for subject in ["Keep me", "Archive me", "Archive me too"] {
        let res = s
            .http
            .post(format!("{}/send", s.base))
            .bearer_auth(&session)
            .json(&json!({ "accountId": account, "subject": subject,
                           "to": [{ "email": email }], "text": "hello" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    // Wait for the three inbox messages; their ids are also their feed ids.
    let mut inbox = Vec::new();
    for _ in 0..60 {
        let (_, list) = s.get(&session, "/messages?folder=inbox").await;
        inbox = list.as_array().cloned().unwrap_or_default();
        if inbox.len() >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let id_of = |subject: &str| -> String {
        inbox
            .iter()
            .find(|m| m["subject"] == subject)
            .unwrap_or_else(|| panic!("no inbox message {subject}: {inbox:?}"))["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let first = id_of("Keep me");
    let second = id_of("Archive me");
    let third = id_of("Archive me too");
    let (_, feed) = s.get(&session, "/feed?archived=any").await;
    let feed_ids: Vec<&str> = feed["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    for id in [&first, &second, &third] {
        assert!(
            feed_ids.contains(&id.as_str()),
            "mail is projected: {feed_ids:?}"
        );
    }

    // A key archives one: the engine moves it to Archive, so the mail itself is archived.
    let (_, key) = s.key(&session, true, &[]).await;
    let (status, body) = s.send(&key, &format!("/feed/{second}/archive"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let (_, feed) = s.get(&session, "/feed?archived=any").await;
    let visible: Vec<&str> = feed["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["archivedAt"].is_null())
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(
        !visible.contains(&second.as_str()),
        "the archived notification leaves the feed: {visible:?}"
    );
    assert!(visible.contains(&first.as_str()));

    // The app's batch endpoint takes another, through the same engine path.
    let (status, _) = s
        .send(
            &session,
            "/feed/archive",
            Some(json!({ "ids": [third.clone()] })),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, feed) = s.get(&session, "/feed").await;
    let still: Vec<&str> = feed["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["sourceId"].as_str() == Some(&format!("src_m_{account}")))
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(
        still.contains(&first.as_str())
            && !still.contains(&second.as_str())
            && !still.contains(&third.as_str()),
        "{still:?}"
    );

    // The mail is in the archive folder of the mailbox, not only hidden in the feed.
    let mut archived = Vec::new();
    for _ in 0..60 {
        let (_, list) = s.get(&session, "/messages?folder=archive").await;
        archived = list.as_array().cloned().unwrap_or_default();
        if archived.len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let subjects: Vec<&str> = archived
        .iter()
        .map(|m| m["subject"].as_str().unwrap())
        .collect();
    assert!(
        subjects.contains(&"Archive me") && subjects.contains(&"Archive me too"),
        "the mail moved to Archive: {subjects:?}"
    );

    // Once re-projected there, they arrive archived instead of as new notifications.
    let mut reprojected = Vec::new();
    for _ in 0..60 {
        let (_, feed) = s.get(&session, "/feed?archived=any").await;
        reprojected = feed["messages"].as_array().cloned().unwrap_or_default();
        if reprojected
            .iter()
            .filter(|m| m["archivedAt"].as_i64().is_some())
            .count()
            >= 2
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let archived_titles: Vec<&str> = reprojected
        .iter()
        .filter(|m| m["archivedAt"].as_i64().is_some())
        .map(|m| m["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        archived_titles.len(),
        2,
        "mail in Archive is projected archived: {archived_titles:?}"
    );
}
