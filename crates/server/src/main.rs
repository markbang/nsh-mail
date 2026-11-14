use nsh_mail_server::{Config, build, router};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // The image is distroless (no shell, no curl): Docker's HEALTHCHECK runs the binary itself.
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        std::process::exit(healthcheck().await);
    }
    // Recreates the notification feed from the mail tables; mail is only read.
    if std::env::args().nth(1).as_deref() == Some("rebuild-feed") {
        std::process::exit(rebuild_feed());
    }
    // Writes the OpenAPI document to stdout; CI keeps docs/openapi.json in step with it.
    if std::env::args().nth(1).as_deref() == Some("openapi") {
        println!("{}", nsh_mail_server::agent::openapi_json());
        return;
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    if let Err(e) = run().await {
        eprintln!("nsh-mail-server: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    #[cfg(feature = "insecure-test-tls")]
    {
        tracing::warn!("insecure-test-tls: mail server certificates are not checked");
        mail_core::accept_invalid_certs_for_tests();
    }
    let config = Config::from_env()?;
    let bind = config.bind;
    let state = build(config).await?;
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| anyhow::anyhow!("cannot listen on {bind}: {e}"))?;
    tracing::info!("listening on {bind}");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

/// `rebuild-feed`: the projection is derived data, so this is the repair for a feed that is wrong
/// or was written by a buggy version. Counts go to stdout as JSON for an operator to keep.
fn rebuild_feed() -> i32 {
    let dir = std::env::var("NSHMAIL_DATA_DIR").unwrap_or_else(|_| "data".into());
    let path = std::path::Path::new(&dir).join("server.sqlite");
    let counts = mail_core::MailDb::open_unmigrated(&path).and_then(|db| db.rebuild_feed());
    match counts {
        Ok(c) => {
            println!("{}", serde_json::to_string(&c).unwrap_or_default());
            0
        }
        Err(e) => {
            eprintln!("nsh-mail-server rebuild-feed: {}", e.message);
            1
        }
    }
}

async fn healthcheck() -> i32 {
    let port = std::env::var("NSHMAIL_BIND")
        .ok()
        .and_then(|b| b.parse::<std::net::SocketAddr>().ok())
        .map_or(8080, |a| a.port());
    match reqwest::get(format!("http://127.0.0.1:{port}/healthz")).await {
        Ok(r) if r.status().is_success() => 0,
        Ok(r) => {
            eprintln!("healthz: {}", r.status());
            1
        }
        Err(e) => {
            eprintln!("healthz: {e}");
            1
        }
    }
}

async fn shutdown() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        tokio::select! { _ = ctrl_c => {}, _ = term.recv() => {} }
    }
    #[cfg(not(unix))]
    let _ = ctrl_c.await;
}
