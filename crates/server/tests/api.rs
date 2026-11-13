//! Runs against GreenMail (IMAPS 3993, SMTPS 3465, auth disabled), like mail-core's tests:
//! `java -Dgreenmail.setup.test.all -Dgreenmail.auth.disabled -jar greenmail-standalone.jar`.

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{Router, body::Bytes, extract::State, http::HeaderMap, routing::post};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::StreamExt;
use nsh_mail_server::{Config, build, router};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use web_push_native::{
    Auth,
    jwt_simple::algorithms::RS256KeyPair,
    p256::{SecretKey, elliptic_curve::sec1::ToEncodedPoint},
};

const PASSWORD: &str = "app-password-123";

fn unique(name: &str) -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{name}{n}@test.local")
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(unique("nsh-mail-server-"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn config(data_dir: PathBuf) -> Config {
    Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir,
        public_url: "http://localhost".into(),
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
    data_dir: PathBuf,
}

async fn start(config: Config) -> Server {
    mail_core::accept_invalid_certs_for_tests();
    let data_dir = config.data_dir.clone();
    let state = build(config).await.unwrap();
    let addr = serve(router(state)).await;
    Server {
        base: format!("http://{addr}/v1"),
        addr,
        http: Client::new(),
        data_dir,
    }
}

/// A server user with one GreenMail account.
struct User {
    token: String,
    email: String,
    account: String,
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

    async fn add_account(&self, token: &str, email: &str, imap_port: u16) -> reqwest::Response {
        let host = std::env::var("GREENMAIL_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let server = |port: u16| json!({ "host": host, "port": port, "security": "tls" });
        self.http
            .post(format!("{}/accounts", self.base))
            .bearer_auth(token)
            .json(&json!({ "email": email, "password": PASSWORD,
                           "imap": server(imap_port), "smtp": server(3465) }))
            .send()
            .await
            .unwrap()
    }

    async fn user(&self, name: &str) -> User {
        let email = unique(name);
        let token = self.signup(&email).await;
        let res = self.add_account(&token, &email, 3993).await;
        assert_eq!(res.status(), StatusCode::CREATED);
        let account = res.json::<Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        User {
            token,
            email,
            account,
        }
    }

    async fn get(&self, token: &str, path: &str) -> reqwest::Response {
        self.http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
    }

    async fn send(&self, from: &User, to: &User, subject: &str) {
        let res = self
            .http
            .post(format!("{}/send", self.base))
            .bearer_auth(&from.token)
            .json(&json!({ "accountId": from.account, "subject": subject,
                           "to": [{ "email": to.email }], "text": format!("{subject} body") }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    /// Polls the inbox until it holds `n` messages (sync runs in the background).
    async fn inbox(&self, user: &User, n: usize) -> Vec<Value> {
        for _ in 0..60 {
            let list: Vec<Value> = self
                .get(&user.token, "/messages?folder=inbox")
                .await
                .json()
                .await
                .unwrap();
            if list.len() >= n {
                return list;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        panic!("inbox of {} never reached {n} messages", user.email);
    }
}

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

async fn events(s: &Server, token: &str, device: Option<&str>) -> Ws {
    let mut url = format!("ws://{}/v1/events?token={token}", s.addr);
    if let Some(d) = device {
        url += &format!("&device={d}");
    }
    tokio_tungstenite::connect_async(url).await.unwrap().0
}

/// Skips frames until one matches.
async fn next_event(ws: &mut Ws, matches: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let frame = ws.next().await.unwrap().unwrap();
            let Ok(text) = frame.to_text() else { continue };
            let Ok(event) = serde_json::from_str::<Value>(text) else {
                continue;
            };
            if matches(&event) {
                return event;
            }
        }
    })
    .await
    .expect("no matching WebSocket event within 30 s")
}

#[tokio::test(flavor = "multi_thread")]
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

    let unreachable = s.add_account(&token, &unique("alice"), 1).await;
    assert_eq!(unreachable.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        unreachable.json::<Value>().await.unwrap()["code"],
        "ERR_NETWORK"
    );

    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let accounts: Value = s.get(&alice.token, "/accounts").await.json().await.unwrap();
    assert_eq!(accounts.as_array().unwrap().len(), 1);
    assert_eq!(accounts[0]["email"], alice.email.as_str());

    let secrets: Vec<_> = std::fs::read_dir(s.data_dir.join("users"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".secrets.json"))
        .collect();
    assert_eq!(secrets.len(), 2);
    for path in secrets {
        let stored = std::fs::read_to_string(path).unwrap();
        assert!(!stored.contains(PASSWORD), "secret stored in plaintext");
    }

    s.send(&bob, &alice, "Lunch?").await;
    s.send(&bob, &alice, "Report").await;
    let sent: Value = s
        .get(&bob.token, "/messages?folder=sent")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(sent.as_array().unwrap().len(), 2, "copies in Sent");

    let inbox = s.inbox(&alice, 2).await;
    assert_eq!(inbox.len(), 2);
    let page: Vec<Value> = s
        .get(&alice.token, "/messages?limit=1")
        .await
        .json()
        .await
        .unwrap();
    let older: Vec<Value> = s
        .get(
            &alice.token,
            &format!(
                "/messages?limit=1&before={}",
                page[0]["id"].as_str().unwrap()
            ),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(older.len(), 1);
    assert_ne!(older[0]["id"], page[0]["id"]);

    let lunch = inbox.iter().find(|m| m["subject"] == "Lunch?").unwrap();
    let lunch_id = lunch["id"].as_str().unwrap();
    assert_eq!(lunch["from"]["email"], bob.email.as_str());
    let patch = s
        .http
        .patch(format!("{}/messages/{lunch_id}", s.base))
        .bearer_auth(&alice.token)
        .json(&json!({ "seen": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::NO_CONTENT);
    let msg: Value = s
        .get(&alice.token, &format!("/messages/{lunch_id}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(msg["seen"], true);
    assert!(msg["text"].as_str().unwrap().contains("Lunch? body"));
    let hits: Value = s
        .get(&alice.token, "/search?q=lunch")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(hits.as_array().unwrap().len(), 1);

    assert_eq!(
        s.get(&bob.token, &format!("/messages/{lunch_id}"))
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

/// An attachment with a Chinese name, fetched from IMAP on demand and served as a download.
#[tokio::test(flavor = "multi_thread")]
async fn attachment_download() {
    use lettre::message::{Attachment, MultiPart, SinglePart, header::ContentType};
    use lettre::transport::smtp::client::{Tls, TlsParameters};
    use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

    let s = start(config(temp_dir())).await;
    let alice = s.user("alice").await;
    let host = std::env::var("GREENMAIL_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let pdf = b"%PDF-1.4 statement".to_vec();
    let message = lettre::Message::builder()
        .from("Bank <bank@test.local>".parse().unwrap())
        .to(alice.email.parse().unwrap())
        .subject("statement")
        .multipart(
            MultiPart::mixed()
                .singlepart(SinglePart::plain("Attached.".to_string()))
                .singlepart(
                    Attachment::new("10月账单 \"final\".pdf".into())
                        .body(pdf.clone(), ContentType::parse("application/pdf").unwrap()),
                ),
        )
        .unwrap();
    let tls = TlsParameters::builder(host.clone())
        .dangerous_accept_invalid_certs(true)
        .dangerous_accept_invalid_hostnames(true)
        .build_rustls()
        .unwrap();
    AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
        .port(3465)
        .tls(Tls::Wrapper(tls))
        .build()
        .send(message)
        .await
        .unwrap();
    s.http
        .post(format!("{}/sync", s.base))
        .bearer_auth(&alice.token)
        .send()
        .await
        .unwrap();
    let inbox = s.inbox(&alice, 1).await;
    let id = inbox[0]["id"].as_str().unwrap();

    let res = s
        .get(&alice.token, &format!("/messages/{id}/attachments/0"))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let header = |name: &str| res.headers()[name].to_str().unwrap().to_string();
    assert_eq!(header("content-type"), "application/pdf");
    assert_eq!(
        header("content-disposition"),
        "attachment; filename=\"10___ _final_.pdf\"; \
         filename*=UTF-8''10%E6%9C%88%E8%B4%A6%E5%8D%95%20%22final%22.pdf"
    );
    assert_eq!(header("content-security-policy"), "sandbox");
    assert_eq!(res.bytes().await.unwrap(), pdf);

    let missing = s
        .get(&alice.token, &format!("/messages/{id}/attachments/1"))
        .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let bob = s.signup(&unique("bob")).await;
    let other = s.get(&bob, &format!("/messages/{id}/attachments/0")).await;
    assert_eq!(other.status(), StatusCode::NOT_FOUND);
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

/// FCM pointed at the mock push service.
fn fcm_config(dir: PathBuf, mock: SocketAddr) -> Config {
    let key = RS256KeyPair::generate(2048).unwrap();
    let sa = dir.join("fcm.json");
    std::fs::write(&sa, json!({
        "type": "service_account", "project_id": "demo", "client_email": "push@demo.iam.gserviceaccount.com",
        "private_key": key.to_pem().unwrap(), "token_uri": format!("http://{mock}/token"),
    }).to_string()).unwrap();
    let mut cfg = config(dir);
    cfg.fcm_service_account = Some(sa);
    cfg.fcm_base_url = format!("http://{mock}");
    cfg
}

#[tokio::test(flavor = "multi_thread")]
async fn events_and_push() {
    let (mock, captured) = mock_push().await;
    let s = start(fcm_config(temp_dir(), mock)).await;

    let email = unique("alice");
    let token = s.signup(&email).await;
    let mut ws = events(&s, &token, None).await;
    let res = s.add_account(&token, &email, 3993).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let account = res.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let alice = User {
        token: token.clone(),
        email,
        account: account.clone(),
    };
    // Mail that arrives during the first sync is not "new"; wait for it to finish.
    let synced = next_event(&mut ws, |e| {
        e["type"] == "syncState" && e["accountId"] == account.as_str() && e["syncing"] == false
    })
    .await;
    assert!(synced["error"].is_null(), "first sync failed: {synced}");
    let bob = s.user("bob").await;

    let ua_secret = SecretKey::from_slice(&[9; 32]).unwrap();
    let ua_auth = [3u8; 16];
    let b64 = |b: &[u8]| URL_SAFE_NO_PAD.encode(b);
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

    s.send(&bob, &alice, "Lunch?").await;
    let event = next_event(&mut ws, |e| e["type"] == "newMail").await;
    assert_eq!(event["summary"]["subject"], "Lunch?");
    assert_eq!(event["summary"]["accountId"], account.as_str());

    let sends = wait_for(&captured, 3).await;
    let fcm_body: Value = sends
        .iter()
        .find(|(p, _, b)| p.ends_with("messages:send") && contains(b, "good-token"))
        .map(|(_, _, b)| serde_json::from_slice(b).unwrap())
        .expect("FCM send");
    assert_eq!(fcm_body["message"]["data"]["subject"], "Lunch?");
    assert_eq!(fcm_body["message"]["data"]["account"], account.as_str());
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
    let _ws2 = events(&s, &token, Some(&web)).await;
    captured.0.lock().unwrap().clear();
    s.send(&bob, &alice, "Secret plans").await;
    let second = next_event(&mut ws, |e| e["type"] == "newMail").await;
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
    assert_eq!(data["id"], second["summary"]["id"]);
    assert!(data.get("subject").is_none());
}

fn contains(b: &Bytes, s: &str) -> bool {
    String::from_utf8_lossy(b).contains(s)
}

/// Waits until `n` non-token requests reached the mock.
async fn wait_for(c: &Captured, n: usize) -> Vec<(String, HeaderMap, Bytes)> {
    for _ in 0..600 {
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

/// No request touches alice after the restart, so only the loops started at boot can see the mail.
#[test]
fn sync_resumes_after_restart() {
    let dir = temp_dir();
    let runtime = || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    };
    // Dropping the first runtime stops that server and its sync loops.
    let (alice, bob) = runtime().block_on(async {
        let s = start(config(dir.clone())).await;
        let (alice, bob) = (s.user("alice").await, s.user("bob").await);
        let res = s
            .http
            .post(format!("{}/devices", s.base))
            .bearer_auth(&alice.token)
            .json(&json!({ "kind": "fcm", "token": "good-token" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        s.send(&bob, &alice, "Before restart").await;
        s.inbox(&alice, 1).await;
        (alice, bob)
    });
    runtime().block_on(async {
        let (mock, captured) = mock_push().await;
        let s = start(fcm_config(dir, mock)).await;
        s.send(&bob, &alice, "After restart").await;
        let sends = wait_for(&captured, 1).await;
        let data = &serde_json::from_slice::<Value>(&sends[0].2).unwrap()["message"]["data"];
        assert_eq!(data["subject"], "After restart");
        assert_eq!(data["account"], alice.account.as_str());
    });
}

impl Server {
    /// Stages `data` in chunks, as the app does.
    async fn upload(&self, user: &User, filename: &str, mime_type: &str, data: &[u8]) -> String {
        let res = self
            .http
            .post(format!("{}/uploads", self.base))
            .bearer_auth(&user.token)
            .json(&json!({ "filename": filename, "mimeType": mime_type, "size": data.len() }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let id = res.json::<Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let mut offset = 0;
        for chunk in data.chunks(1024 * 1024) {
            let res = self
                .http
                .patch(format!("{}/uploads/{id}?offset={offset}", self.base))
                .bearer_auth(&user.token)
                .body(chunk.to_vec())
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            offset += chunk.len();
            assert_eq!(res.json::<Value>().await.unwrap()["received"], offset);
        }
        id
    }

    async fn send_draft(&self, from: &User, draft: Value) -> reqwest::Response {
        let mut draft = draft;
        draft["accountId"] = json!(from.account);
        self.http
            .post(format!("{}/send", self.base))
            .bearer_auth(&from.token)
            .json(&draft)
            .send()
            .await
            .unwrap()
    }

    /// The attachments of message `id`: names, types and sizes.
    async fn attachments(&self, user: &User, id: &str) -> Vec<(String, String, u64)> {
        let m: Value = self
            .get(&user.token, &format!("/messages/{id}"))
            .await
            .json()
            .await
            .unwrap();
        m["attachments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                (
                    a["filename"].as_str().unwrap_or_default().to_string(),
                    a["mimeType"].as_str().unwrap().to_string(),
                    a["size"].as_u64().unwrap(),
                )
            })
            .collect()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn attachments_upload_send_forward_and_limit() {
    let mut c = config(temp_dir());
    c.max_attachments = 3 * 1024 * 1024;
    let s = start(c).await;
    let (alice, bob) = (s.user("alice").await, s.user("bob").await);
    let me: Value = s.get(&alice.token, "/me").await.json().await.unwrap();
    assert_eq!(me["maxAttachmentBytes"], 3 * 1024 * 1024);

    let pdf: Vec<u8> = (0..2_500_000u32).map(|i| (i * 31 % 251) as u8).collect();
    let png: Vec<u8> = b"\x89PNG\r\n\x1a\n"
        .iter()
        .copied()
        .chain(0..200u8)
        .collect();
    let pdf_name = "2026年第三季度财务报告.pdf";
    let a = s.upload(&alice, pdf_name, "application/pdf", &pdf).await;
    let b = s.upload(&alice, "截图.png", "image/png", &png).await;
    let res = s
        .send_draft(
            &alice,
            json!({ "to": [{ "email": bob.email }], "subject": "附件", "text": "two files",
                    "attachments": [{ "kind": "upload", "id": a }, { "kind": "upload", "id": b }] }),
        )
        .await;
    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "{:?}",
        res.text().await
    );
    let expected = vec![
        (
            pdf_name.to_string(),
            "application/pdf".to_string(),
            pdf.len() as u64,
        ),
        (
            "截图.png".to_string(),
            "image/png".to_string(),
            png.len() as u64,
        ),
    ];
    let received = s.inbox(&bob, 1).await;
    let id = received[0]["id"].as_str().unwrap();
    assert_eq!(s.attachments(&bob, id).await, expected, "received");
    for (index, data) in [&pdf, &png].into_iter().enumerate() {
        let res = s
            .get(&bob.token, &format!("/messages/{id}/attachments/{index}"))
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            &res.bytes().await.unwrap()[..],
            &data[..],
            "bytes of {index}"
        );
    }

    let sent: Vec<Value> = s
        .get(&alice.token, "/messages?folder=sent")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        s.attachments(&alice, sent[0]["id"].as_str().unwrap()).await,
        expected,
        "Sent copy"
    );

    // A sent draft's uploads are gone.
    let again = s
        .send_draft(
            &alice,
            json!({ "to": [{ "email": bob.email }], "subject": "again", "text": "",
                                     "attachments": [{ "kind": "upload", "id": a }] }),
        )
        .await;
    assert_eq!(again.status(), StatusCode::NOT_FOUND);

    // Bob forwards the PDF only.
    let res = s
        .send_draft(
            &bob,
            json!({ "to": [{ "email": alice.email }], "subject": "Fwd: 附件", "text": "fyi",
                    "attachments": [{ "kind": "forward", "messageId": id, "index": 0 }] }),
        )
        .await;
    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "{:?}",
        res.text().await
    );
    let inbox = s.inbox(&alice, 1).await;
    assert_eq!(
        s.attachments(&alice, inbox[0]["id"].as_str().unwrap())
            .await,
        expected[..1]
    );

    // Over the limit: one file at creation, several together at send.
    let res = s
        .http
        .post(format!("{}/uploads", s.base))
        .bearer_auth(&alice.token)
        .json(&json!({ "filename": "huge.bin", "mimeType": "", "size": 4 * 1024 * 1024 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(res.json::<Value>().await.unwrap()["code"], "ERR_TOO_LARGE");
    let c = s.upload(&alice, "c.pdf", "application/pdf", &pdf).await;
    let d = s.upload(&alice, "d.pdf", "application/pdf", &pdf).await;
    let res = s
        .send_draft(
            &alice,
            json!({ "to": [{ "email": bob.email }], "subject": "too big", "text": "",
                    "attachments": [{ "kind": "upload", "id": c }, { "kind": "upload", "id": d }] }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(res.json::<Value>().await.unwrap()["code"], "ERR_TOO_LARGE");

    let res = s
        .http
        .delete(format!("{}/uploads/{c}", s.base))
        .bearer_auth(&alice.token)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let left: Vec<_> = std::fs::read_dir(s.data_dir.join("uploads"))
        .unwrap()
        .flat_map(|u| std::fs::read_dir(u.unwrap().path()).unwrap())
        .map(|f| f.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(left.len(), 2, "only d stays staged: {left:?}");
    assert!(left.iter().all(|f| f.starts_with(&d)));
}
