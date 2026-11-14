//! Ingestion front ends: our webhook with the shipped presets and signature check, Gotify and
//! ntfy compatibility, caps, dedup, rate limit. No mail server needed.

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use nsh_mail_server::{Config, build, ingest::hmac_sha256, router};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sha2::Digest;

/// title, body, sender, link, priority, external id
type Row = (
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    Option<String>,
);

fn temp_dir() -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("nsh-mail-ingest-{n}"));
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
    origin: String,
    base: String,
    http: Client,
    data_dir: PathBuf,
}

async fn start(data_dir: PathBuf) -> Server {
    let state = build(config(data_dir.clone())).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    Server {
        origin: format!("http://{addr}"),
        base: format!("http://{addr}/v1"),
        http: Client::new(),
        data_dir,
    }
}

impl Server {
    async fn signup(&self, email: &str) -> String {
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

    /// A source of `kind` with `config`, returning its token and ingest URL.
    async fn source(
        &self,
        session: &str,
        kind: &str,
        config: Value,
        secret: Option<&str>,
    ) -> (String, String) {
        let mut body = json!({ "kind": kind, "name": format!("{kind} test"), "config": config });
        if let Some(secret) = secret {
            body["secret"] = json!(secret);
        }
        let res = self
            .http
            .post(format!("{}/sources", self.base))
            .bearer_auth(session)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body: Value = res.json().await.unwrap();
        (
            body["token"].as_str().unwrap().to_string(),
            body["ingestUrl"].as_str().unwrap().to_string(),
        )
    }

    /// The messages of a source, oldest first, read straight from the database.
    fn feed(&self, source: &str) -> Vec<Row> {
        let db = rusqlite::Connection::open(self.data_dir.join("server.sqlite")).unwrap();
        let mut stmt = db
            .prepare(
                "SELECT title, COALESCE(body_text, body_html, ''), sender, link, priority, external_id
                 FROM feed_messages WHERE source_id = ?1 ORDER BY pk",
            )
            .unwrap();
        stmt.query_map([source], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
    }

    fn source_id_of(&self, token: &str) -> String {
        db_source_id(&self.data_dir, token)
    }

    /// The same path the ingest URL names, on this test server.
    fn at(&self, ingest_url: &str) -> String {
        format!(
            "{}{}",
            self.origin,
            ingest_url
                .strip_prefix("https://mail.test")
                .expect("ingest URL is built from PUBLIC_URL")
        )
    }
}

fn db_source_id(data_dir: &std::path::Path, token: &str) -> String {
    let db = rusqlite::Connection::open(data_dir.join("server.sqlite")).unwrap();
    db.query_row(
        "SELECT id FROM sources WHERE ingest_token_hash = ?1",
        [sha2::Sha256::digest(token.as_bytes()).to_vec()],
        |r| r.get(0),
    )
    .unwrap()
}

/// Every shipped preset, against the payload the service really sends.
#[tokio::test]
async fn presets_map_real_payloads() {
    let server = start(temp_dir()).await;
    let session = server.signup("owner@test.local").await;

    for (fixture, preset, title_has, sender, priority) in [
        (
            include_str!("fixtures/github-issue.json"),
            "github",
            "markbang/nsh-mail: Sync drops messages above 5k",
            "bangwu",
            3,
        ),
        (
            include_str!("fixtures/github-actions.json"),
            "github",
            "markbang/nsh-mail: CI",
            "markbang",
            3,
        ),
        (
            include_str!("fixtures/gitlab-pipeline.json"),
            "gitlab",
            "pipeline: main",
            "Bang Wu",
            3,
        ),
        (
            include_str!("fixtures/sentry-issue-alert.json"),
            "sentry",
            "nsh-mail-web: TypeError: cannot read property 'id' of undefined",
            "nsh-mail-web",
            6,
        ),
        (
            include_str!("fixtures/grafana-alert.json"),
            "grafana",
            "[FIRING:1] HighErrorRate",
            "nsh-mail",
            6,
        ),
        (
            include_str!("fixtures/uptime-kuma.json"),
            "uptime-kuma",
            "[Down] imap.bangwu.me",
            "imap.bangwu.me",
            3,
        ),
    ] {
        let (token, url) = server
            .source(&session, "webhook", json!({ "preset": preset }), None)
            .await;
        let res = server
            .http
            .post(server.at(&url))
            .header("content-type", "application/json")
            .body(fixture)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::ACCEPTED, "{preset}");
        let id = server.source_id_of(&token);
        let feed = server.feed(&id);
        assert_eq!(feed.len(), 1, "{preset}");
        let (title, body, from, link, got_priority, external_id) = &feed[0];
        assert_eq!(title, title_has, "{preset}");
        assert!(!body.is_empty(), "{preset} body");
        assert_eq!(from.as_deref(), Some(sender), "{preset} sender");
        assert!(link.is_some(), "{preset} link");
        assert_eq!(*got_priority, priority, "{preset} priority");
        assert!(external_id.is_some(), "{preset} external id");

        // The sender's retry must not double the feed.
        server
            .http
            .post(server.at(&url))
            .header("content-type", "application/json")
            .body(fixture)
            .send()
            .await
            .unwrap();
        assert_eq!(server.feed(&id).len(), 1, "{preset} dedup");
    }
}

#[tokio::test]
async fn own_mapping_forms_and_html() {
    let dir = temp_dir();
    let server = start(dir).await;
    let session = server.signup("owner@test.local").await;
    let (token, url) = server
        .source(
            &session,
            "webhook",
            json!({ "mapping": {
                "title": "{{service}} {{level}}",
                "body": "$.detail",
                "bodyHtml": "$.html",
                "link": "$.href",
                "priority": "$.level",
                "tags": "$.labels",
                "externalId": "$.key"
            }}),
            None,
        )
        .await;
    let payload = json!({
        "service": "backup",
        "level": "error",
        "detail": "disk full",
        "html": "<p style=\"color:red\">disk full<script>alert(1)</script><img src=\"https://tracker.example/p.gif\"></p>",
        "href": "https://example.com/backups",
        "labels": ["backup", "disk"],
        "key": "backup-1"
    });
    let res = server
        .http
        .post(server.at(&url))
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
    let id = server.source_id_of(&token);
    let feed = server.feed(&id);
    assert_eq!(feed[0].0, "backup error");
    assert_eq!(feed[0].4, 6);
    let db = rusqlite::Connection::open(server.data_dir.join("server.sqlite")).unwrap();
    let (html, tags): (String, String) = db
        .query_row(
            "SELECT body_html, tags FROM feed_messages WHERE source_id = ?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(html.contains("disk full"));
    assert!(!html.contains("script"), "scripts are removed: {html}");
    assert!(
        !html.contains("tracker.example"),
        "remote images are removed: {html}"
    );
    assert_eq!(tags, r#"["backup","disk"]"#);

    // Form-encoded bodies work too, unasked and with ?format=form.
    let (token, url) = server.source(&session, "webhook", json!({}), None).await;
    let res = server
        .http
        .post(server.at(&url))
        .header("content-type", "application/x-www-form-urlencoded")
        .body("title=Disk+space&message=below+5%25")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
    let feed = server.feed(&server.source_id_of(&token));
    assert_eq!(feed[0].0, "Disk space");
    assert!(feed[0].1.contains("below 5%"), "{:?}", feed[0].1);
}

#[tokio::test]
async fn signature_size_cap_rate_limit_and_paused_sources() {
    let dir = temp_dir();
    let server = start(dir).await;
    let session = server.signup("owner@test.local").await;
    let secret = "s3cret";
    let (_, url) = server
        .source(&session, "webhook", json!({}), Some(secret))
        .await;
    let body = br#"{"title":"signed","message":"ok"}"#;

    let unsigned = server
        .http
        .post(server.at(&url))
        .body(body.to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(unsigned.status(), StatusCode::UNAUTHORIZED);
    let wrong = server
        .http
        .post(server.at(&url))
        .header("x-hub-signature-256", format!("sha256={}", "0".repeat(64)))
        .body(body.to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    let signed = server
        .http
        .post(server.at(&url))
        .header(
            "x-hub-signature-256",
            format!(
                "sha256={}",
                hmac_sha256(secret.as_bytes(), body)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ),
        )
        .body(body.to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(signed.status(), StatusCode::ACCEPTED);
    assert_eq!(
        server
            .http
            .post(server.at(&url))
            .header(
                "x-hub-signature-256",
                format!(
                    "sha256={}",
                    hmac_sha256(secret.as_bytes(), b"other")
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                ),
            )
            .body(body.to_vec())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    // Over 256 KiB is refused before parsing.
    let big = vec![b'a'; 300 * 1024];
    assert_eq!(
        server
            .http
            .post(server.at(&url))
            .body(big)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );

    // 60 a minute, 120 at once.
    let (token, url) = server.source(&session, "webhook", json!({}), None).await;
    let mut limited = false;
    for i in 0..140 {
        let status = server
            .http
            .post(server.at(&url))
            .json(&json!({ "title": format!("burst {i}"), "id": format!("burst-{i}") }))
            .send()
            .await
            .unwrap()
            .status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            limited = true;
            break;
        }
        assert_eq!(status, StatusCode::ACCEPTED);
    }
    assert!(limited, "the burst limit never kicked in");
    assert_eq!(server.feed(&server.source_id_of(&token)).len(), 120);

    // Paused: stored nothing, answers 403.
    let id = server.source_id_of(&token);
    server
        .http
        .patch(format!("{}/sources/{id}", server.base))
        .bearer_auth(&session)
        .json(&json!({ "enabled": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        server
            .http
            .post(server.at(&url))
            .json(&json!({ "title": "paused" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(server.feed(&id).len(), 120);
}

#[tokio::test]
async fn gotify_compatible() {
    let dir = temp_dir();
    let server = start(dir).await;
    let session = server.signup("owner@test.local").await;
    let (token, url) = server.source(&session, "gotify", json!({}), None).await;
    assert!(url.starts_with("https://mail.test/message?token="));

    let res = server
        .http
        .post(format!("{}/message?token={token}", server.origin))
        .json(&json!({
            "title": "Backup done",
            "message": "nightly backup finished in 42s",
            "priority": 5,
            "extras": { "client::display": { "contentType": "text/plain" } }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: Value = res.json().await.unwrap();
    assert!(body["id"].is_i64());
    assert_eq!(body["appid"], 1);
    assert_eq!(body["title"], "Backup done");
    assert_eq!(body["message"], "nightly backup finished in 42s");
    assert_eq!(body["priority"], 5);
    assert!(body["date"].as_str().unwrap().ends_with('Z'));

    let feed = server.feed(&server.source_id_of(&token));
    assert_eq!(feed[0].0, "Backup done");
    assert_eq!(feed[0].4, 5);

    // A webhook token on the Gotify endpoint, and a token that does not exist.
    let (webhook, _) = server.source(&session, "webhook", json!({}), None).await;
    assert_eq!(
        server
            .http
            .post(format!("{}/message?token={webhook}", server.origin))
            .json(&json!({ "title": "x" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .http
            .post(format!("{}/message?token=nope", server.origin))
            .json(&json!({ "title": "x" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn ntfy_compatible() {
    let dir = temp_dir();
    let server = start(dir).await;
    let session = server.signup("owner@test.local").await;
    let (topic, url) = server.source(&session, "ntfy", json!({}), None).await;
    assert_eq!(url, format!("https://mail.test/{topic}"));

    let res = server
        .http
        .put(format!("{}/{}", server.origin, topic))
        .header("title", "Deploy")
        .header("priority", "4")
        .header("tags", "rocket,prod")
        .header("click", "https://example.com/deploys/1")
        .body("v1.2.3 is live")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["event"], "message");
    assert_eq!(body["title"], "Deploy");
    assert_eq!(body["message"], "v1.2.3 is live");
    assert_eq!(body["priority"], 4);
    assert_eq!(body["tags"], json!(["rocket", "prod"]));

    let feed = server.feed(&server.source_id_of(&topic));
    assert_eq!(feed[0].0, "Deploy");
    assert_eq!(feed[0].2.as_deref(), None);
    assert_eq!(feed[0].3.as_deref(), Some("https://example.com/deploys/1"));
    assert_eq!(feed[0].4, 8);

    // `GET /{topic}/json` replays from `since=` and then follows new messages (raw TCP: the
    // response stays open, which no buffering client helper gives us).
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let addr = server.origin.strip_prefix("http://").unwrap().to_string();
    let mut socket = tokio::net::TcpStream::connect(&addr).await.unwrap();
    socket
        .write_all(format!("GET /{topic}/json?since=0 HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut text = String::new();
    let mut buf = [0u8; 4096];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !text.contains("Deploy") && tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(3), socket.read(&mut buf)).await {
            Ok(Ok(n)) if n > 0 => text.push_str(&String::from_utf8_lossy(&buf[..n])),
            _ => break,
        }
    }
    assert!(text.contains("\"title\":\"Deploy\""), "{text}");
    server
        .http
        .post(format!("{}/{}", server.origin, topic))
        .header("title", "Live")
        .body("second")
        .send()
        .await
        .unwrap();
    while !text.contains("Live") && tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(3), socket.read(&mut buf)).await {
            Ok(Ok(n)) if n > 0 => text.push_str(&String::from_utf8_lossy(&buf[..n])),
            _ => break,
        }
    }
    assert!(text.contains("\"title\":\"Live\""), "{text}");

    // Unknown topic.
    assert_eq!(
        server
            .http
            .get(format!("{}/nope/json", server.origin))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn presets_endpoint_and_secret_flag() {
    let dir = temp_dir();
    let server = start(dir).await;
    let session = server.signup("owner@test.local").await;
    let presets: Value = server
        .http
        .get(format!("{}/sources/presets", server.base))
        .bearer_auth(&session)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let names: Vec<&str> = presets
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["github", "gitlab", "sentry", "grafana", "uptime-kuma"]
    );

    let (token, _) = server
        .source(&session, "webhook", json!({}), Some("k"))
        .await;
    let id = server.source_id_of(&token);
    let source: Value = server
        .http
        .get(format!("{}/sources/{id}", server.base))
        .bearer_auth(&session)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(source["hasSecret"], true);
    assert!(source.get("secret").is_none());
    assert_eq!(source["hasToken"], true);
}
