use std::{
    collections::HashMap,
    path::Path,
    sync::Mutex as StdMutex,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use web_push_native::{
    Auth, WebPushBuilder,
    jwt_simple::{
        algorithms::{ECDSAP256PublicKeyLike, ES256KeyPair, RS256KeyPair, RSAKeyPairLike},
        claims::Claims,
        prelude::Duration as JwtDuration,
    },
    p256::PublicKey,
};

use crate::{
    db::{Db, Device},
    types::Summary,
};

/// Loads the VAPID key from env, or from `<data>/vapid_private_key`, generating it on first start.
pub fn load_vapid(env_key: Option<&str>, data_dir: &Path) -> Result<ES256KeyPair> {
    let decode = |s: &str| -> Result<ES256KeyPair> {
        let raw = URL_SAFE_NO_PAD
            .decode(s.trim())
            .context("VAPID private key is not base64url")?;
        ES256KeyPair::from_bytes(&raw).map_err(|e| anyhow!("invalid VAPID private key: {e}"))
    };
    if let Some(k) = env_key {
        return decode(k).context("NSHMAIL_VAPID_PRIVATE_KEY");
    }
    let path = data_dir.join("vapid_private_key");
    if path.exists() {
        return decode(&std::fs::read_to_string(&path)?)
            .with_context(|| path.display().to_string());
    }
    let kp = ES256KeyPair::generate();
    write_private(&path, &URL_SAFE_NO_PAD.encode(kp.to_bytes()))?;
    tracing::info!("generated VAPID key at {}", path.display());
    Ok(kp)
}

fn write_private(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    opts.open(path)?.write_all(contents.as_bytes())?;
    Ok(())
}

pub fn vapid_public_key(kp: &ES256KeyPair) -> String {
    URL_SAFE_NO_PAD.encode(kp.public_key().public_key().to_bytes_uncompressed())
}

#[derive(Deserialize)]
struct ServiceAccount {
    project_id: String,
    client_email: String,
    private_key: String,
    #[serde(default = "default_token_uri")]
    token_uri: String,
}

fn default_token_uri() -> String {
    "https://oauth2.googleapis.com/token".into()
}

pub struct Fcm {
    sa: ServiceAccount,
    key: RS256KeyPair,
    base_url: String,
    token: Mutex<Option<(String, Instant)>>,
}

impl Fcm {
    pub fn load(path: &Path, base_url: String) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read FCM service account {}", path.display()))?;
        let sa: ServiceAccount = serde_json::from_str(&raw).with_context(|| {
            format!("{} is not a Firebase service account JSON", path.display())
        })?;
        let key = RS256KeyPair::from_pem(&sa.private_key)
            .map_err(|e| anyhow!("service account private_key: {e}"))?;
        Ok(Fcm {
            sa,
            key,
            base_url,
            token: Mutex::new(None),
        })
    }

    async fn access_token(&self, http: &reqwest::Client) -> Result<String> {
        let mut cached = self.token.lock().await;
        if let Some((t, exp)) = cached.as_ref()
            && Instant::now() < *exp
        {
            return Ok(t.clone());
        }
        let claims = Claims::with_custom_claims(
            json!({ "scope": "https://www.googleapis.com/auth/firebase.messaging" }),
            JwtDuration::from_hours(1),
        )
        .with_issuer(&self.sa.client_email)
        .with_audience(&self.sa.token_uri);
        let assertion = self
            .key
            .sign(claims)
            .map_err(|e| anyhow!("signing FCM JWT: {e}"))?;
        let res = http
            .post(&self.sa.token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &assertion),
            ])
            .send()
            .await?;
        if !res.status().is_success() {
            bail!(
                "FCM token exchange failed: {} {}",
                res.status(),
                res.text().await.unwrap_or_default()
            );
        }
        #[derive(Deserialize)]
        struct Token {
            access_token: String,
            expires_in: u64,
        }
        let t: Token = res.json().await?;
        let exp = Instant::now() + Duration::from_secs(t.expires_in.saturating_sub(60));
        *cached = Some((t.access_token.clone(), exp));
        Ok(t.access_token)
    }

    /// Ok(false) when FCM says the token is gone.
    async fn send(&self, http: &reqwest::Client, token: &str, data: &Value) -> Result<bool> {
        let url = format!(
            "{}/v1/projects/{}/messages:send",
            self.base_url, self.sa.project_id
        );
        let body = json!({ "message": { "token": token, "data": data, "android": { "priority": "high" } } });
        let res = http
            .post(url)
            .bearer_auth(self.access_token(http).await?)
            .json(&body)
            .send()
            .await?;
        let status = res.status();
        if status.is_success() {
            return Ok(true);
        }
        let text = res.text().await.unwrap_or_default();
        if status == 404 || status == 410 || text.contains("UNREGISTERED") {
            return Ok(false);
        }
        bail!("FCM send failed: {status} {text}")
    }
}

/// Live WebSocket connections per (user, device), so online devices are not pushed twice.
#[derive(Default)]
pub struct Presence(StdMutex<HashMap<(i64, String), usize>>);

impl Presence {
    pub fn enter(&self, user: i64, device: &str) {
        *self
            .0
            .lock()
            .unwrap()
            .entry((user, device.into()))
            .or_default() += 1;
    }
    pub fn leave(&self, user: i64, device: &str) {
        let mut m = self.0.lock().unwrap();
        if let Some(n) = m.get_mut(&(user, device.into())) {
            *n -= 1;
            if *n == 0 {
                m.remove(&(user, device.into()));
            }
        }
    }
    fn online(&self, user: i64, device: &str) -> bool {
        self.0.lock().unwrap().contains_key(&(user, device.into()))
    }
}

pub struct Push {
    pub db: Db,
    pub http: reqwest::Client,
    pub vapid: ES256KeyPair,
    pub vapid_subject: String,
    pub fcm: Option<Fcm>,
    pub presence: Presence,
}

impl Push {
    pub async fn new_mail(&self, user_id: i64, summary: &Summary) {
        let (devices, hide) = match (self.db.devices(user_id), self.db.user(user_id)) {
            (Ok(d), Ok(Some(u))) => (d, u.hide_content),
            (Err(e), _) | (_, Err(e)) => return tracing::error!("push: loading devices: {e:#}"),
            _ => return,
        };
        let mut data =
            json!({ "type": "newMail", "account": summary.account_id, "id": summary.id });
        if !hide {
            if let Some(from) = &summary.from {
                data["from"] = from
                    .name
                    .clone()
                    .unwrap_or_else(|| from.email.clone())
                    .into();
            }
            data["subject"] = summary.subject.clone().into();
        }
        for d in devices
            .iter()
            .filter(|d| !self.presence.online(user_id, &d.id))
        {
            match self.send(d, &data).await {
                Ok(true) => {}
                Ok(false) => {
                    tracing::info!(device = %d.id, "push endpoint gone, removing device");
                    let _ = self.db.delete_device(user_id, &d.id);
                }
                Err(e) => tracing::warn!(device = %d.id, "push failed: {e:#}"),
            }
        }
    }

    async fn send(&self, d: &Device, data: &Value) -> Result<bool> {
        match d.kind.as_str() {
            "fcm" => match &self.fcm {
                Some(fcm) => fcm.send(&self.http, &d.token, data).await,
                None => Ok(true),
            },
            "webpush" => self.web_push(d, data).await,
            k => bail!("unknown device kind {k}"),
        }
    }

    async fn web_push(&self, d: &Device, data: &Value) -> Result<bool> {
        let (ua_public, ua_auth) = parse_subscription_keys(
            d.p256dh.as_deref().unwrap_or_default(),
            d.auth.as_deref().unwrap_or_default(),
        )?;
        let req = WebPushBuilder::new(d.token.parse()?, ua_public, ua_auth)
            .with_vapid(&self.vapid, &self.vapid_subject)
            .build(data.to_string())
            .map_err(|e| anyhow!("encrypting web push: {e}"))?;
        let (parts, body) = req.into_parts();
        let res = self
            .http
            .post(parts.uri.to_string())
            .headers(parts.headers)
            .header("Urgency", "high")
            .body(body)
            .send()
            .await?;
        let status = res.status();
        if status == 404 || status == 410 {
            return Ok(false);
        }
        if !status.is_success() {
            bail!(
                "web push failed: {status} {}",
                res.text().await.unwrap_or_default()
            );
        }
        Ok(true)
    }
}

pub fn parse_subscription_keys(p256dh: &str, auth: &str) -> Result<(PublicKey, Auth)> {
    let pk = URL_SAFE_NO_PAD
        .decode(p256dh.trim_end_matches('='))
        .context("p256dh is not base64url")?;
    let pk =
        PublicKey::from_sec1_bytes(&pk).map_err(|_| anyhow!("p256dh is not a P-256 public key"))?;
    let auth = URL_SAFE_NO_PAD
        .decode(auth.trim_end_matches('='))
        .context("auth is not base64url")?;
    if auth.len() != 16 {
        bail!("auth must be 16 bytes");
    }
    Ok((pk, Auth::clone_from_slice(&auth)))
}
