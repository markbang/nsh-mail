//! Old per-user databases (`users/<id>.sqlite`, written by the mail-core release before the
//! shared database) move into `server.sqlite` at start. Needs GreenMail like `api.rs`.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use lettre::message::{Attachment, MultiPart, SinglePart, header::ContentType};
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use mail_core_v0 as v0;
use nsh_mail_server::{Config, build, crypto::sha256, db::Db, router};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

const KEY: [u8; 32] = [7; 32];

fn host() -> String {
    std::env::var("GREENMAIL_HOST").unwrap_or_else(|_| "127.0.0.1".into())
}

fn unique(name: &str) -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{name}{n}@test.local")
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(unique("nsh-mail-migrate-"));
    std::fs::create_dir_all(dir.join("users")).unwrap();
    dir
}

fn config(data_dir: &Path) -> Config {
    Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir: data_dir.to_path_buf(),
        public_url: "http://localhost".into(),
        master_key: KEY,
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

async fn deliver(to: &str, subject: &str, text: String, attachment: Option<&[u8]>) {
    let builder = lettre::Message::builder()
        .from("Sender <sender@test.local>".parse().unwrap())
        .to(to.parse().unwrap())
        .subject(subject);
    let message = match attachment {
        Some(data) => builder.multipart(
            MultiPart::mixed()
                .singlepart(SinglePart::plain(text))
                .singlepart(Attachment::new("report.pdf".into()).body(
                    data.to_vec(),
                    ContentType::parse("application/pdf").unwrap(),
                )),
        ),
        None => builder.body(text),
    }
    .unwrap();
    let tls = TlsParameters::builder(host())
        .dangerous_accept_invalid_certs(true)
        .dangerous_accept_invalid_hostnames(true)
        .build_rustls()
        .unwrap();
    AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host())
        .port(3465)
        .tls(Tls::Wrapper(tls))
        .build()
        .send(message)
        .await
        .unwrap();
}

/// A user as the old server kept them: a row in server.sqlite, a session, and mail synced by the
/// old engine into `users/<id>.sqlite` with credentials in `users/<id>.secrets.json`.
struct Legacy {
    id: i64,
    token: String,
    email: String,
    account: String,
}

async fn legacy_user(db: &Db, users: &Path, name: &str) -> (Legacy, v0::Engine) {
    let email = unique(name);
    let id = db.create_user(&email, "unused").unwrap().unwrap();
    let token = format!("token-{id}-{}", unique(""));
    db.create_session(&sha256(token.as_bytes()), id).unwrap();
    let engine = v0::Engine::open_with(v0::EngineOptions {
        db_path: users.join(format!("{id}.sqlite")),
        secrets: Arc::new(v0::secrets::EncryptedFileStore::with_key(
            users.join(format!("{id}.secrets.json")),
            KEY,
        )),
        oauth: Default::default(),
        max_attachments: 25 * 1024 * 1024,
    })
    .unwrap();
    let endpoint = |port| v0::types::Endpoint {
        host: host(),
        port,
        security: v0::types::Security::Tls,
    };
    let account = engine
        .add_imap_account(v0::types::ImapInput {
            email: email.clone(),
            name: None,
            username: None,
            password: "secret".into(),
            imap: endpoint(3993),
            smtp: endpoint(3465),
            provider: None,
        })
        .await
        .unwrap()
        .id;
    (
        Legacy {
            id,
            token,
            email,
            account,
        },
        engine,
    )
}

impl Legacy {
    async fn inbox(&self, engine: &v0::Engine, n: usize) -> Vec<v0::types::Summary> {
        for _ in 0..60 {
            engine.sync(&self.account).await.unwrap();
            let list = engine
                .list(v0::types::ListQuery {
                    folder: v0::types::FolderKind::Inbox,
                    account: None,
                    before: None,
                    limit: 50,
                })
                .await
                .unwrap();
            if list.len() >= n {
                return list;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        panic!("old inbox of {} never reached {n}", self.email);
    }
}

struct Api {
    base: String,
    http: Client,
}

impl Api {
    async fn get(&self, token: &str, path: &str) -> reqwest::Response {
        self.http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
    }

    async fn json(&self, token: &str, path: &str) -> Value {
        let res = self.get(token, path).await;
        assert_eq!(res.status(), StatusCode::OK, "{path}");
        res.json().await.unwrap()
    }

    async fn ids(&self, token: &str, path: &str) -> Vec<String> {
        let mut ids: Vec<String> = self
            .json(token, path)
            .await
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap().to_string())
            .collect();
        ids.sort();
        ids
    }
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn count(db: &Path, sql: &str) -> i64 {
    rusqlite::Connection::open(db)
        .unwrap()
        .query_row(sql, [], |r| r.get(0))
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn per_user_databases_move_into_server_sqlite() {
    mail_core::accept_invalid_certs_for_tests();
    v0::accept_invalid_certs_for_tests();
    let dir = temp_dir();
    let users = dir.join("users");

    // The old server: users and sessions in server.sqlite, mail per user.
    let db = Db::open(&dir.join("server.sqlite")).unwrap();
    let (alice, alice_engine) = legacy_user(&db, &users, "alice").await;
    let (bob, bob_engine) = legacy_user(&db, &users, "bob").await;
    let pdf = b"%PDF-1.4 quarterly".to_vec();
    deliver(
        &alice.email,
        "Quarterly",
        "numbers zebra".into(),
        Some(&pdf),
    )
    .await;
    deliver(&alice.email, "Lunch", "noon?".into(), None).await;
    // Over the size fetched whole during sync: its body is not cached.
    let big = format!("quokka newsletter\n{}", "lorem ipsum ".repeat(4000));
    deliver(&alice.email, "Newsletter", big, None).await;
    deliver(&bob.email, "Private", "xylophone".into(), None).await;
    let old_alice = alice.inbox(&alice_engine, 3).await;
    let old_bob = bob.inbox(&bob_engine, 1).await;
    let id_of = |list: &[v0::types::Summary], subject: &str| {
        list.iter()
            .find(|m| m.subject == subject)
            .unwrap()
            .id
            .clone()
    };
    let (quarterly, lunch, newsletter) = (
        id_of(&old_alice, "Quarterly"),
        id_of(&old_alice, "Lunch"),
        id_of(&old_alice, "Newsletter"),
    );
    let private = id_of(&old_bob, "Private");
    alice_engine
        .set_flags(
            &lunch,
            v0::types::Flags {
                seen: Some(true),
                flagged: Some(true),
            },
        )
        .await
        .unwrap();
    assert_eq!(alice_engine.search("zebra", 10).await.unwrap().len(), 1);
    drop((alice_engine, bob_engine, db));

    let sources: Vec<(i64, i64)> = [alice.id, bob.id]
        .iter()
        .map(|id| {
            let f = users.join(format!("{id}.sqlite"));
            (
                count(&f, "SELECT count(*) FROM messages"),
                count(&f, "SELECT count(*) FROM messages_fts"),
            )
        })
        .collect();

    // The new server.
    let state = build(config(&dir)).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    let api = Api {
        base: format!("http://{addr}/v1"),
        http: Client::new(),
    };
    for id in [alice.id, bob.id] {
        let names = files(&users);
        assert!(
            names.contains(&format!("{id}.sqlite.migrated")),
            "{names:?}"
        );
        assert!(!names.contains(&format!("{id}.sqlite")), "{names:?}");
    }
    let server_db = dir.join("server.sqlite");
    for (id, (messages, indexed)) in [alice.id, bob.id].iter().zip(&sources) {
        assert_eq!(
            count(
                &server_db,
                &format!("SELECT count(*) FROM messages WHERE user_id = {id}")
            ),
            *messages
        );
        assert_eq!(
            count(
                &server_db,
                &format!(
                    "SELECT count(*) FROM messages_fts WHERE rowid IN \
                     (SELECT pk FROM messages WHERE user_id = {id})"
                )
            ),
            *indexed
        );
    }

    // Existing sessions, same message ids, flags, cached body, attachment, search.
    let mut expected: Vec<String> = old_alice.iter().map(|m| m.id.clone()).collect();
    expected.sort();
    assert_eq!(
        api.ids(&alice.token, "/messages?folder=inbox").await,
        expected
    );
    assert_eq!(
        api.ids(&bob.token, "/messages?folder=inbox").await,
        vec![private.clone()]
    );
    let msg = api.json(&alice.token, &format!("/messages/{lunch}")).await;
    assert_eq!(
        (msg["seen"].clone(), msg["flagged"].clone()),
        (json!(true), json!(true))
    );
    let msg = api
        .json(&alice.token, &format!("/messages/{quarterly}"))
        .await;
    assert_eq!(msg["attachments"][0]["filename"], "report.pdf");
    let res = api
        .get(
            &alice.token,
            &format!("/messages/{quarterly}/attachments/0"),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.bytes().await.unwrap(), pdf);
    let msg = api
        .json(&alice.token, &format!("/messages/{newsletter}"))
        .await;
    assert!(msg["text"].as_str().unwrap().contains("lorem ipsum"));
    assert_eq!(
        api.ids(&alice.token, "/search?q=zebra").await,
        vec![quarterly.clone()]
    );
    assert_eq!(
        api.ids(&alice.token, "/search?q=quokka").await,
        vec![newsletter.clone()]
    );
    assert_eq!(
        api.ids(&bob.token, "/search?q=xylophone").await,
        vec![private.clone()]
    );
    let accounts = api.json(&alice.token, "/accounts").await;
    assert_eq!(accounts.as_array().unwrap().len(), 1);
    assert_eq!(accounts[0]["email"], alice.email.as_str());

    // Nothing of one user reaches the other.
    for q in ["zebra", "quokka", "Quarterly", "numbers"] {
        assert!(
            api.ids(&bob.token, &format!("/search?q={q}"))
                .await
                .is_empty(),
            "{q}"
        );
    }
    assert!(
        api.ids(&alice.token, "/search?q=xylophone")
            .await
            .is_empty()
    );
    for path in [
        format!("/messages/{quarterly}"),
        format!("/messages/{quarterly}/attachments/0"),
        format!("/messages?before={quarterly}"),
    ] {
        assert_eq!(
            api.get(&bob.token, &path).await.status(),
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    let patch = api
        .http
        .patch(format!("{}/messages/{quarterly}", api.base))
        .bearer_auth(&bob.token)
        .json(&json!({ "seen": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::NOT_FOUND);
    let gone = api
        .http
        .delete(format!("{}/accounts/{}", api.base, alice.account))
        .bearer_auth(&bob.token)
        .send()
        .await
        .unwrap();
    assert!(gone.status().is_client_error() || gone.status().is_success());
    assert_eq!(
        api.json(&alice.token, "/accounts")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let unread = api.json(&bob.token, "/unread").await;
    assert!(unread.get(&alice.account).is_none(), "{unread}");

    // Sync continues from the imported state: same ids and flags, and new mail arrives.
    deliver(&alice.email, "After", "post migration".into(), None).await;
    let res = api
        .http
        .post(format!("{}/sync", api.base))
        .bearer_auth(&alice.token)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let now = api.ids(&alice.token, "/messages?folder=inbox").await;
    assert_eq!(now.len(), 4, "{now:?}");
    assert!(expected.iter().all(|id| now.contains(id)), "{now:?}");
    let msg = api.json(&alice.token, &format!("/messages/{lunch}")).await;
    assert_eq!(
        (msg["seen"].clone(), msg["flagged"].clone()),
        (json!(true), json!(true))
    );

    // A copy of the migrated file in another data dir, without credentials so no sync runs.
    let other = temp_dir();
    let migrated = users.join(format!("{}.sqlite.migrated", alice.id));
    let copy = other.join("users/1.sqlite");
    std::fs::copy(&migrated, &copy).unwrap();
    drop(build(config(&other)).await.unwrap());
    let rows = count(
        &other.join("server.sqlite"),
        "SELECT count(*) FROM messages",
    );
    assert_eq!(rows, sources[0].0);
    // Stopped after the import committed but before the rename: verified, renamed, not doubled.
    std::fs::copy(&migrated, &copy).unwrap();
    drop(build(config(&other)).await.unwrap());
    assert_eq!(
        files(&other.join("users")),
        ["1.sqlite.migrated", "1.sqlite.migrated.2"]
    );
    // A plain restart changes nothing.
    drop(build(config(&other)).await.unwrap());
    assert_eq!(
        count(
            &other.join("server.sqlite"),
            "SELECT count(*) FROM messages"
        ),
        rows
    );
    assert_eq!(
        files(&other.join("users")),
        ["1.sqlite.migrated", "1.sqlite.migrated.2"]
    );

    // A damaged file stops the server and stays as it was.
    let broken = temp_dir();
    let mut bytes = std::fs::read(&migrated).unwrap();
    bytes[100..].fill(0x55);
    let path = broken.join("users/5.sqlite");
    std::fs::write(&path, &bytes).unwrap();
    let e = build(config(&broken))
        .await
        .err()
        .expect("refuses to start");
    let message = format!("{e:#}");
    assert!(
        message.contains("5.sqlite") && message.contains("left as it was"),
        "{message}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(files(&broken.join("users")), ["5.sqlite"]);

    // The app's local database from before user ids is upgraded in place.
    let local = temp_dir().join("mail.sqlite");
    std::fs::copy(&migrated, &local).unwrap();
    for _ in 0..2 {
        let engine =
            mail_core::Engine::open(&local, Arc::new(mail_core::secrets::MemoryStore::new()))
                .unwrap();
        let hits = engine.search("zebra", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, quarterly);
        let (meta, data) = engine.attachment(&quarterly, 0).await.unwrap();
        assert_eq!(
            (meta.filename.as_deref(), data),
            (Some("report.pdf"), pdf.clone())
        );
        let unread = engine.unread().await.unwrap();
        assert_eq!(unread.values().sum::<u32>(), 2);
    }
}
