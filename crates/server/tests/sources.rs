//! Sources API and the mail projection migration. No mail server needed: the feed is built from
//! `server.sqlite` alone.

use std::{net::SocketAddr, path::PathBuf};

use nsh_mail_server::{Config, build, crypto::sha256, router};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

fn temp_dir() -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("nsh-mail-sources-{n}"));
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
    http: Client,
    data_dir: PathBuf,
}

async fn start(data_dir: PathBuf) -> Server {
    let state = build(config(data_dir.clone())).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    Server {
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

    async fn send(
        &self,
        method: &str,
        token: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = match method {
            "POST" => self.http.post(format!("{}{path}", self.base)),
            "PATCH" => self.http.patch(format!("{}{path}", self.base)),
            "DELETE" => self.http.delete(format!("{}{path}", self.base)),
            _ => self.http.get(format!("{}{path}", self.base)),
        }
        .bearer_auth(token);
        if let Some(body) = body {
            req = req.json(&body);
        }
        let res = req.send().await.unwrap();
        let status = res.status();
        let text = res.text().await.unwrap();
        let value = serde_json::from_str(&text).unwrap_or(Value::Null);
        (status, value)
    }

    /// The token behind a source, as the ingestion path finds it.
    fn token_known(&self, token: &str) -> bool {
        let db = rusqlite::Connection::open(self.data_dir.join("server.sqlite")).unwrap();
        let n: i64 = db
            .query_row(
                "SELECT count(*) FROM sources WHERE ingest_token_hash = ?1",
                [sha256(token.as_bytes())],
                |r| r.get(0),
            )
            .unwrap();
        n == 1
    }

    fn raw_token_stored(&self, token: &str) -> bool {
        let db = rusqlite::Connection::open(self.data_dir.join("server.sqlite")).unwrap();
        let n: i64 = db
            .query_row(
                "SELECT count(*) FROM sources WHERE instr(CAST(ingest_token_hash AS TEXT), ?1) > 0",
                [token],
                |r| r.get(0),
            )
            .unwrap();
        n > 0
    }
}

#[tokio::test]
async fn sources_crud_tokens_and_test_message() {
    let dir = temp_dir();
    let server = start(dir).await;
    let alice = server.signup("alice@test.local").await;
    let bob = server.signup("bob@test.local").await;

    let (status, body) = server
        .send(
            "POST",
            &alice,
            "/sources",
            Some(json!({ "kind": "webhook", "name": "CI", "icon": "github", "config": { "preset": "github" } })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let source = &body["source"];
    let id = source["id"].as_str().unwrap().to_string();
    let token = body["token"].as_str().unwrap().to_string();
    assert_eq!(source["kind"], "webhook");
    assert_eq!(source["hasToken"], true);
    assert_eq!(
        body["ingestUrl"],
        format!("https://mail.test/hooks/{token}")
    );
    assert!(server.token_known(&token));
    assert!(!server.raw_token_stored(&token), "only the hash is stored");

    // Someone else's session sees nothing and cannot touch it.
    let (_, list) = server.send("GET", &bob, "/sources", None).await;
    assert_eq!(list["sources"], json!([]));
    let (status, _) = server
        .send(
            "PATCH",
            &bob,
            &format!("/sources/{id}"),
            Some(json!({ "name": "x" })),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, list) = server.send("GET", &alice, "/sources", None).await;
    assert_eq!(list["sources"].as_array().unwrap().len(), 1);
    assert!(list["sources"][0].get("token").is_none());

    let (status, updated) = server
        .send(
            "PATCH",
            &alice,
            &format!("/sources/{id}"),
            Some(json!({ "name": "Builds", "minPriority": 3, "enabled": false })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (
            updated["name"].as_str(),
            updated["minPriority"].as_i64(),
            updated["enabled"].as_bool()
        ),
        (Some("Builds"), Some(3), Some(false))
    );

    // A test message goes through the same insert path as real ingestion.
    let (status, body) = server
        .send("POST", &alice, &format!("/sources/{id}/test"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"]["sourceId"], id);
    assert_eq!(body["message"]["title"], "Test message from Builds");

    // Rotation: a new token works, the old one is gone.
    let (status, body) = server
        .send("POST", &alice, &format!("/sources/{id}/token"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let rotated = body["token"].as_str().unwrap().to_string();
    assert_ne!(rotated, token);
    assert!(server.token_known(&rotated));
    assert!(!server.token_known(&token));

    // Mail sources follow their account; an unknown kind is refused.
    let (status, body) = server
        .send(
            "POST",
            &alice,
            "/sources",
            Some(json!({ "kind": "mail", "name": "mail" })),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, _) = server
        .send(
            "POST",
            &alice,
            "/sources",
            Some(json!({ "kind": "carrier-pigeon", "name": "x" })),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, _) = server
        .send("DELETE", &alice, &format!("/sources/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = server.send("GET", &alice, "/sources", None).await;
    assert_eq!(list["sources"], json!([]));
    // Its messages go with it.
    let db = rusqlite::Connection::open(server.data_dir.join("server.sqlite")).unwrap();
    let n: i64 = db
        .query_row("SELECT count(*) FROM feed_messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

/// A `server.sqlite` written before the feed existed migrates at start: one mail source per
/// account, one feed row per mail message, and `rebuild-feed` reproduces exactly what is there.
#[tokio::test]
async fn migration_projects_mail_written_before_the_feed() {
    let dir = temp_dir();
    let path = dir.join("server.sqlite");
    let account = json!({
        "id": "acct1", "email": "inbox@test.local", "username": "inbox@test.local",
        "auth": { "kind": "password" },
        "imap": { "host": "127.0.0.1", "port": 993, "security": "tls" },
        "smtp": { "host": "127.0.0.1", "port": 465, "security": "tls" }
    });
    {
        let db = nsh_mail_server::db::Db::open(&path).unwrap();
        let user = db.create_user("owner@test.local", "x").unwrap().unwrap();
        let some = nsh_mail_server::db::Db::open(&path).unwrap();
        some.create_session(&sha256(b"session-token"), user)
            .unwrap();
        let mail = mail_core::MailDb::open(&path).unwrap();
        drop(mail);
        // Strip the feed, as the server that wrote this database never had it.
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "DROP TRIGGER messages_feed_insert;
             DROP TRIGGER messages_feed_seen;
             DROP TRIGGER messages_feed_delete;
             DROP TRIGGER accounts_source_insert;
             DROP TRIGGER accounts_source_update;
             DROP TRIGGER accounts_source_delete;
             DROP TRIGGER feed_messages_fts_delete;
             DROP TABLE feed_messages_fts;
             DROP TABLE feed_messages;
             DROP TABLE feed_meta;
             DROP TABLE sources;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (id, user_id, position, data) VALUES ('acct1', ?1, 1, ?2)",
            rusqlite::params![user, account.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO folders (id, user_id, account_id, kind, name) VALUES (10, ?1, 'acct1', 'inbox', 'INBOX')",
            [user],
        )
        .unwrap();
        for (uid, seen, subject) in [(1u32, false, "Deploy failed"), (2, true, "Daily digest")] {
            conn.execute(
                "INSERT INTO messages (user_id, id, folder_id, uid, date, from_addr, to_addrs, cc_addrs,
                                       subject, snippet, refs, seen, flagged, has_attachments, size)
                 VALUES (?1, ?2, 10, ?3, ?4, ?5, '[]', '[]', ?6, 'text', '[]', ?7, 0, 0, 10)",
                rusqlite::params![
                    user,
                    format!("m{uid}"),
                    uid,
                    1_700_000_000_000i64 + uid as i64,
                    json!({ "name": "CI", "email": "ci@test.local" }).to_string(),
                    subject,
                    seen
                ],
            )
            .unwrap();
        }
    }

    let server = start(dir.clone()).await;
    let (status, list) = server.send("GET", "session-token", "/sources", None).await;
    assert_eq!(status, StatusCode::OK);
    let sources = &list["sources"];
    assert_eq!(sources.as_array().unwrap().len(), 1, "{list}");
    assert_eq!(sources[0]["kind"], "mail");
    assert_eq!(sources[0]["name"], "inbox@test.local");
    assert_eq!(sources[0]["config"]["accountId"], "acct1");
    assert_eq!(sources[0]["hasToken"], false);

    let db = rusqlite::Connection::open(dir.join("server.sqlite")).unwrap();
    let counts = |db: &rusqlite::Connection| -> (i64, i64) {
        (
            db.query_row("SELECT count(*) FROM feed_messages", [], |r| r.get(0))
                .unwrap(),
            db.query_row(
                "SELECT count(*) FROM feed_messages WHERE read_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap(),
        )
    };
    assert_eq!(counts(&db), (2, 1));

    // Idempotent: a second start projects nothing new.
    drop(db);
    let server = start(dir.clone()).await;
    let (_, list) = server.send("GET", "session-token", "/sources", None).await;
    assert_eq!(list["sources"].as_array().unwrap().len(), 1);
    let db = rusqlite::Connection::open(dir.join("server.sqlite")).unwrap();
    assert_eq!(counts(&db), (2, 1));

    // A damaged projection is restored from the mail rows alone.
    db.execute("DELETE FROM feed_messages", []).unwrap();
    let rebuilt = mail_core::MailDb::open_unmigrated(&dir.join("server.sqlite"))
        .unwrap()
        .rebuild_feed()
        .unwrap();
    assert_eq!(
        (
            rebuilt.mail_messages,
            rebuilt.projected,
            rebuilt.feed_messages
        ),
        (2, 2, 2)
    );
    assert_eq!(counts(&db), (2, 1));
}
