//! Local mode end to end, through the UI, against GreenMail (IMAPS 3993,
//! SMTPS 3465, self-signed, any login). Runs when `GREENMAIL_HOST` is set:
//! `java -Dgreenmail.setup.test.all -Dgreenmail.auth.disabled -jar greenmail-standalone.jar`,
//! then `GREENMAIL_HOST=127.0.0.1 cargo test --test greenmail`.

use std::{path::PathBuf, sync::Arc, time::Duration};

use mail_core::{Engine, secrets::MemoryStore, types::*};
use nsh_core::{
    element::Registry,
    geometry::{Points, Size},
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

/// Alice writes to `to` from an engine of her own.
fn deliver(host: &str, to: &str, subject: &str) {
    let dir = scratch("alice");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let engine = Engine::open(dir.join("mail.sqlite"), Arc::new(MemoryStore::new())).unwrap();
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
        engine
            .send(Draft {
                account_id: alice.id,
                to: vec![Address {
                    name: None,
                    email: to.into(),
                }],
                subject: subject.into(),
                text: "Shall we try the noodle place at noon?".into(),
                ..Draft::default()
            })
            .await
            .unwrap();
    });
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

    // The reply is in Sent, and search finds it.
    step(&mut app, |a| a.press("text:Sent"));
    step(&mut app, |a| {
        a.wait_for_within("id:messages >> text:Re: Lunch on Friday?", WAIT)
    });
    step(&mut app, |a| a.type_text("id:search", "Lunch"));
    step(&mut app, |a| {
        a.wait_for_within("id:results >> text:Re: Lunch on Friday?", WAIT)
    });
}
