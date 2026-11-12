//! Real IMAP/SMTP against GreenMail (IMAPS 3993, SMTPS 3465, self-signed cert, auth disabled).
//! Local: `java -Dgreenmail.setup.test.all -Dgreenmail.auth.disabled -jar greenmail-standalone.jar`
//! then `cargo test -p mail-core --features insecure-test-tls`.

use std::sync::Arc;
use std::time::Duration;

use mail_core::secrets::MemoryStore;
use mail_core::*;
use tokio::sync::broadcast;

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

fn to(email: &str) -> Vec<Address> {
    vec![Address {
        name: None,
        email: email.into(),
    }]
}

fn draft(from: &Account, recipient: &str, subject: &str, text: &str) -> Draft {
    Draft {
        account_id: from.id.clone(),
        to: to(recipient),
        subject: subject.into(),
        text: text.into(),
        ..Default::default()
    }
}

fn query(folder: FolderKind, account: &Account, before: Option<String>, limit: u32) -> ListQuery {
    ListQuery {
        folder,
        account: Some(account.id.clone()),
        before,
        limit,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sync_send_search_flags_and_pagination() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let (alice_email, bob_email) = (unique("alice"), unique("bob"));
    let bob = engine.add_imap_account(input(&bob_email)).await.unwrap();

    for i in 0..3 {
        let mut d = draft(
            &bob,
            &alice_email,
            &format!("hello {i}"),
            &format!("body {i} 季度发票"),
        );
        if i == 0 {
            d.html = Some(
                r#"<p>hi<script>alert(1)</script><img src="https://tracker.example/p.gif"></p>"#
                    .into(),
            );
        }
        engine.send(d).await.unwrap();
    }
    // GreenMail has no Sent mailbox: the engine creates it and appends a copy.
    let sent = engine
        .list(query(FolderKind::Sent, &bob, None, 10))
        .await
        .unwrap();
    assert_eq!(sent.len(), 3, "copies in Sent: {sent:?}");

    let alice = engine.add_imap_account(input(&alice_email)).await.unwrap();
    engine.sync(&alice.id).await.unwrap();

    let page1 = engine
        .list(query(FolderKind::Inbox, &alice, None, 2))
        .await
        .unwrap();
    let subjects: Vec<_> = page1.iter().map(|s| s.subject.as_str()).collect();
    assert_eq!(subjects, ["hello 2", "hello 1"]);
    let page2 = engine
        .list(query(
            FolderKind::Inbox,
            &alice,
            Some(page1[1].id.clone()),
            2,
        ))
        .await
        .unwrap();
    assert_eq!(
        page2.iter().map(|s| s.subject.as_str()).collect::<Vec<_>>(),
        ["hello 0"]
    );
    assert!(
        page2[0].snippet.contains("body 0"),
        "snippet: {:?}",
        page2[0].snippet
    );
    assert_eq!(page2[0].from.as_ref().unwrap().email, bob_email);

    let hits = engine.search("发票", 50).await.unwrap();
    assert_eq!(hits.iter().filter(|h| h.account_id == alice.id).count(), 3);
    let hits = engine.search("hello 1", 50).await.unwrap();
    assert!(
        hits.iter()
            .any(|h| h.account_id == alice.id && h.subject == "hello 1")
    );

    let first = engine.get(&page2[0].id).await.unwrap();
    assert!(first.text.as_deref().unwrap().contains("body 0"));
    let html = first.html.as_deref().unwrap();
    assert!(
        !html.contains("script") && !html.contains("tracker.example"),
        "html: {html}"
    );
    assert_eq!(first.remote_images, ["https://tracker.example/p.gif"]);

    // Flags round-trip through the server: a second engine with an empty database sees them.
    engine
        .set_flags(
            &page1[1].id,
            Flags {
                seen: Some(true),
                flagged: Some(true),
            },
        )
        .await
        .unwrap();
    let dir2 = tempfile::tempdir().unwrap();
    let other = engine_with_account(&dir2, &alice_email).await;
    let (other_engine, other_alice) = (&other.0, &other.1);
    let seen = other_engine
        .list(query(FolderKind::Inbox, other_alice, None, 10))
        .await
        .unwrap();
    let h1 = seen.iter().find(|s| s.subject == "hello 1").unwrap();
    assert!(h1.seen && h1.flagged);
    assert!(
        seen.iter()
            .filter(|s| s.subject != "hello 1")
            .all(|s| !s.seen && !s.flagged)
    );

    // And a change made elsewhere comes back down on the next sync.
    other_engine
        .set_flags(
            &h1.id,
            Flags {
                seen: Some(false),
                flagged: None,
            },
        )
        .await
        .unwrap();
    engine.sync(&alice.id).await.unwrap();
    let h1 = engine
        .list(query(FolderKind::Inbox, &alice, None, 10))
        .await
        .unwrap();
    let h1 = h1.iter().find(|s| s.subject == "hello 1").unwrap();
    assert!(!h1.seen && h1.flagged);

    // Reply threading headers survive the trip.
    let mut reply = draft(&alice, &bob_email, "Re: hello 0", "reply");
    reply.in_reply_to = first.message_id.clone();
    reply.references = first.references.clone();
    engine.send(reply).await.unwrap();
    engine.sync(&bob.id).await.unwrap();
    let inbox = engine
        .list(query(FolderKind::Inbox, &bob, None, 10))
        .await
        .unwrap();
    let got = engine.get(&inbox[0].id).await.unwrap();
    assert_eq!(got.summary.subject, "Re: hello 0");
    assert_eq!(got.in_reply_to, first.message_id);
    assert!(first.message_id.is_some());

    engine.remove_account(&alice.id).await.unwrap();
    assert!(
        engine
            .accounts()
            .await
            .unwrap()
            .iter()
            .all(|a| a.id != alice.id)
    );
    assert!(
        engine
            .list(query(FolderKind::Inbox, &alice, None, 10))
            .await
            .unwrap()
            .is_empty()
    );
}

async fn engine_with_account(dir: &tempfile::TempDir, email: &str) -> (Engine, Account) {
    let e = engine(dir);
    let a = e.add_imap_account(input(email)).await.unwrap();
    e.sync(&a.id).await.unwrap();
    (e, a)
}

async fn next_event(
    rx: &mut broadcast::Receiver<MailEvent>,
    pred: impl Fn(&MailEvent) -> bool,
) -> MailEvent {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match rx.recv().await {
                Ok(e) if pred(&e) => return e,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(e) => panic!("event channel closed: {e}"),
            }
        }
    })
    .await
    .expect("timed out waiting for event")
}

#[tokio::test(flavor = "multi_thread")]
async fn idle_reports_new_mail() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let (alice_email, bob_email) = (unique("alice"), unique("bob"));
    let bob = engine.add_imap_account(input(&bob_email)).await.unwrap();
    engine
        .send(draft(&bob, &alice_email, "before", "creates the mailbox"))
        .await
        .unwrap();
    let alice = engine.add_imap_account(input(&alice_email)).await.unwrap();

    let mut rx = engine.subscribe();
    engine.run().await.unwrap();
    let id = alice.id.clone();
    next_event(&mut rx, |e| {
        matches!(e, MailEvent::SyncState { account_id, syncing: false, error: None } if *account_id == id)
    })
    .await;
    let initial = engine
        .list(query(FolderKind::Inbox, &alice, None, 10))
        .await
        .unwrap();
    assert_eq!(initial.len(), 1, "initial sync is not reported as new mail");

    engine
        .send(draft(&bob, &alice_email, "pushed", "via IDLE"))
        .await
        .unwrap();
    let event = next_event(&mut rx, |e| matches!(e, MailEvent::NewMail { .. })).await;
    let MailEvent::NewMail { summary } = event else {
        unreachable!()
    };
    assert_eq!(summary.account_id, alice.id);
    assert_eq!(summary.subject, "pushed");
    engine.stop();
}

/// Over `FULL_FETCH_MAX`: synced as header + first text part, body fetched when opened.
#[tokio::test(flavor = "multi_thread")]
async fn large_message_is_fetched_on_open() {
    use lettre::message::{Attachment, MultiPart, SinglePart, header::ContentType};
    use lettre::transport::smtp::client::{Tls, TlsParameters};
    use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let alice_email = unique("alice");
    let message = lettre::Message::builder()
        .from("Carol <carol@test.local>".parse().unwrap())
        .to(alice_email.parse().unwrap())
        .subject("quarterly report")
        .multipart(
            MultiPart::mixed()
                .singlepart(SinglePart::plain("See the attached report.".to_string()))
                .singlepart(Attachment::new("report.bin".into()).body(
                    vec![7u8; 64 * 1024],
                    ContentType::parse("application/octet-stream").unwrap(),
                )),
        )
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

    let alice = engine.add_imap_account(input(&alice_email)).await.unwrap();
    engine.sync(&alice.id).await.unwrap();
    let inbox = engine
        .list(query(FolderKind::Inbox, &alice, None, 10))
        .await
        .unwrap();
    assert_eq!(inbox.len(), 1);
    let s = &inbox[0];
    assert_eq!(s.snippet, "See the attached report.");
    assert!(s.has_attachments);
    assert_eq!(s.from.as_ref().unwrap().name.as_deref(), Some("Carol"));

    let m = engine.get(&s.id).await.unwrap();
    assert_eq!(
        m.text.as_deref().map(str::trim),
        Some("See the attached report.")
    );
    assert_eq!(m.attachments.len(), 1);
    assert_eq!(m.attachments[0].filename.as_deref(), Some("report.bin"));
    assert_eq!(m.attachments[0].size, 64 * 1024);
}
