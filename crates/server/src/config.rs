use std::{env, net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

#[derive(Clone)]
pub struct OAuthClient {
    pub id: String,
    pub secret: String,
}

#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub public_url: String,
    pub master_key: [u8; 32],
    pub web_dir: Option<PathBuf>,
    pub cors_origins: Vec<String>,
    pub allow_signup: bool,
    pub google: Option<OAuthClient>,
    pub microsoft: Option<OAuthClient>,
    pub fcm_service_account: Option<PathBuf>,
    /// Overridable so tests can point FCM at a local mock.
    pub fcm_base_url: String,
    pub vapid_private_key: Option<String>,
    pub vapid_subject: String,
}

fn var(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub fn parse_master_key(value: &str) -> Result<[u8; 32]> {
    let bytes = STANDARD
        .decode(value.trim())
        .context("NSHMAIL_MASTER_KEY is not valid base64")?;
    bytes.try_into().map_err(|_| {
        anyhow::anyhow!("NSHMAIL_MASTER_KEY must decode to exactly 32 bytes (generate one with `openssl rand -base64 32`)")
    })
}

fn oauth_client(id: &str, secret: &str) -> Result<Option<OAuthClient>> {
    match (var(id), var(secret)) {
        (Some(id), Some(secret)) => Ok(Some(OAuthClient { id, secret })),
        (None, None) => Ok(None),
        _ => bail!("set both {id} and {secret}, or neither"),
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let Some(key) = var("NSHMAIL_MASTER_KEY") else {
            bail!(
                "NSHMAIL_MASTER_KEY is not set. It encrypts stored mail credentials; \
                 generate one with `openssl rand -base64 32`, keep it outside the data directory, \
                 and never change it after accounts were added"
            );
        };
        let bind = var("NSHMAIL_BIND")
            .unwrap_or_else(|| "0.0.0.0:8080".into())
            .parse()
            .context("NSHMAIL_BIND must be host:port, e.g. 0.0.0.0:8080")?;
        let public_url = var("NSHMAIL_PUBLIC_URL")
            .unwrap_or_else(|| "http://localhost:8080".into())
            .trim_end_matches('/')
            .to_string();
        let allow_signup = match var("NSHMAIL_ALLOW_SIGNUP").as_deref() {
            None | Some("true" | "1") => true,
            Some("false" | "0") => false,
            Some(other) => bail!("NSHMAIL_ALLOW_SIGNUP must be true or false, got {other:?}"),
        };
        Ok(Config {
            bind,
            data_dir: var("NSHMAIL_DATA_DIR")
                .unwrap_or_else(|| "data".into())
                .into(),
            master_key: parse_master_key(&key)?,
            web_dir: var("NSHMAIL_WEB_DIR").map(PathBuf::from),
            cors_origins: var("NSHMAIL_CORS_ORIGINS")
                .map(|v| {
                    v.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            allow_signup,
            google: oauth_client("NSHMAIL_GOOGLE_CLIENT_ID", "NSHMAIL_GOOGLE_CLIENT_SECRET")?,
            microsoft: oauth_client("NSHMAIL_MS_CLIENT_ID", "NSHMAIL_MS_CLIENT_SECRET")?,
            fcm_service_account: var("NSHMAIL_FCM_SERVICE_ACCOUNT").map(PathBuf::from),
            fcm_base_url: "https://fcm.googleapis.com".into(),
            vapid_private_key: var("NSHMAIL_VAPID_PRIVATE_KEY"),
            vapid_subject: var("NSHMAIL_VAPID_SUBJECT").unwrap_or_else(|| public_url.clone()),
            public_url,
        })
    }
}
