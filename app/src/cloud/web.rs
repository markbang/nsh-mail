//! The browser's part of cloud mode: the page's origin (the server's default
//! address), the return from a provider's sign-in window, and Web Push
//! through the service worker `app/web/sw.js` (docs/design.md §5).

use std::{cell::RefCell, future::Future};

use futures_channel::oneshot;
use js_sys::{Object, Promise, Reflect, Uint8Array};
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    BroadcastChannel, MessageEvent, Navigator, Notification, NotificationPermission,
    PermissionStatus, PushManager, PushSubscription, PushSubscriptionOptionsInit,
    ServiceWorkerRegistration, Window,
};

use mail_core::types::{Error, ErrorCode, Result};

use super::Api;

/// Where the window the sign-in returned to tells the one that opened it.
const OAUTH_CHANNEL: &str = "nshmail-oauth";
/// Served next to the page: the server serves `sw.js` from the web client's directory.
const SERVICE_WORKER: &str = "sw.js";

fn reason(e: JsValue) -> String {
    e.dyn_ref::<js_sys::Error>()
        .map(|e| String::from(e.message()))
        .or_else(|| e.as_string())
        .unwrap_or_else(|| format!("{e:?}"))
}

fn fail(e: JsValue) -> Error {
    Error::new(ErrorCode::Unsupported, reason(e))
}

async fn js(promise: Promise) -> Result<JsValue> {
    JsFuture::from(promise).await.map_err(fail)
}

fn window() -> Result<Window> {
    web_sys::window().ok_or_else(|| Error::new(ErrorCode::Unsupported, "no browser window"))
}

pub fn page_origin() -> Option<String> {
    web_sys::window()?.location().origin().ok()
}

/// In the window a sign-in returned to (`/?oauth-done&…`): hands the link to
/// the window that started it and closes. A window the browser keeps open
/// goes on as the app.
pub fn finish_oauth() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(href) = window.location().href() else {
        return;
    };
    let query = href.split_once('?').map_or("", |(_, q)| q);
    if !query.split('&').any(|part| part == "oauth-done") {
        return;
    }
    if let Ok(channel) = BroadcastChannel::new(OAUTH_CHANNEL) {
        let _ = channel.post_message(&JsValue::from_str(&href));
        channel.close();
    }
    let _ = window.close();
    let _ = nsh_core::prelude::set_app_url(".");
}

/// Stops listening when the wait ends or is dropped, so the browser never
/// calls a dropped closure.
/// Holds the closure the channel calls.
struct OnMessage(
    BroadcastChannel,
    #[allow(dead_code)] Closure<dyn FnMut(MessageEvent)>,
);

impl Drop for OnMessage {
    fn drop(&mut self) {
        self.0.set_onmessage(None);
        self.0.close();
    }
}

/// The link the sign-in window returns with. Listens from this call on, so
/// call it before opening the window.
pub fn oauth_return() -> impl Future<Output = Result<String>> {
    let (tx, rx) = oneshot::channel::<String>();
    let tx = RefCell::new(Some(tx));
    let listening = BroadcastChannel::new(OAUTH_CHANNEL).map(|channel| {
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            if let (Some(link), Some(tx)) = (e.data().as_string(), tx.borrow_mut().take()) {
                let _ = tx.send(link);
            }
        });
        channel.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        OnMessage(channel, on_message)
    });
    async move {
        let _listening = listening.map_err(fail)?;
        rx.await
            .map_err(|_| Error::new(ErrorCode::OAuth, "the sign-in window closed"))
    }
}

struct OnChange(PermissionStatus, #[allow(dead_code)] Closure<dyn FnMut()>);

impl Drop for OnChange {
    fn drop(&mut self) {
        self.0.set_onchange(None);
    }
}

/// Whether the user lets the app notify, waiting while it has not answered
/// (onboarding asks right after signing in).
async fn notifications_allowed(navigator: &Navigator) -> Result<bool> {
    loop {
        match Notification::permission() {
            NotificationPermission::Granted => return Ok(true),
            NotificationPermission::Denied => return Ok(false),
            _ => {}
        }
        let query = Object::new();
        Reflect::set(&query, &"name".into(), &"notifications".into()).map_err(fail)?;
        let status: PermissionStatus = js(navigator
            .permissions()
            .map_err(fail)?
            .query(&query)
            .map_err(fail)?)
        .await?
        .unchecked_into();
        let (tx, rx) = oneshot::channel::<()>();
        let tx = RefCell::new(Some(tx));
        let on_change = Closure::<dyn FnMut()>::new(move || {
            if let Some(tx) = tx.borrow_mut().take() {
                let _ = tx.send(());
            }
        });
        status.set_onchange(Some(on_change.as_ref().unchecked_ref()));
        let _guard = OnChange(status, on_change);
        let _ = rx.await;
    }
}

fn supported(window: &Window) -> bool {
    let has = |target: &JsValue, name: &str| Reflect::has(target, &name.into()).unwrap_or(false);
    has(&window.navigator(), "serviceWorker") && has(window, "PushManager")
}

/// The VAPID key, base64url as the server gives it, as bytes.
fn key_bytes(window: &Window, key: &str) -> Result<Uint8Array> {
    let mut b64 = key.replace('-', "+").replace('_', "/");
    while !b64.len().is_multiple_of(4) {
        b64.push('=');
    }
    let raw = window.atob(&b64).map_err(fail)?;
    let bytes: Vec<u8> = raw.chars().map(|c| c as u8).collect();
    Ok(Uint8Array::from(bytes.as_slice()))
}

async fn push_manager(window: &Window, register: bool) -> Result<Option<PushManager>> {
    let workers = window.navigator().service_worker();
    let registration = if register {
        js(workers.register(SERVICE_WORKER)).await?;
        js(workers.ready().map_err(fail)?).await?
    } else {
        js(workers.get_registration()).await?
    };
    if registration.is_undefined() {
        return Ok(None);
    }
    let registration: ServiceWorkerRegistration = registration.unchecked_into();
    registration.push_manager().map(Some).map_err(fail)
}

/// Subscribes this browser to Web Push and registers it with the server:
/// its device id, or `None` where the browser has no push or the user
/// declined notifications.
pub async fn subscribe_push(api: &Api) -> Result<Option<String>> {
    let window = window()?;
    if !supported(&window) || !notifications_allowed(&window.navigator()).await? {
        return Ok(None);
    }
    let Some(manager) = push_manager(&window, true).await? else {
        return Ok(None);
    };
    let key: Value = api.call("GET", "/push/vapid-key", None).await?;
    let key = key["key"]
        .as_str()
        .ok_or_else(|| Error::new(ErrorCode::Protocol, "the server has no Web Push key"))?;
    let options = PushSubscriptionOptionsInit::new();
    options.set_user_visible_only(true);
    options.set_application_server_key(&key_bytes(&window, key)?.into());
    let subscription = js(manager.subscribe_with_options(&options).map_err(fail)?).await?;
    // `toJSON`: `{endpoint, keys: {p256dh, auth}}`, as the server reads it.
    let subscription = js_sys::JSON::stringify(&subscription)
        .map_err(fail)?
        .as_string()
        .unwrap_or_default();
    let subscription: Value = super::parse(&subscription)?;
    let device: Value = api
        .call(
            "POST",
            "/devices",
            Some(json!({ "kind": "webpush", "subscription": subscription })),
        )
        .await?;
    Ok(device["id"].as_str().map(str::to_owned))
}

/// Ends this browser's push subscription (signing out).
pub async fn unsubscribe_push() {
    let Ok(window) = window() else { return };
    if !supported(&window) {
        return;
    }
    let ended = async {
        let Some(manager) = push_manager(&window, false).await? else {
            return Ok(());
        };
        let subscription = js(manager.get_subscription().map_err(fail)?).await?;
        if let Some(subscription) = subscription.dyn_ref::<PushSubscription>() {
            js(subscription.unsubscribe().map_err(fail)?).await?;
        }
        Ok::<_, Error>(())
    };
    if let Err(e) = ended.await {
        log::warn!("cannot end the push subscription: {}", e.message);
    }
}
