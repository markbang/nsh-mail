//! Paged, resumable sync and backfill against GreenMail (IMAPS 3993, plain IMAP 3143 for
//! seeding). Local: `java -Dgreenmail.setup.test.all -Dgreenmail.auth.disabled -jar
//! greenmail-standalone.jar`, then `cargo test -p mail-core --features insecure-test-tls`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mail_core::secrets::MemoryStore;
use mail_core::*;
use tokio::sync::broadcast;

const SEED: u32 = 900;
const WINDOW: u32 = 500;
const PAGE: u32 = 200;

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

fn input(email: &str) -> ImapInput {
    ImapInput {
        email: email.into(),
        name: None,
        username: None,
        password: "secret".into(),
        imap: Endpoint {
            host: host(),
            port: 3993,
            security: Security::Tls,
        },
        smtp: Endpoint {
            host: host(),
            port: 3465,
            security: Security::Tls,
        },
        provider: None,
    }
}

fn engine(dir: &tempfile::TempDir) -> Engine {
    accept_invalid_certs_for_tests();
    Engine::open(dir.path().join("mail.sqlite"), Arc::new(MemoryStore::new())).unwrap()
}

/// Delivers `count` messages with subjects `msg-0001`… into `email`'s INBOX, over plain IMAP
/// (much faster than SMTP, and GreenMail creates the user on login).
async fn seed(email: &str, count: u32) {
    let tcp = tokio::net::TcpStream::connect((host(), 3143))
        .await
        .unwrap();
    let mut client = async_imap::Client::new(tcp);
    client.read_response().await.unwrap();
    let mut session = client
        .login(email, "secret")
        .await
        .map_err(|e| e.0)
        .unwrap();
    for i in 1..=count {
        let raw = format!(
            "From: sender@test.local\r\nTo: {email}\r\nSubject: msg-{i:04}\r\n\
             Message-ID: <{i}@test.local>\r\nDate: Mon, 01 Jan 2024 00:00:00 +0000\r\n\r\n\
             body of message {i}\r\n"
        );
        session
            .append("INBOX", None, None, raw.as_bytes())
            .await
            .unwrap();
    }
}

/// Every subject in the inbox, paging through `list`'s 500-message cap.
async fn subjects(engine: &Engine, account: &Account) -> Vec<String> {
    let mut out = Vec::new();
    let mut before = None;
    loop {
        let page = engine
            .list(ListQuery {
                folder: FolderKind::Inbox,
                account: Some(account.id.clone()),
                before,
                limit: 500,
            })
            .await
            .unwrap();
        let last = page.len() < 500;
        before = page.last().map(|s| s.id.clone());
        out.extend(page.into_iter().map(|s| s.subject));
        if last || before.is_none() {
            return out;
        }
    }
}

/// The seeded subjects 1..=count, in order.
fn seeded(count: u32) -> Vec<String> {
    (1..=count).map(|i| format!("msg-{i:04}")).collect()
}

/// The process's peak resident memory, from /proc (Linux only; 0 elsewhere).
fn peak_rss() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .map(|l| l.trim().to_string())
        })
        .unwrap_or_default()
}

fn next_progress(rx: &mut broadcast::Receiver<MailEvent>) -> Option<(SyncProgress, Instant)> {
    loop {
        match rx.try_recv() {
            Ok(MailEvent::SyncProgress { progress, .. }) => {
                return Some((progress, Instant::now()));
            }
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn initial_sync_pages_with_progress() {
    let email = unique("paged");
    seed(&email, 450).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let account = engine.add_imap_account(input(&email)).await.unwrap();
    let mut rx = engine.subscribe();

    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(180), engine.sync(&account.id))
        .await
        .expect("sync must not hang")
        .unwrap();
    let took = started.elapsed();

    // Progress arrives per page and only grows.
    let mut progress = Vec::new();
    while let Some((p, _)) = next_progress(&mut rx) {
        progress.push(p);
    }
    assert!(
        progress.len() >= 3,
        "one event per page: {progress:?} in {took:?}"
    );
    assert!(
        progress.windows(2).all(|w| w[0].messages <= w[1].messages),
        "messages so far never go back: {progress:?}"
    );
    assert_eq!(progress.last().unwrap().messages, 450);
    let total = progress.last().unwrap().folders_total;
    assert!(total >= 1);
    assert_eq!(progress.last().unwrap().folders_done, total);
    assert!(
        engine.progress().is_none(),
        "progress is cleared at the end"
    );

    let mut got = subjects(&engine, &account).await;
    got.sort();
    assert_eq!(got, seeded(450));
    let hits = engine.search("msg-0042", 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].subject, "msg-0042");
    println!(
        "initial sync of 450 messages: {took:?}, {} pages",
        progress.len()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sync_resumes_after_cancel() {
    let email = unique("resume");
    seed(&email, SEED).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let account = engine.add_imap_account(input(&email)).await.unwrap();
    let mut rx = engine.subscribe();

    // Stop the first sync while it is still writing pages.
    let first = engine.clone();
    let id = account.id.clone();
    let task = tokio::spawn(async move { first.sync(&id).await });
    let mut pages = 0;
    while pages < 1 {
        let event = tokio::time::timeout(Duration::from_secs(60), rx.recv())
            .await
            .expect("progress")
            .unwrap();
        if matches!(event, MailEvent::SyncProgress { .. }) {
            pages += 1;
        }
    }
    task.abort();
    let _ = task.await;

    // The next sync continues from the committed page instead of starting over.
    tokio::time::timeout(Duration::from_secs(180), engine.sync(&account.id))
        .await
        .expect("resumed sync must not hang")
        .unwrap();
    let mut got = subjects(&engine, &account).await;
    got.sort();
    assert_eq!(
        got,
        seeded(SEED)[(SEED - WINDOW) as usize..],
        "the newest {WINDOW} messages, no gap and nothing twice"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn backfill_pages_older_mail_into_search() {
    let email = unique("backfill");
    seed(&email, SEED).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let account = engine.add_imap_account(input(&email)).await.unwrap();
    engine.sync(&account.id).await.unwrap();
    assert_eq!(subjects(&engine, &account).await.len(), WINDOW as usize);
    assert!(
        engine
            .search("body of message 0001", 10)
            .await
            .unwrap()
            .is_empty(),
        "older mail is not stored yet"
    );

    let mut rx = engine.subscribe();
    let first = engine
        .backfill(Some(&account.id), FolderKind::Inbox)
        .await
        .unwrap();
    assert_eq!(first.inserted, PAGE);
    assert!(first.pending);
    assert!(matches!(rx.try_recv(), Ok(MailEvent::Changed { .. })));

    let second = engine
        .backfill(Some(&account.id), FolderKind::Inbox)
        .await
        .unwrap();
    assert_eq!(second.inserted, PAGE);
    assert!(!second.pending);
    let done = engine
        .backfill(Some(&account.id), FolderKind::Inbox)
        .await
        .unwrap();
    assert_eq!((done.inserted, done.pending), (0, false));

    let mut got = subjects(&engine, &account).await;
    got.sort();
    assert_eq!(got, seeded(SEED));
    let hits = engine.search("body of message 0001", 10).await.unwrap();
    assert_eq!(hits.len(), 1, "search now covers older mail");
    assert_eq!(hits[0].subject, "msg-0001");
}

/// `cargo test -p mail-core --features insecure-test-tls -- --ignored bench_5000 --nocapture`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "benchmark: seeds 5,000 messages"]
async fn bench_5000() {
    let count: u32 = std::env::var("SEED_COUNT")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(5000);
    let email = unique("bench");
    seed(&email, count).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let account = engine.add_imap_account(input(&email)).await.unwrap();
    let mut rx = engine.subscribe();

    let started = Instant::now();
    let mut first_page = None;
    let mut pages = 0;
    let sync = tokio::spawn({
        let engine = engine.clone();
        let id = account.id.clone();
        async move { engine.sync(&id).await }
    });
    while let Ok(event) = rx.recv().await {
        if let MailEvent::SyncProgress { progress, .. } = event {
            pages += 1;
            first_page.get_or_insert(started.elapsed());
            if progress.messages >= WINDOW || count <= WINDOW {
                break;
            }
        }
    }
    tokio::time::timeout(Duration::from_secs(300), sync)
        .await
        .expect("sync")
        .unwrap()
        .unwrap();
    let initial = started.elapsed();
    println!(
        "{count} messages: first page {first_page:?}, first {WINDOW} after {initial:?}, {pages} pages, peak {}",
        peak_rss()
    );

    if std::env::var("SYNC_ONLY").is_ok() {
        return;
    }

    let started = Instant::now();
    let mut more = 0;
    while engine
        .backfill(Some(&account.id), FolderKind::Inbox)
        .await
        .unwrap()
        .pending
    {
        more += 1;
    }
    println!(
        "backfill {} more pages in {:?} ({:?} per page)",
        more,
        started.elapsed(),
        started.elapsed() / (more + 1)
    );
    assert_eq!(subjects(&engine, &account).await.len(), count as usize);
}
