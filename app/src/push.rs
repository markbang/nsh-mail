//! The device's push token: FCM's on Android, which the Gradle host
//! (`android/`) sends as the host event `push.token` and `push.ts` hands to
//! `Push.setToken`. Cloud mode registers it with the server
//! (`POST /v1/devices {kind: "fcm", token}`) through [`on_token`].

use std::{cell::RefCell, rc::Rc};

use nsh_core::prelude::*;

/// Where a token goes: the cloud backend's registration.
pub type TokenSink = Rc<dyn Fn(&str)>;

#[derive(Default)]
struct State {
    token: Option<String>,
    sink: Option<TokenSink>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Sends the current token (if any) and each new one to `sink`; `None`
/// stops. The cloud backend sets it once signed in and clears it on sign-out.
pub fn on_token(sink: Option<TokenSink>) {
    let current = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.sink = sink.clone();
        s.token.clone()
    });
    if let (Some(sink), Some(token)) = (sink, current) {
        sink(&token);
    }
}

/// The device's push registration, as TSX hands it over.
#[module]
pub struct Push;

#[module]
impl Push {
    /// Keeps `token` and hands it to cloud mode, at once when signed in.
    // Async: registering runs a task, which needs the engine's runtime.
    pub async fn set_token(token: String) {
        let sink = STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.token.as_deref() == Some(token.as_str()) {
                return None;
            }
            s.token = Some(token.clone());
            s.sink.clone()
        });
        if let Some(sink) = sink {
            sink(&token);
        }
    }
}
