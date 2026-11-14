pub mod api;
pub mod config;
pub mod crypto;
pub mod db;
pub mod events;
pub mod ingest;
pub mod mail;
pub mod notify;
pub mod oauth;
pub mod push;
pub mod rotate;
pub mod uploads;

use std::{sync::Arc, time::Instant};

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    extract::Request,
    http::{HeaderValue, Method, header},
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, post},
};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
};

pub use config::Config;
use db::Db;
use mail::Mailboxes;
use mail_core::MailDb;
use push::{Fcm, Push};

pub struct AppState {
    pub config: Config,
    pub db: Db,
    pub mail_db: MailDb,
    pub mailboxes: Mailboxes,
    pub push: Arc<Push>,
    pub http: reqwest::Client,
    pub vapid_public_key: String,
    /// Webhook secrets for notification sources, AES-256-GCM under the master key. It lives in
    /// `users/` as `*.secrets.json` so `rotate_secrets` re-encrypts it with the rest.
    pub notify_secrets: Arc<mail_core::secrets::EncryptedFileStore>,
    pub rate_limits: ingest::RateLimits,
    pub feed_events: tokio::sync::broadcast::Sender<events::FeedEvent>,
}

/// Opens storage (moving old per-user mail databases in), loads push credentials and starts mail
/// sync for every user with an account.
pub async fn build(config: Config) -> Result<Arc<AppState>> {
    let users_dir = config.data_dir.join("users");
    std::fs::create_dir_all(&users_dir)
        .with_context(|| format!("cannot create data directory {}", users_dir.display()))?;
    let db_path = config.data_dir.join("server.sqlite");
    let db = Db::open(&db_path)?;
    let mail_db = MailDb::open(&db_path)
        .map_err(|e| anyhow::anyhow!("cannot open {}: {}", db_path.display(), e.message))?;
    let dir = users_dir.clone();
    let import_db = mail_db.clone();
    tokio::task::spawn_blocking(move || mail::import_legacy(&import_db, &dir)).await??;
    let (dir, key, old) = (users_dir.clone(), config.master_key, config.master_key_old);
    tokio::task::spawn_blocking(move || rotate::rotate_secrets(&dir, key, old)).await??;
    let http = reqwest::Client::builder()
        .user_agent(concat!("nsh-mail-server/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let vapid = push::load_vapid(config.vapid_private_key.as_deref(), &config.data_dir)?;
    let fcm = match &config.fcm_service_account {
        Some(path) => Some(Fcm::load(path, config.fcm_base_url.clone())?),
        None => None,
    };
    let vapid_public_key = push::vapid_public_key(&vapid);
    let stale = db.drop_stale_webpush(&vapid_public_key)?;
    if stale > 0 {
        tracing::info!(
            devices = stale,
            "dropped Web Push subscriptions made for another VAPID key; browsers subscribe again"
        );
    }
    let push = Arc::new(Push {
        db: db.clone(),
        http: http.clone(),
        vapid,
        vapid_subject: config.vapid_subject.clone(),
        fcm,
        presence: Default::default(),
    });
    let notify_secrets = Arc::new(mail_core::secrets::EncryptedFileStore::with_key(
        users_dir.join("notify.secrets.json"),
        config.master_key,
    ));
    let mailboxes = Mailboxes::new(
        mail_db.clone(),
        users_dir,
        config.master_key,
        config.oauth.clone(),
        config.max_attachments,
        push.clone(),
    );
    uploads::sweep(config.data_dir.join("uploads"));
    let (feed_events, _) = tokio::sync::broadcast::channel(512);
    let state = Arc::new(AppState {
        vapid_public_key,
        config,
        db,
        mail_db,
        mailboxes,
        push,
        http,
        notify_secrets,
        rate_limits: Default::default(),
        feed_events,
    });
    let users = state
        .mailboxes
        .existing_users()
        .map_err(|e| anyhow::anyhow!("cannot list users with mail: {}", e.message))?;
    for user_id in users {
        if let Err(e) = state.mailboxes.get(user_id).await {
            tracing::error!(user_id, "cannot open mail store: {}", e.message);
        }
    }
    Ok(state)
}

pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/auth/signup", post(api::signup))
        .route("/auth/login", post(api::login))
        .route("/auth/logout", post(api::logout))
        .route("/me", get(api::me).patch(api::update_me))
        .route("/accounts", get(api::list_accounts).post(api::add_account))
        .route(
            "/accounts/{id}",
            delete(api::delete_account).patch(api::patch_account),
        )
        .route("/detect", get(api::detect))
        .route("/sync", get(api::sync_status).post(api::sync))
        .route("/sync/older", post(api::older))
        .route("/oauth/{provider}/start", get(oauth::start))
        .route("/oauth/{provider}/callback", get(oauth::callback))
        .route("/messages", get(api::list_messages))
        .route(
            "/messages/{id}",
            get(api::get_message).patch(api::patch_message),
        )
        .route("/messages/{id}/move", post(api::move_message))
        .route(
            "/messages/{id}/attachments/{index}",
            get(api::get_attachment),
        )
        .route("/threads", get(api::list_threads))
        .route(
            "/threads/{id}",
            get(api::get_thread).patch(api::patch_thread),
        )
        .route("/threads/{id}/move", post(api::move_thread))
        .route("/send", post(api::send))
        .route("/outbox", get(api::outbox))
        .route("/outbox/{id}", delete(api::cancel_outgoing))
        .route("/uploads", post(uploads::create))
        .route(
            "/uploads/{id}",
            axum::routing::patch(uploads::append)
                .delete(uploads::delete)
                .layer(DefaultBodyLimit::max(uploads::CHUNK_LIMIT)),
        )
        .route("/search", get(api::search))
        .route("/unread", get(api::unread))
        .route(
            "/sources",
            get(notify::list_sources).post(notify::create_source),
        )
        .route(
            "/sources/{id}",
            get(notify::get_source)
                .patch(notify::update_source)
                .delete(notify::delete_source),
        )
        .route("/sources/presets", get(notify::list_presets))
        .route("/sources/{id}/token", post(notify::rotate_token))
        .route("/sources/{id}/test", post(notify::test_source))
        .route("/devices", post(api::add_device))
        .route("/devices/{id}", delete(api::delete_device))
        .route("/push/vapid-key", get(api::vapid_key))
        .route("/events", get(events::events));

    // Ingestion, outside `/v1` because these URLs are copied into other services: our webhook,
    // Gotify's `POST /message?token=`, and ntfy's `PUT|POST /{topic}`.
    let ingest_limit = DefaultBodyLimit::max(ingest::BODY_LIMIT);
    let mut app = Router::new()
        .nest("/v1", api)
        .route("/hooks/{token}", post(ingest::webhook).layer(ingest_limit))
        .route(
            "/message",
            post(ingest::gotify).layer(DefaultBodyLimit::max(ingest::BODY_LIMIT)),
        )
        .route(
            "/{topic}",
            get(ingest::web_asset)
                .put(ingest::ntfy_publish)
                .post(ingest::ntfy_publish)
                .layer(DefaultBodyLimit::max(ingest::BODY_LIMIT)),
        )
        .route("/{topic}/json", get(ingest::ntfy_stream))
        .route("/healthz", get(api::health));

    if let Some(dir) = &state.config.web_dir {
        let sw = Router::new()
            .route_service("/sw.js", ServeFile::new(dir.join("sw.js")))
            .layer(SetResponseHeaderLayer::overriding(
                header::HeaderName::from_static("service-worker-allowed"),
                HeaderValue::from_static("/"),
            ));
        app = app.merge(sw).fallback_service(ServeDir::new(dir));
    }

    if !state.config.cors_origins.is_empty() {
        let origins: Vec<HeaderValue> = state
            .config
            .cors_origins
            .iter()
            .filter_map(|o| HeaderValue::from_str(o).ok())
            .collect();
        app = app.layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(origins))
                .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        );
    }

    app.layer(middleware::from_fn(log_request))
        .with_state(state)
}

/// Logs the path only: query strings may carry session tokens (`?token=`) or OAuth codes.
async fn log_request(req: Request, next: Next) -> Response {
    let (method, path, start) = (
        req.method().clone(),
        req.uri().path().to_string(),
        Instant::now(),
    );
    let res = next.run(req).await;
    tracing::info!(%method, path, status = res.status().as_u16(), ms = start.elapsed().as_millis() as u64);
    res
}
