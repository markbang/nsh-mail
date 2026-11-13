//! A server whose certificate comes from a private CA that only the OS trust store knows, with
//! every certificate check on. `private-ca.sh` makes the CA, starts GreenMail with its
//! certificate and runs this twice: with the CA in `SSL_CERT_FILE` (`PRIVATE_CA_TRUSTED=1`) the
//! account is added, without it the login fails on the certificate.

use std::sync::Arc;

use mail_core::secrets::MemoryStore;
use mail_core::*;

#[tokio::test]
#[ignore = "needs the GreenMail that tests/private-ca.sh starts"]
async fn private_ca_from_the_os_trust_store() {
    let trusted = std::env::var("PRIVATE_CA_TRUSTED").expect("run by tests/private-ca.sh") == "1";
    let endpoint = |port| Endpoint {
        host: "localhost".into(),
        port,
        security: Security::Tls,
    };
    let dir = tempfile::tempdir().unwrap();
    let engine =
        Engine::open(dir.path().join("mail.sqlite"), Arc::new(MemoryStore::new())).unwrap();
    let added = engine
        .add_imap_account(ImapInput {
            email: "ca@test.local".into(),
            name: None,
            username: None,
            password: "secret".into(),
            imap: endpoint(4993),
            smtp: endpoint(4465),
            provider: None,
        })
        .await;
    if trusted {
        added.expect("the OS trust store's CA verifies the server");
    } else {
        let e = added.expect_err("a CA outside the trust stores must be refused");
        assert_eq!(e.code, ErrorCode::Tls, "{e:?}");
    }
}
