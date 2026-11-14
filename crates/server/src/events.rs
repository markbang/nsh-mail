use std::{sync::Arc, time::Duration};

use mail_core::feed::FeedMessage;
use serde::Serialize;

use axum::{
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use mail_core::types::MailEvent;

use crate::{
    AppState,
    api::{ApiResult, AuthUser},
};

/// What a live client is told when the feed changes: the app's All view, an agent's SSE stream
/// and ntfy's JSON stream all read these. `user_id` says whose feed it is — the subscription is
/// shared, so every stream must filter, and it is not sent to anyone.
#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FeedEvent {
    Message {
        #[serde(skip)]
        user_id: i64,
        message: Box<FeedMessage>,
    },
    Read {
        #[serde(skip)]
        user_id: i64,
        id: String,
        source_id: String,
        /// The feed's row number, so a stream that resumes from `Last-Event-ID` does not repeat it.
        seq: i64,
        read_at: Option<i64>,
    },
    Archived {
        #[serde(skip)]
        user_id: i64,
        id: String,
        source_id: String,
        seq: i64,
        archived_at: Option<i64>,
    },
}

impl FeedEvent {
    pub fn user_id(&self) -> i64 {
        match self {
            FeedEvent::Message { user_id, .. }
            | FeedEvent::Read { user_id, .. }
            | FeedEvent::Archived { user_id, .. } => *user_id,
        }
    }

    pub fn source_id(&self) -> &str {
        match self {
            FeedEvent::Message { message, .. } => &message.source_id,
            FeedEvent::Read { source_id, .. } | FeedEvent::Archived { source_id, .. } => source_id,
        }
    }
}

#[derive(Deserialize)]
pub struct EventParams {
    /// Device id from POST /v1/devices; while connected, that device gets no push.
    device: Option<String>,
}

pub async fn events(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(p): Query<EventParams>,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    let mail = state.mailboxes.get(user.id).await?;
    let rx = mail.subscribe();
    Ok(ws.on_upgrade(move |socket| run(state, user.id, p.device, socket, rx)))
}

async fn run(
    state: Arc<AppState>,
    user_id: i64,
    device: Option<String>,
    mut socket: WebSocket,
    mut rx: tokio::sync::broadcast::Receiver<MailEvent>,
) {
    if let Some(d) = &device {
        state.push.presence.enter(user_id, d);
    }
    let mut ping = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(ev) => {
                    let Ok(text) = serde_json::to_string(&ev) else { continue };
                    if socket.send(Message::Text(text.into())).await.is_err() { break }
                }
                // A client that missed events must reload; closing makes it reconnect and refetch.
                Err(RecvError::Lagged(_) | RecvError::Closed) => break,
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            _ = ping.tick() => {
                if socket.send(Message::Ping(Default::default())).await.is_err() { break }
            }
        }
    }
    if let Some(d) = &device {
        state.push.presence.leave(user_id, d);
    }
}
