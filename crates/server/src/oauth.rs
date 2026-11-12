//! Server-side OAuth web flow (authorization code + PKCE, confidential client) for cloud mode.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::Url;
use serde::Deserialize;

use crate::{
    AppState,
    api::{ApiError, ApiResult, AuthUser},
    config::OAuthClient,
    crypto,
    db::{OAuthState, now},
    mail::OAuthTokens,
    types::{ErrorCode, OAuthProvider},
};

struct ProviderInfo {
    provider: OAuthProvider,
    auth_url: &'static str,
    token_url: &'static str,
    scope: &'static str,
    extra: &'static [(&'static str, &'static str)],
    env: &'static str,
}

const GOOGLE: ProviderInfo = ProviderInfo {
    provider: OAuthProvider::Google,
    auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
    token_url: "https://oauth2.googleapis.com/token",
    scope: "openid email https://mail.google.com/",
    // Without prompt=consent Google omits the refresh token on repeat grants.
    extra: &[("access_type", "offline"), ("prompt", "consent")],
    env: "NSHMAIL_GOOGLE_CLIENT_ID / NSHMAIL_GOOGLE_CLIENT_SECRET",
};

const MICROSOFT: ProviderInfo = ProviderInfo {
    provider: OAuthProvider::Microsoft,
    auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
    token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
    scope: "openid email offline_access https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send",
    extra: &[("prompt", "select_account")],
    env: "NSHMAIL_MS_CLIENT_ID / NSHMAIL_MS_CLIENT_SECRET",
};

fn provider(name: &str) -> ApiResult<&'static ProviderInfo> {
    match name {
        "google" => Ok(&GOOGLE),
        "microsoft" => Ok(&MICROSOFT),
        _ => Err(ApiError::new(
            StatusCode::NOT_FOUND,
            ErrorCode::NotFound,
            "unknown OAuth provider; use google or microsoft",
        )),
    }
}

fn client<'a>(state: &'a AppState, p: &ProviderInfo) -> Option<&'a OAuthClient> {
    match p.provider {
        OAuthProvider::Google => state.config.google.as_ref(),
        OAuthProvider::Microsoft => state.config.microsoft.as_ref(),
    }
}

fn redirect_uri(state: &AppState, name: &str) -> String {
    format!("{}/v1/oauth/{name}/callback", state.config.public_url)
}

/// `native` returns to the app via `nshmail://oauth-done`, anything else to the web client.
fn finish(state: &AppState, return_to: &str, result: Result<String, String>) -> Response {
    let base = if return_to == "native" {
        "nshmail://oauth-done".to_string()
    } else {
        format!("{}/?oauth-done", state.config.public_url)
    };
    let pair = match &result {
        Ok(account) => ("account", account.as_str()),
        Err(e) => ("error", e.as_str()),
    };
    match Url::parse_with_params(&base, [pair]) {
        Ok(url) => Redirect::to(url.as_str()).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "bad redirect target").into_response(),
    }
}

#[derive(Deserialize)]
pub struct StartParams {
    #[serde(rename = "return")]
    return_to: Option<String>,
}

pub async fn start(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(name): Path<String>,
    Query(q): Query<StartParams>,
) -> ApiResult<Response> {
    let p = provider(&name)?;
    let return_to = q.return_to.unwrap_or_else(|| "web".into());
    let Some(client) = client(&state, p) else {
        return Ok(finish(
            &state,
            &return_to,
            Err(format!(
                "{name} sign-in is not configured on this server ({})",
                p.env
            )),
        ));
    };
    let st = crypto::new_token();
    let verifier = crypto::new_token();
    let challenge = URL_SAFE_NO_PAD.encode(crypto::sha256(verifier.as_bytes()));
    state.db.put_oauth_state(
        &crypto::sha256(st.as_bytes()),
        &OAuthState {
            user_id: user.id,
            provider: name.clone(),
            verifier,
            return_to,
        },
    )?;
    let redirect = redirect_uri(&state, &name);
    let mut params = vec![
        ("client_id", client.id.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("response_type", "code"),
        ("scope", p.scope),
        ("state", st.as_str()),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
    ];
    params.extend_from_slice(p.extra);
    let url = Url::parse_with_params(p.auth_url, &params).map_err(anyhow::Error::from)?;
    Ok(Redirect::to(url.as_str()).into_response())
}

#[derive(Deserialize)]
pub struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

pub async fn callback(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<CallbackParams>,
) -> ApiResult<Response> {
    let p = provider(&name)?;
    let pending = match q.state.as_deref() {
        Some(s) => state.db.take_oauth_state(&crypto::sha256(s.as_bytes()))?,
        None => None,
    };
    let Some(pending) = pending.filter(|s| s.provider == name) else {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::OAuth,
            "this sign-in link expired or was already used; start again from the app",
        ));
    };
    let result = complete(&state, p, &name, &pending, q).await;
    if let Err(e) = &result {
        tracing::warn!(provider = name, "OAuth sign-in failed: {e}");
    }
    Ok(finish(&state, &pending.return_to, result))
}

async fn complete(
    state: &AppState,
    p: &ProviderInfo,
    name: &str,
    pending: &OAuthState,
    q: CallbackParams,
) -> Result<String, String> {
    if let Some(err) = q.error {
        return Err(q.error_description.unwrap_or(err));
    }
    let code = q
        .code
        .ok_or("the provider returned no authorization code")?;
    let client = client(state, p).ok_or("OAuth is no longer configured on this server")?;
    let redirect = redirect_uri(state, name);
    let res = state
        .http
        .post(p.token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("client_id", client.id.as_str()),
            ("client_secret", client.secret.as_str()),
            ("code_verifier", pending.verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("cannot reach {name}: {e}"))?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("{name} rejected the sign-in ({status}): {body}"));
    }
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<i64>,
        id_token: Option<String>,
    }
    let t: TokenResponse = res
        .json()
        .await
        .map_err(|e| format!("unexpected token response: {e}"))?;
    let email = t
        .id_token
        .as_deref()
        .and_then(id_token_email)
        .ok_or("the provider did not return an email address")?;
    let tokens = OAuthTokens {
        access_token: t.access_token,
        refresh_token: t.refresh_token,
        expires_at: t.expires_in.map(|s| now() + s),
    };
    let mail = state
        .mailboxes
        .get(pending.user_id)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let account = mail
        .add_oauth(p.provider, email, tokens)
        .await
        .map_err(|e| e.message)?;
    Ok(account.id)
}

/// The id_token came straight from the provider's token endpoint over TLS, so its claims are
/// trusted without signature verification (OIDC Core §3.1.3.7).
fn id_token_email(jwt: &str) -> Option<String> {
    let payload = URL_SAFE_NO_PAD.decode(jwt.split('.').nth(1)?).ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    ["email", "preferred_username"].iter().find_map(|k| {
        claims
            .get(*k)?
            .as_str()
            .filter(|s| s.contains('@'))
            .map(str::to_lowercase)
    })
}
