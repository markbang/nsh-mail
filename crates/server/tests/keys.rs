//! Rotating NSHMAIL_MASTER_KEY and the VAPID key. No mail server needed.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use mail_core::secrets::{EncryptedFileStore, SecretStore};
use nsh_mail_server::{Config, build, router};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use web_push_native::{
    jwt_simple::algorithms::ES256KeyPair,
    p256::{SecretKey, elliptic_curve::sec1::ToEncodedPoint},
};

const OLD: [u8; 32] = [1; 32];
const NEW: [u8; 32] = [2; 32];
const WRONG: [u8; 32] = [3; 32];

fn temp_dir() -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("nsh-mail-keys-{n}"));
    std::fs::create_dir_all(dir.join("users")).unwrap();
    dir
}

fn config(data_dir: &Path, key: [u8; 32], old: Option<[u8; 32]>, vapid: Option<&str>) -> Config {
    Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir: data_dir.to_path_buf(),
        public_url: "http://localhost".into(),
        master_key: key,
        master_key_old: old,
        web_dir: None,
        cors_origins: vec![],
        allow_signup: true,
        oauth: Default::default(),
        fcm_service_account: None,
        fcm_base_url: String::new(),
        vapid_private_key: vapid.map(str::to_owned),
        vapid_subject: "mailto:test@example.com".into(),
        max_attachments: 25 * 1024 * 1024,
    }
}

fn store(dir: &Path, user: i64, key: [u8; 32]) -> EncryptedFileStore {
    EncryptedFileStore::with_key(dir.join(format!("users/{user}.secrets.json")), key)
}

fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(dir.join("users"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read(p).unwrap())
        })
        .collect()
}

#[tokio::test]
async fn master_key_rotation() {
    let dir = temp_dir();
    let mut expected = BTreeMap::new();
    for (k, v) in [
        ("acct-a/password", "hunter2"),
        ("acct-b/oauth", "{\"refresh\":\"r\"}"),
    ] {
        store(&dir, 4, OLD).set(k, v).unwrap();
        expected.insert(k.to_string(), v.to_string());
    }
    // A file with one entry already under the new key, as when an account was added after the
    // key changed but before the rotation ran.
    store(&dir, 5, OLD).set("acct-c/password", "old").unwrap();
    store(&dir, 5, NEW).set("acct-d/password", "new").unwrap();

    build(config(&dir, NEW, Some(OLD), None)).await.unwrap();
    let (got, used_old) = store(&dir, 4, NEW).read_all(None).unwrap();
    assert_eq!((got, used_old), (expected, 0));
    let (got, _) = store(&dir, 5, NEW).read_all(None).unwrap();
    assert_eq!(got["acct-c/password"], "old");
    assert_eq!(got["acct-d/password"], "new");
    assert!(
        files(&dir).keys().all(|n| n.ends_with(".secrets.json")),
        "backup or temp files left: {:?}",
        files(&dir).keys().collect::<Vec<_>>()
    );

    let before = files(&dir);
    build(config(&dir, NEW, Some(OLD), None)).await.unwrap();
    build(config(&dir, NEW, None, None)).await.unwrap();
    assert_eq!(files(&dir), before, "a second start rewrote rotated files");
}

#[tokio::test]
async fn refuses_to_start_when_a_secret_opens_with_neither_key() {
    let dir = temp_dir();
    store(&dir, 4, OLD)
        .set("acct-a/password", "hunter2")
        .unwrap();
    store(&dir, 6, OLD).set("acct-b/password", "pw").unwrap();
    let before = files(&dir);

    for old in [Some(WRONG), None] {
        let e = match build(config(&dir, NEW, old, None)).await {
            Ok(_) => panic!("started with {old:?}"),
            Err(e) => format!("{e:#}"),
        };
        assert!(
            e.contains("4.secrets.json") && e.contains("6.secrets.json"),
            "{e}"
        );
        assert!(e.contains("NSHMAIL_MASTER_KEY_OLD"), "{e}");
        assert_eq!(files(&dir), before, "files changed");
    }
}

fn vapid_key() -> String {
    URL_SAFE_NO_PAD.encode(ES256KeyPair::generate().to_bytes())
}

#[tokio::test]
async fn webpush_devices_from_another_vapid_key_are_dropped() {
    let dir = temp_dir();
    let http = Client::new();
    let start = |vapid: String| {
        let dir = dir.clone();
        async move {
            let state = build(config(&dir, NEW, None, Some(&vapid))).await.unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
            format!("http://{addr}/v1")
        }
    };
    let ua = SecretKey::from_slice(&[9; 32]).unwrap().public_key();
    let subscription = json!({
        "endpoint": "https://push.example/wp/1",
        "keys": {
            "p256dh": URL_SAFE_NO_PAD.encode(ua.to_encoded_point(false).as_bytes()),
            "auth": URL_SAFE_NO_PAD.encode([3u8; 16]),
        },
    });

    let base = start(vapid_key()).await;
    let token = http
        .post(format!("{base}/auth/signup"))
        .json(&json!({ "email": "a@test.local", "password": "correct horse" }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let first_key = http
        .get(format!("{base}/push/vapid-key"))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["key"]
        .clone();
    let register = |base: String, key: Value| {
        let (http, token, subscription) = (http.clone(), token.clone(), subscription.clone());
        async move {
            http.post(format!("{base}/devices"))
                .bearer_auth(&token)
                .json(&json!({ "kind": "webpush", "subscription": subscription, "vapidKey": key }))
                .send()
                .await
                .unwrap()
        }
    };
    let res = register(base.clone(), json!("not-this-server")).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let res = register(base.clone(), first_key.clone()).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let device = res.json::<Value>().await.unwrap()["id"].clone();

    let base = start(vapid_key()).await;
    let second_key = http
        .get(format!("{base}/push/vapid-key"))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["key"]
        .clone();
    assert_ne!(first_key, second_key);
    let gone = http
        .delete(format!("{base}/devices/{}", device.as_str().unwrap()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(
        gone.status(),
        StatusCode::NOT_FOUND,
        "stale subscription kept"
    );
    let res = register(base.clone(), first_key).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let res = register(base, second_key).await;
    assert_eq!(res.status(), StatusCode::CREATED);
}
