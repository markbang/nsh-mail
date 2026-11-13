//! Local mode end to end, through the UI, against GreenMail (IMAPS 3993,
//! SMTPS 3465, self-signed, any login). Runs when `GREENMAIL_HOST` is set:
//! `java -Dgreenmail.setup.test.all -Dgreenmail.auth.disabled -jar greenmail-standalone.jar`,
//! then `GREENMAIL_HOST=127.0.0.1 cargo test --test greenmail`.

use std::{path::PathBuf, sync::Arc, time::Duration};

use mail_core::{Engine, StagedFile, secrets::MemoryStore, types::*};
use nsh_core::{
    PasteEvent, PastedFile,
    element::Registry,
    geometry::{Points, Size},
    image::ImageData,
    picker::PickedFile,
    platform::{Environment, Idiom, Locale, Os},
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

const WAIT: Duration = Duration::from_secs(60);

fn endpoint(host: &str, port: u16) -> Endpoint {
    Endpoint {
        host: host.into(),
        port,
        security: Security::Tls,
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nshmail-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const MENU: &[u8] = b"%PDF-1.4 menu";
const MAP: &[u8] = b"\x89PNG\r\n\x1a\n map";
const REPORT: &[u8] = b"%PDF-1.4 report";
const SHOT: &[u8] = b"\x89PNG\r\n\x1a\n shot";

/// Alice's account on an engine of her own.
async fn alice(host: &str, to: &str) -> (Engine, Account) {
    let engine = Engine::open(
        scratch("alice").join("mail.sqlite"),
        Arc::new(MemoryStore::new()),
    )
    .unwrap();
    let alice = engine
        .add_imap_account(ImapInput {
            email: format!("alice-{to}"),
            name: Some("Alice".into()),
            username: None,
            password: "secret".into(),
            imap: endpoint(host, 3993),
            smtp: endpoint(host, 3465),
            provider: None,
        })
        .await
        .unwrap();
    (engine, alice)
}

fn file(name: &str, mime_type: &str, data: &[u8]) -> StagedFile {
    StagedFile {
        filename: name.into(),
        mime_type: mime_type.into(),
        data: data.to_vec(),
    }
}

/// Alice writes to `to`, with a menu and a map attached.
fn deliver(host: &str, to: &str, subject: &str) {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (engine, alice) = alice(host, to).await;
        let attachments = vec![
            DraftAttachment::Upload { id: "menu".into() },
            DraftAttachment::Upload { id: "map".into() },
        ];
        let draft = Draft {
            account_id: alice.id,
            to: vec![Address {
                name: None,
                email: to.into(),
            }],
            subject: subject.into(),
            text: "Shall we try the noodle place at noon?".into(),
            attachments,
            ..Draft::default()
        };
        let staged = [
            ("menu".into(), file("菜单.pdf", "application/pdf", MENU)),
            ("map".into(), file("map.png", "image/png", MAP)),
        ];
        engine.send_with(draft, staged.into()).await.unwrap();
    });
}

/// The attachments of the message `subject` in Alice's inbox: names and bytes.
fn received(host: &str, to: &str, subject: &str) -> Vec<(String, Vec<u8>)> {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (engine, alice) = alice(host, to).await;
        engine.sync(&alice.id).await.unwrap();
        let inbox = engine
            .list(ListQuery {
                folder: FolderKind::Inbox,
                account: None,
                before: None,
                limit: 20,
            })
            .await
            .unwrap();
        let m = inbox
            .iter()
            .find(|m| m.subject == subject)
            .unwrap_or_else(|| panic!("no {subject:?} in {inbox:?}"));
        let m = engine.get(&m.id).await.unwrap();
        let mut out = Vec::new();
        for a in &m.attachments {
            let (meta, data) = engine.attachment(&m.summary.id, a.index).await.unwrap();
            out.push((meta.filename.unwrap_or_default(), data));
        }
        out
    })
}

fn step(app: &mut Headless, what: impl FnOnce(&mut Headless) -> Result<(), String>) {
    if let Err(e) = what(app) {
        panic!("{e}\non screen:\n{}", app.screen().texts().join("\n"));
    }
}

#[test]
fn local_mode_reads_replies_and_searches_real_mail() {
    let Ok(host) = std::env::var("GREENMAIL_HOST") else {
        eprintln!("skipped: GREENMAIL_HOST is not set");
        return;
    };
    mail_core::accept_invalid_certs_for_tests();
    let bob = format!("bob{}@test.local", std::process::id());
    deliver(&host, &bob, "Lunch on Friday?");

    let ui_sets: Vec<Registry> = vec![nsh_base::registry(), nsh_app_nsh_mail::registry()];
    let entry = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("App.tsx");
    let bundle = nsh_testing::compile_tsx(entry, &ui_sets).expect("App.tsx builds");
    let mut config = EngineConfig {
        ui_sets,
        data_dir: Some(scratch("app")),
        environment: Environment {
            os: Os::Linux,
            idiom: Idiom::Desktop,
            locale: Locale::EN_US,
            ..Environment::default()
        },
        ..EngineConfig::new(bundle)
    };
    config.viewport = Size::new(Points(1280.0), Points(820.0));
    let mut app = Headless::start(config).expect("the engine starts");

    // Onboarding: keep mail on this device.
    step(&mut app, |a| a.wait_for_within("id:onboarding-go", WAIT));
    step(&mut app, |a| a.press("id:onboarding-go"));
    step(&mut app, |a| a.wait_for_within("id:open-settings", WAIT));

    // Add Bob's account with GreenMail's servers.
    step(&mut app, |a| a.press("id:open-settings"));
    step(&mut app, |a| a.press("id:add-account"));
    step(&mut app, |a| a.press("id:add-imap"));
    step(&mut app, |a| a.type_text("id:imap-email", &bob));
    step(&mut app, |a| a.press("id:imap-continue"));
    step(&mut app, |a| a.wait_for_within("id:imap-password", WAIT));
    for (field, value) in [
        ("id:imap-host", host.as_str()),
        ("id:imap-port", "3993"),
        ("id:smtp-host", host.as_str()),
        ("id:smtp-port", "3465"),
        ("id:imap-password", "secret"),
    ] {
        step(&mut app, |a| a.clear(field));
        step(&mut app, |a| a.type_text(field, value));
    }
    step(&mut app, |a| a.press("id:imap-add"));
    step(&mut app, |a| {
        a.wait_for_within(&format!("text:Added {bob}"), WAIT)
    });
    step(&mut app, |a| a.wait_for_gone("id:modal", WAIT));

    // The inbox syncs Alice's message; open it and reply.
    step(&mut app, |a| {
        a.wait_for_within("id:messages >> text:Lunch on Friday?", WAIT)
    });
    step(&mut app, |a| {
        a.press("id:messages >> text:Lunch on Friday?")
    });
    step(&mut app, |a| {
        a.wait_for_within(
            "id:reader >> text:Shall we try the noodle place at noon?",
            WAIT,
        )
    });
    step(&mut app, |a| a.press("id:reply"));
    step(&mut app, |a| a.wait_for_within("id:compose-subject", WAIT));
    step(&mut app, |a| a.press("id:send"));
    step(&mut app, |a| a.wait_for_within("text:Message sent", WAIT));

    // Forward it to Alice: the menu stays, the map goes, a picked report and
    // a pasted screenshot join.
    step(&mut app, |a| a.press("id:forward"));
    step(&mut app, |a| {
        a.wait_for_within("id:compose-attachments >> text:map.png", WAIT)
    });
    step(&mut app, |a| a.press("label:Remove map.png"));
    app.on_pick(|_| {
        vec![PickedFile::new(
            "报告 2026.pdf",
            "application/pdf",
            ImageData::new(REPORT.to_vec()),
        )]
    });
    step(&mut app, |a| a.press("id:compose-attach"));
    let mut paste = PasteEvent::new(String::new());
    paste
        .files
        .push(PastedFile::new("image.png", "image/png", SHOT.to_vec()));
    step(&mut app, |a| a.paste("id:compose-body", &paste));
    step(&mut app, |a| {
        a.wait_for_within("id:compose-attachments >> text:image.png", WAIT)
    });
    step(&mut app, |a| {
        a.type_text("id:compose-to", &format!("alice-{bob}"))
    });
    step(&mut app, |a| a.press("id:send"));
    step(&mut app, |a| a.wait_for_within("text:Message sent", WAIT));
    assert_eq!(
        received(&host, &bob, "Fwd: Lunch on Friday?"),
        vec![
            ("菜单.pdf".to_string(), MENU.to_vec()),
            ("报告 2026.pdf".to_string(), REPORT.to_vec()),
            ("image.png".to_string(), SHOT.to_vec()),
        ]
    );

    // The reply is in Sent, and search finds it.
    step(&mut app, |a| a.press("text:Sent"));
    step(&mut app, |a| {
        a.wait_for_within("id:messages >> text:Re: Lunch on Friday?", WAIT)
    });
    // So is the forward, with what it sent.
    step(&mut app, |a| {
        a.press("id:messages >> text:Fwd: Lunch on Friday?")
    });
    for name in ["菜单.pdf", "报告 2026.pdf", "image.png"] {
        step(&mut app, |a| {
            a.wait_for_within(&format!("id:reader >> text:{name}"), WAIT)
        });
    }
    step(&mut app, |a| a.type_text("id:search", "Lunch"));
    step(&mut app, |a| {
        a.wait_for_within("id:results >> text:Re: Lunch on Friday?", WAIT)
    });
}
