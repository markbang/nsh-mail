use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use aes_gcm::aead::OsRng;
use axum::{Router, body::Bytes, extract::State, http::HeaderMap, routing::post};
use futures_util::StreamExt;
use nsh_mail_server::{Config, build, crypto, mail::fake::FakeBackend, router};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use web_push_native::{
    Auth,
    jwt_simple::algorithms::RS256KeyPair,
    p256::{SecretKey, elliptic_curve::sec1::ToEncodedPoint},
};

fn temp_dir() -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("nsh-mail-test-{}", api_hex(&crypto::random::<8>())));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn api_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn config(data_dir: PathBuf) -> Config {
    Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir,
        public_url: "http://localhost".into(),
        master_key: crypto::random::<32>(),
        web_dir: None,
        cors_origins: vec![],
        allow_signup: true,
        google: None,
        microsoft: None,
        fcm_service_account: None,
        fcm_base_url: String::new(),
        vapid_private_key: None,
        vapid_subject: "mailto:test@example.com".into(),
    }
}

async fn serve(app: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

struct Server {
    base: String,
    addr: SocketAddr,
    http: Client,
    fake: Arc<FakeBackend>,
    data_dir: PathBuf,
}

async fn start(config: Config) -> Server {
    let fake = Arc::new(FakeBackend::default());
    let data_dir = config.data_dir.clone();
    let state = build(config, fake.clone()).await.unwrap();
    let addr = serve(router(state)).await;
    Server {
        base: format!("http://{addr}/v1"),
        addr,
        http: Client::new(),
        fake,
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

    async fn add_account(&self, token: &str, password: &str) -> reqwest::Response {
        let server =
            |host: &str, port: u16| json!({ "host": host, "port": port, "security": "tls" });
        self.http.post(format!("{}/accounts", self.base)).bearer_auth(token)
            .json(&json!({ "email": "me@example.com", "password": password,
                           "imap": server("imap.example.com", 993), "smtp": server("smtp.example.com", 465) }))
            .send().await.unwrap()
    }

    async fn get(&self, token: &str, path: &str) -> reqwest::Response {
        self.http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn auth_accounts_and_messages() {
    let s = start(config(temp_dir())).await;
    let token = s.signup("Alice@Example.com").await;

    let dup = s
        .http
        .post(format!("{}/auth/signup", s.base))
        .json(&json!({ "email": "alice@example.com", "password": "another one" }))
        .send()
        .await
        .unwrap();
    assert_eq!(dup.status(), StatusCode::CONFLICT);
    let bad = s
        .http
        .post(format!("{}/auth/login", s.base))
        .json(&json!({ "email": "alice@example.com", "password": "wrong password" }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
    let login = s
        .http
        .post(format!("{}/auth/login", s.base))
        .json(&json!({ "email": "alice@example.com", "password": "correct horse" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);

    assert_eq!(
        s.add_account(&token, "wrong").await.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let res = s.add_account(&token, "app-password-123").await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let account: Value = res.json().await.unwrap();
    let account_id = account["id"].as_str().unwrap();

    let accounts: Value = s.get(&token, "/accounts").await.json().await.unwrap();
    assert_eq!(accounts.as_array().unwrap().len(), 1);
    assert_eq!(accounts[0]["email"], "me@example.com");

    let db = rusqlite::Connection::open(s.data_dir.join("server.sqlite")).unwrap();
    let stored: Vec<u8> = db
        .query_row("SELECT value FROM secrets", [], |r| r.get(0))
        .unwrap();
    assert!(
        !stored.windows(16).any(|w| w == b"app-password-123"),
        "secret stored in plaintext"
    );

    let first = s
        .fake
        .deliver(account_id, "bob@example.com", "Lunch?")
        .unwrap();
    s.fake
        .deliver(account_id, "carol@example.com", "Report")
        .unwrap();
    let inbox: Value = s
        .get(&token, "/messages?folder=inbox&limit=10")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(inbox.as_array().unwrap().len(), 2);
    assert_eq!(inbox[0]["subject"], "Report");
    let older: Value = s
        .get(
            &token,
            &format!("/messages?before={}", inbox[0]["id"].as_str().unwrap()),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(older[0]["id"], first.id.as_str());

    let patch = s
        .http
        .patch(format!("{}/messages/{}", s.base, first.id))
        .bearer_auth(&token)
        .json(&json!({ "seen": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::NO_CONTENT);
    let msg: Value = s
        .get(&token, &format!("/messages/{}", first.id))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(msg["seen"], true);
    let hits: Value = s.get(&token, "/search?q=lunch").await.json().await.unwrap();
    assert_eq!(hits.as_array().unwrap().len(), 1);

    let other = s.signup("mallory@example.com").await;
    assert_eq!(
        s.get(&other, &format!("/messages/{}", first.id))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    let out = s
        .http
        .post(format!("{}/auth/logout", s.base))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(out.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        s.get(&token, "/accounts").await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<(String, HeaderMap, Bytes)>>>);

async fn mock_push() -> (SocketAddr, Captured) {
    async fn record(
        State(c): State<Captured>,
        uri: axum::http::Uri,
        headers: HeaderMap,
        body: Bytes,
    ) -> (StatusCode, String) {
        let path = uri.path().to_string();
        c.0.lock()
            .unwrap()
            .push((path.clone(), headers, body.clone()));
        match path.as_str() {
            "/token" => (StatusCode::OK, json!({ "access_token": "fcm-access", "expires_in": 3600 }).to_string()),
            _ if String::from_utf8_lossy(&body).contains("gone-token") => {
                (StatusCode::NOT_FOUND, json!({ "error": { "status": "NOT_FOUND", "details": [{ "errorCode": "UNREGISTERED" }] } }).to_string())
            }
            _ => (StatusCode::CREATED, "{}".into()),
        }
    }
    let captured = Captured::default();
    let app = Router::new()
        .fallback(post(record))
        .with_state(captured.clone());
    (serve(app).await, captured)
}

#[tokio::test]
async fn events_and_push() {
    let (mock, captured) = mock_push().await;
    let dir = temp_dir();
    let key = RS256KeyPair::generate(2048).unwrap();
    let sa = dir.join("fcm.json");
    std::fs::write(&sa, json!({
        "type": "service_account", "project_id": "demo", "client_email": "push@demo.iam.gserviceaccount.com",
        "private_key": key.to_pem().unwrap(), "token_uri": format!("http://{mock}/token"),
    }).to_string()).unwrap();
    let mut cfg = config(dir);
    cfg.fcm_service_account = Some(sa);
    cfg.fcm_base_url = format!("http://{mock}");
    let s = start(cfg).await;

    let token = s.signup("alice@example.com").await;
    let account: Value = s
        .add_account(&token, "app-password")
        .await
        .json()
        .await
        .unwrap();
    let account_id = account["id"].as_str().unwrap().to_string();

    let ua_secret = SecretKey::random(&mut OsRng);
    let ua_auth = crypto::random::<16>();
    let b64 = |b: &[u8]| crypto::b64url(b);
    let device = |body: Value| {
        let (http, base, token) = (s.http.clone(), s.base.clone(), token.clone());
        async move {
            let res = http
                .post(format!("{base}/devices"))
                .bearer_auth(&token)
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::CREATED);
            res.json::<Value>().await.unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        }
    };
    let _fcm = device(json!({ "kind": "fcm", "token": "good-token" })).await;
    let gone = device(json!({ "kind": "fcm", "token": "gone-token" })).await;
    let web = device(json!({ "kind": "webpush", "subscription": {
        "endpoint": format!("http://{mock}/wp/1"),
        "keys": { "p256dh": b64(ua_secret.public_key().to_encoded_point(false).as_bytes()), "auth": b64(&ua_auth) },
    }})).await;

    let (mut ws, _) =
        tokio_tungstenite::connect_async(format!("ws://{}/v1/events?token={token}", s.addr))
            .await
            .unwrap();
    s.fake
        .deliver(&account_id, "bob@example.com", "Lunch?")
        .unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let event: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(event["type"], "newMail");
    assert_eq!(event["summary"]["subject"], "Lunch?");

    let sends = wait_for(&captured, 3).await;
    let fcm_body: Value = sends
        .iter()
        .find(|(p, _, b)| p.ends_with("messages:send") && contains(b, "good-token"))
        .map(|(_, _, b)| serde_json::from_slice(b).unwrap())
        .expect("FCM send");
    assert_eq!(fcm_body["message"]["data"]["subject"], "Lunch?");
    assert_eq!(fcm_body["message"]["data"]["account"], account_id.as_str());
    let (_, headers, body) = sends
        .iter()
        .find(|(p, _, _)| p == "/wp/1")
        .expect("web push");
    assert_eq!(headers["content-encoding"], "aes128gcm");
    assert!(
        headers["authorization"]
            .to_str()
            .unwrap()
            .starts_with("vapid t=")
    );
    let plain =
        web_push_native::decrypt(body.to_vec(), &ua_secret, Auth::from_slice(&ua_auth)).unwrap();
    let payload: Value = serde_json::from_slice(&plain).unwrap();
    assert_eq!(payload["subject"], "Lunch?");

    let del = s
        .http
        .delete(format!("{}/devices/{gone}", s.base))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(
        del.status(),
        StatusCode::NOT_FOUND,
        "UNREGISTERED device should have been removed"
    );

    // Hidden content, and the web device is online: FCM gets ids only, Web Push is skipped.
    let res = s
        .http
        .patch(format!("{}/me", s.base))
        .bearer_auth(&token)
        .json(&json!({ "hideContent": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let (_ws2, _) = tokio_tungstenite::connect_async(format!(
        "ws://{}/v1/events?token={token}&device={web}",
        s.addr
    ))
    .await
    .unwrap();
    captured.0.lock().unwrap().clear();
    let second = s
        .fake
        .deliver(&account_id, "bob@example.com", "Secret plans")
        .unwrap();
    let sends = wait_for(&captured, 1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !captured
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|(p, _, _)| p == "/wp/1"),
        "online device was pushed"
    );
    let data: Value =
        serde_json::from_slice::<Value>(&sends[0].2).unwrap()["message"]["data"].clone();
    assert_eq!(data["id"], second.id.as_str());
    assert!(data.get("subject").is_none());
}

fn contains(b: &Bytes, s: &str) -> bool {
    String::from_utf8_lossy(b).contains(s)
}

/// Waits until `n` non-token requests reached the mock.
async fn wait_for(c: &Captured, n: usize) -> Vec<(String, HeaderMap, Bytes)> {
    for _ in 0..100 {
        let got: Vec<_> =
            c.0.lock()
                .unwrap()
                .iter()
                .filter(|(p, _, _)| p != "/token")
                .cloned()
                .collect();
        if got.len() >= n {
            return got;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("mock received fewer than {n} push requests");
}
