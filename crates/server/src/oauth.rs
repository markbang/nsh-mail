//! Server-side OAuth web flow (authorization code + PKCE, confidential client) for cloud mode.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use mail_core::{
    OAuthApp,
    oauth::{Pkce, authorize_url, exchange},
    types::{ErrorCode, OAuthProvider},
};
use reqwest::Url;
use serde::Deserialize;

use crate::{
    AppState,
    api::{ApiError, ApiResult, AuthUser},
    crypto,
    db::OAuthState,
};

fn provider(name: &str) -> ApiResult<OAuthProvider> {
    match name {
        "google" => Ok(OAuthProvider::Google),
        "microsoft" => Ok(OAuthProvider::Microsoft),
        _ => Err(ApiError::new(
            StatusCode::NOT_FOUND,
            ErrorCode::NotFound,
            "unknown OAuth provider; use google or microsoft",
        )),
    }
}

fn app(state: &AppState, p: OAuthProvider) -> Option<&OAuthApp> {
    match p {
        OAuthProvider::Google => state.config.oauth.google.as_ref(),
        OAuthProvider::Microsoft => state.config.oauth.microsoft.as_ref(),
    }
}

fn env_vars(p: OAuthProvider) -> &'static str {
    match p {
        OAuthProvider::Google => "NSHMAIL_GOOGLE_CLIENT_ID / NSHMAIL_GOOGLE_CLIENT_SECRET",
        OAuthProvider::Microsoft => "NSHMAIL_MS_CLIENT_ID / NSHMAIL_MS_CLIENT_SECRET",
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
    let Some(app) = app(&state, p) else {
        return Ok(finish(
            &state,
            &return_to,
            Err(format!(
                "{name} sign-in is not configured on this server ({})",
                env_vars(p)
            )),
        ));
    };
    let st = crypto::new_token();
    let pkce = Pkce::new();
    state.db.put_oauth_state(
        &crypto::sha256(st.as_bytes()),
        &OAuthState {
            user_id: user.id,
            provider: name.clone(),
            verifier: pkce.verifier,
            return_to,
        },
    )?;
    let redirect = redirect_uri(&state, &name);
    let url = authorize_url(p, app, &redirect, &st, &pkce.challenge, None);
    Ok(Redirect::to(&url).into_response())
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
    p: OAuthProvider,
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
    let app = app(state, p).ok_or("OAuth is no longer configured on this server")?;
    let redirect = redirect_uri(state, name);
    let grant = exchange(&state.http, p, app, &redirect, &code, &pending.verifier)
        .await
        .map_err(|e| e.message)?;
    let mail = state
        .mailboxes
        .get(pending.user_id)
        .await
        .map_err(|e| e.message)?;
    let account = mail.add_oauth_account(grant).await.map_err(|e| e.message)?;
    Ok(account.id)
}
