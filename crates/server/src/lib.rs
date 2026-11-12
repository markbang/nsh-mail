pub mod api;
pub mod config;
pub mod crypto;
pub mod db;
pub mod events;
pub mod mail;
pub mod oauth;
pub mod push;
pub mod types;

use std::{sync::Arc, time::Instant};

use anyhow::{Context, Result};
use axum::{
    Router,
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
use mail::{MailBackend, Mailboxes};
use push::{Fcm, Push};
use types::MailEvent;

pub struct AppState {
    pub config: Config,
    pub db: Db,
    pub mailboxes: Mailboxes,
    pub push: Arc<Push>,
    pub http: reqwest::Client,
    pub vapid_public_key: String,
}

/// Opens storage, loads push credentials and wires mail events to WebSocket clients and push.
pub async fn build(config: Config, backend: Arc<dyn MailBackend>) -> Result<Arc<AppState>> {
    let users_dir = config.data_dir.join("users");
    std::fs::create_dir_all(&users_dir)
        .with_context(|| format!("cannot create data directory {}", users_dir.display()))?;
    let db = Db::open(&config.data_dir.join("server.sqlite"))?;
    let cipher = crypto::Cipher::new(&config.master_key);
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
    let push = Arc::new(Push {
        db: db.clone(),
        http: http.clone(),
        vapid,
        vapid_subject: config.vapid_subject.clone(),
        fcm,
        presence: Default::default(),
    });
    let mailboxes = Mailboxes::new(backend, db.clone(), cipher, users_dir);
    let push_for_events = push.clone();
    mailboxes.on_open(Box::new(move |user_id, mail| {
        let push = push_for_events.clone();
        let mut rx = mail.subscribe();
        tokio::spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                match rx.recv().await {
                    Ok(MailEvent::NewMail { summary }) => {
                        let push = push.clone();
                        tokio::spawn(async move { push.new_mail(user_id, &summary).await });
                    }
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }));
    let state = Arc::new(AppState {
        vapid_public_key,
        config,
        db,
        mailboxes,
        push,
        http,
    });
    for user_id in state.db.users_with_secrets()? {
        if let Err(e) = state.mailboxes.get(user_id).await {
            tracing::error!(user_id, "cannot open mail store: {e:#}");
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
        .route("/accounts/{id}", delete(api::delete_account))
        .route("/oauth/{provider}/start", get(oauth::start))
        .route("/oauth/{provider}/callback", get(oauth::callback))
        .route("/messages", get(api::list_messages))
        .route(
            "/messages/{id}",
            get(api::get_message).patch(api::patch_message),
        )
        .route("/send", post(api::send))
        .route("/search", get(api::search))
        .route("/devices", post(api::add_device))
        .route("/devices/{id}", delete(api::delete_device))
        .route("/push/vapid-key", get(api::vapid_key))
        .route("/events", get(events::events));

    let mut app = Router::new()
        .nest("/v1", api)
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
