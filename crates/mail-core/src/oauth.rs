//! OAuth 2.0 for Google and Microsoft: authorization code + PKCE, token exchange and refresh, and a
//! loopback redirect listener on `127.0.0.1:<random port>` for native apps. Opening the browser is
//! the caller's job. The server runs its own web redirect with `authorize_url` + `exchange`.

use std::fmt;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::error::err;
use crate::types::*;

#[derive(Clone)]
pub struct OAuthApp {
    pub client_id: String,
    /// Google "desktop app" clients have a (non-confidential) secret; Microsoft public clients none.
    pub client_secret: Option<String>,
}

#[derive(Clone, Default)]
pub struct OAuthApps {
    pub google: Option<OAuthApp>,
    pub microsoft: Option<OAuthApp>,
}

impl OAuthApps {
    /// Client ids baked in at build time: `NSHMAIL_GOOGLE_CLIENT_ID`, `NSHMAIL_GOOGLE_CLIENT_SECRET`,
    /// `NSHMAIL_MS_CLIENT_ID`.
    pub fn builtin() -> Self {
        Self {
            google: option_env!("NSHMAIL_GOOGLE_CLIENT_ID").map(|id| OAuthApp {
                client_id: id.to_string(),
                client_secret: option_env!("NSHMAIL_GOOGLE_CLIENT_SECRET").map(str::to_string),
            }),
            microsoft: option_env!("NSHMAIL_MS_CLIENT_ID").map(|id| OAuthApp {
                client_id: id.to_string(),
                client_secret: None,
            }),
        }
    }

    pub fn get(&self, provider: OAuthProvider) -> Result<&OAuthApp> {
        let (app, var) = match provider {
            OAuthProvider::Google => (&self.google, "NSHMAIL_GOOGLE_CLIENT_ID"),
            OAuthProvider::Microsoft => (&self.microsoft, "NSHMAIL_MS_CLIENT_ID"),
        };
        app.as_ref().ok_or_else(|| {
            err(
                ErrorCode::OAuth,
                format!(
                    "{} sign-in is not available in this build: it was compiled without {var}",
                    provider_name(provider)
                ),
            )
        })
    }
}

fn provider_name(p: OAuthProvider) -> &'static str {
    match p {
        OAuthProvider::Google => "Google",
        OAuthProvider::Microsoft => "Microsoft",
    }
}

struct Endpoints {
    authorize: &'static str,
    token: &'static str,
    scope: &'static str,
}

fn endpoints(p: OAuthProvider) -> Endpoints {
    match p {
        OAuthProvider::Google => Endpoints {
            authorize: "https://accounts.google.com/o/oauth2/v2/auth",
            token: "https://oauth2.googleapis.com/token",
            scope: "https://mail.google.com/ openid email",
        },
        OAuthProvider::Microsoft => Endpoints {
            authorize: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            token: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            scope: "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access openid email",
        },
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix seconds.
    pub expires_at: i64,
}

impl fmt::Debug for OAuthTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthTokens")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// A completed sign-in: who signed in, and the tokens for IMAP/SMTP XOAUTH2.
#[derive(Clone, Debug)]
pub struct OAuthGrant {
    pub provider: OAuthProvider,
    pub email: String,
    pub name: Option<String>,
    pub tokens: OAuthTokens,
}

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn new() -> Self {
        let verifier = random_token(48);
        let challenge = B64URL.encode(sha2::Sha256::digest(verifier.as_bytes()));
        Self {
            verifier,
            challenge,
        }
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

pub fn random_token(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    getrandom::fill(&mut b).expect("OS random source unavailable");
    B64URL.encode(b)
}

pub fn authorize_url(
    provider: OAuthProvider,
    app: &OAuthApp,
    redirect_uri: &str,
    state: &str,
    pkce_challenge: &str,
    login_hint: Option<&str>,
) -> String {
    let e = endpoints(provider);
    let mut params = vec![
        ("client_id", app.client_id.as_str()),
        ("response_type", "code"),
        ("redirect_uri", redirect_uri),
        ("scope", e.scope),
        ("state", state),
        ("code_challenge", pkce_challenge),
        ("code_challenge_method", "S256"),
    ];
    match provider {
        OAuthProvider::Google => params.extend([("access_type", "offline"), ("prompt", "consent")]),
        OAuthProvider::Microsoft => params.push(("prompt", "select_account")),
    }
    if let Some(hint) = login_hint {
        params.push(("login_hint", hint));
    }
    reqwest::Url::parse_with_params(e.authorize, &params)
        .expect("static authorize URL")
        .to_string()
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    id_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn token_request(
    http: &reqwest::Client,
    provider: OAuthProvider,
    app: &OAuthApp,
    form: &[(&str, &str)],
) -> Result<TokenResponse> {
    let mut form = form.to_vec();
    form.push(("client_id", &app.client_id));
    if let Some(secret) = &app.client_secret {
        form.push(("client_secret", secret));
    }
    let resp = http
        .post(endpoints(provider).token)
        .form(&form)
        .send()
        .await?;
    let status = resp.status();
    let body: TokenResponse = resp.json().await.map_err(|e| {
        err(
            ErrorCode::OAuth,
            format!("unexpected token response ({status}): {e}"),
        )
    })?;
    if let Some(e) = &body.error {
        let code = if e == "invalid_grant" {
            ErrorCode::Auth
        } else {
            ErrorCode::OAuth
        };
        return Err(err(
            code,
            format!(
                "{} sign-in failed: {e}{}",
                provider_name(provider),
                body.error_description
                    .as_deref()
                    .map(|d| format!(" ({d})"))
                    .unwrap_or_default()
            ),
        ));
    }
    if body.access_token.is_none() {
        return Err(err(
            ErrorCode::OAuth,
            format!("token response without access_token ({status})"),
        ));
    }
    Ok(body)
}

fn tokens(body: &TokenResponse, previous_refresh: Option<&str>) -> OAuthTokens {
    OAuthTokens {
        access_token: body.access_token.clone().unwrap_or_default(),
        refresh_token: body
            .refresh_token
            .clone()
            .or_else(|| previous_refresh.map(str::to_string)),
        expires_at: now() + body.expires_in.unwrap_or(3600),
    }
}

pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub async fn exchange(
    http: &reqwest::Client,
    provider: OAuthProvider,
    app: &OAuthApp,
    redirect_uri: &str,
    code: &str,
    pkce_verifier: &str,
) -> Result<OAuthGrant> {
    let body = token_request(
        http,
        provider,
        app,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", pkce_verifier),
        ],
    )
    .await?;
    // The ID token comes straight from the token endpoint over TLS, so its claims can be read
    // without verifying the signature (OpenID Connect Core §3.1.3.7).
    let claims = body
        .id_token
        .as_deref()
        .and_then(|t| t.split('.').nth(1))
        .and_then(|p| B64URL.decode(p.trim_end_matches('=')).ok())
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .unwrap_or_default();
    let email = claims["email"]
        .as_str()
        .or_else(|| claims["preferred_username"].as_str())
        .filter(|e| e.contains('@'))
        .ok_or_else(|| {
            err(
                ErrorCode::OAuth,
                "the provider did not return the account's email address",
            )
        })?
        .to_string();
    Ok(OAuthGrant {
        provider,
        email,
        name: claims["name"].as_str().map(str::to_string),
        tokens: tokens(&body, None),
    })
}

pub async fn refresh(
    http: &reqwest::Client,
    provider: OAuthProvider,
    app: &OAuthApp,
    refresh_token: &str,
) -> Result<OAuthTokens> {
    let body = token_request(
        http,
        provider,
        app,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
    )
    .await?;
    Ok(tokens(&body, Some(refresh_token)))
}

/// A sign-in waiting for the browser to come back to `http://127.0.0.1:<port>/`.
pub struct LoopbackAuthorization {
    /// Open this in the system browser.
    pub url: String,
    listener: TcpListener,
    redirect_uri: String,
    state: String,
    pkce: Pkce,
    provider: OAuthProvider,
    app: OAuthApp,
    http: reqwest::Client,
}

const LOOPBACK_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub async fn start_loopback(
    http: reqwest::Client,
    provider: OAuthProvider,
    apps: &OAuthApps,
    login_hint: Option<&str>,
) -> Result<LoopbackAuthorization> {
    let app = apps.get(provider)?.clone();
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let redirect_uri = format!("http://127.0.0.1:{}/", listener.local_addr()?.port());
    let state = random_token(16);
    let pkce = Pkce::new();
    let url = authorize_url(
        provider,
        &app,
        &redirect_uri,
        &state,
        &pkce.challenge,
        login_hint,
    );
    Ok(LoopbackAuthorization {
        url,
        listener,
        redirect_uri,
        state,
        pkce,
        provider,
        app,
        http,
    })
}

impl LoopbackAuthorization {
    /// Waits (up to 10 minutes) for the redirect, then exchanges the code.
    pub async fn finish(self) -> Result<OAuthGrant> {
        let code = tokio::time::timeout(LOOPBACK_TIMEOUT, self.wait_for_code())
            .await
            .map_err(|_| {
                err(
                    ErrorCode::OAuth,
                    "sign-in timed out: the browser never returned",
                )
            })??;
        exchange(
            &self.http,
            self.provider,
            &self.app,
            &self.redirect_uri,
            &code,
            &self.pkce.verifier,
        )
        .await
    }

    async fn wait_for_code(&self) -> Result<String> {
        loop {
            let (mut sock, _) = self.listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            let mut len = 0;
            while len < buf.len() {
                let n = sock.read(&mut buf[len..]).await?;
                if n == 0 {
                    break;
                }
                len += n;
                if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let req = String::from_utf8_lossy(&buf[..len]);
            let target = req
                .lines()
                .next()
                .and_then(|l| l.strip_prefix("GET "))
                .and_then(|l| l.split(' ').next())
                .unwrap_or("");
            let Ok(url) = reqwest::Url::parse(&format!("http://127.0.0.1{target}")) else {
                respond(&mut sock, "400 Bad Request", "").await;
                continue;
            };
            if url.path() != "/" {
                respond(&mut sock, "404 Not Found", "").await;
                continue;
            }
            let get = |k: &str| {
                url.query_pairs()
                    .find(|(n, _)| n == k)
                    .map(|(_, v)| v.into_owned())
            };
            if get("state").as_deref() != Some(self.state.as_str()) {
                respond(&mut sock, "400 Bad Request", "").await;
                continue;
            }
            if let Some(e) = get("error") {
                respond(&mut sock, "200 OK", PAGE_FAILED).await;
                let detail = get("error_description")
                    .map(|d| format!(" ({d})"))
                    .unwrap_or_default();
                return Err(err(
                    ErrorCode::OAuth,
                    format!("sign-in was not completed: {e}{detail}"),
                ));
            }
            if let Some(code) = get("code") {
                respond(&mut sock, "200 OK", PAGE_DONE).await;
                return Ok(code);
            }
            respond(&mut sock, "400 Bad Request", "").await;
        }
    }
}

const PAGE_DONE: &str = "<!doctype html><meta charset=utf-8><title>nsh-mail</title>\
<p>Signed in. You can close this window and return to nsh-mail.</p>\
<p>登录成功，可以关闭此页面并返回 nsh-mail。</p>";
const PAGE_FAILED: &str = "<!doctype html><meta charset=utf-8><title>nsh-mail</title>\
<p>Sign-in was not completed. Return to nsh-mail to try again.</p>\
<p>登录未完成，请返回 nsh-mail 重试。</p>";

async fn respond(sock: &mut tokio::net::TcpStream, status: &str, body: &str) {
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = sock.write_all(resp.as_bytes()).await;
    let _ = sock.shutdown().await;
}
