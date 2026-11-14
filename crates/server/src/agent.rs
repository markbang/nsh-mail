//! The agent API: an API key reads the notification feed over REST, SSE, WebSocket or MCP,
//! narrowed to the key's sources and scopes. The app uses the same routes with its session, so
//! what an agent sees and what a person sees cannot drift.

use std::{
    collections::{HashMap, VecDeque},
    convert::Infallible,
    sync::Arc,
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{
        FromRequestParts, Path, Query, Request, State, WebSocketUpgrade,
        ws::{Message as WsMessage, WebSocket},
    },
    http::{StatusCode, request::Parts},
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::stream::{self, Stream};
use mail_core::{
    feed::{Archived, Cursor, FeedMessage, LIST_LIMIT, ListFilter, now_ms},
    types::{ErrorCode, FolderKind},
};
use notify_mcp::{BackendFactory, Error as McpError, ListQuery, Notify, Page};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::broadcast::{Receiver, error::RecvError};
use utoipa::ToSchema;

use crate::{
    AppState,
    api::{ApiError, ApiResult, AuthUser},
    apikeys::{self, Agent, Key},
    events::FeedEvent,
};

/// How many notifications a new stream replays before it follows live ones; a client further
/// behind pages the REST API and then connects with `?since=`.
const REPLAY_LIMIT: u32 = 500;
/// Ids one `POST /v1/feed/read` or `/archive` may carry.
const BATCH_LIMIT: usize = 200;

/// Who is calling: the app's session (everything its user has) or an API key (its scope).
pub enum Caller {
    Session { user_id: i64 },
    Key(Agent),
}

impl Caller {
    pub fn user_id(&self) -> i64 {
        match self {
            Caller::Session { user_id } => *user_id,
            Caller::Key(agent) => agent.user_id,
        }
    }

    /// The sources it may see; `None` is every source of its user.
    pub fn sources(&self) -> Option<Vec<String>> {
        match self {
            Caller::Session { .. } => None,
            Caller::Key(agent) => agent.sources.clone(),
        }
    }

    pub fn is_key(&self) -> bool {
        matches!(self, Caller::Key(_))
    }

    pub fn may_see(&self, source_id: &str) -> bool {
        match self.sources() {
            Some(sources) => sources.iter().any(|s| s == source_id),
            None => true,
        }
    }

    pub fn require_write(&self) -> ApiResult<()> {
        match self {
            Caller::Session { .. } => Ok(()),
            Caller::Key(agent) => agent.require_write(),
        }
    }
}

impl FromRequestParts<Arc<AppState>> for Caller {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> ApiResult<Self> {
        // A key is recognisable by its prefix, so a session token never reaches the key check.
        if apikeys::bearer(parts).is_some() || parts.headers.contains_key("x-api-key") {
            return Agent::from_request_parts(parts, state)
                .await
                .map(Caller::Key);
        }
        AuthUser::from_request_parts(parts, state)
            .await
            .map(|user| Caller::Session { user_id: user.id })
    }
}

// ---------------------------------------------------------------- feed (REST)

#[derive(Deserialize)]
pub struct FeedParams {
    /// One source's id (`src_…`); every source the caller may see when left out.
    source: Option<String>,
    /// Words to find in sender, title and body.
    q: Option<String>,
    /// The same, spelled as the app's feed query does.
    text: Option<String>,
    /// `1` or `true`: only unread notifications still in the feed.
    unread: Option<String>,
    /// `0` (the feed view, default), `1` (only archived) or `any`.
    archived: Option<String>,
    /// Only notifications received after this: RFC 3339, ms since the epoch, or a message id.
    since: Option<String>,
    /// The previous page's last message, by id.
    before: Option<String>,
    /// The previous page's `nextCursor`.
    cursor: Option<String>,
    limit: Option<u32>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FeedList {
    messages: Vec<FeedMessage>,
    /// The cursor for the next page; absent on the last one.
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
    /// More notifications wait beyond this page.
    more: bool,
    /// Unread notifications the caller can see.
    unread: i64,
    /// Unread per source id.
    counts: HashMap<String, i64>,
}

/// One page of the notification feed, newest first.
#[utoipa::path(
    get,
    path = "/v1/feed",
    tag = "feed",
    security(("bearer" = [])),
    params(
        ("source" = Option<String>, Query, description = "Only this source (src_…)"),
        ("q" = Option<String>, Query, description = "Words to find in sender, title and body"),
        ("unread" = Option<String>, Query, description = "1: only unread notifications in the feed"),
        ("archived" = Option<String>, Query, description = "0 (default), 1 (only archived) or any"),
        ("since" = Option<String>, Query, description = "Only newer than this RFC 3339 time, ms since the epoch or message id"),
        ("before" = Option<String>, Query, description = "The previous page's last message id"),
        ("cursor" = Option<String>, Query, description = "The previous page's nextCursor"),
        ("limit" = Option<u32>, Query, description = "1–200, default 50"),
    ),
    responses(
        (status = 200, description = "A page of the feed", body = FeedList),
        (status = 401, description = "No valid session or API key"),
        (status = 403, description = "The API key lacks the write scope"),
    )
)]
pub async fn list_feed(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Query(p): Query<FeedParams>,
) -> ApiResult<Json<FeedList>> {
    let feed = state.mail_db.feed(caller.user_id());
    let before = match (p.cursor.as_deref(), p.before.as_deref()) {
        (Some(cursor), _) => Some(decode_cursor(cursor).ok_or_else(|| {
            ApiError::bad_request("cursor is not one this server issued; start the page again")
        })?),
        (None, Some(id)) => Some(feed.cursor_of(id)?),
        (None, None) => None,
    };
    let filter = ListFilter {
        sources: caller.sources(),
        source: p.source.clone(),
        unread: truthy(p.unread.as_deref()),
        archived: archived(p.archived.as_deref())?,
        text: p.q.or(p.text),
        since: match p.since.as_deref() {
            Some(value) => Some(since_ms(&feed, value)?),
            None => None,
        },
        before,
        limit: p.limit.unwrap_or(50).clamp(1, LIST_LIMIT),
    };
    if let Some(source) = &filter.source
        && !caller.may_see(source)
    {
        return Err(forbidden_source(source));
    }
    let page = feed.list(&filter)?;
    let (mut unread, per_source) = feed.unread()?;
    let mut counts: HashMap<String, i64> = per_source.into_iter().collect();
    if let Some(scope) = caller.sources() {
        counts.retain(|id, _| scope.contains(id));
        unread = counts.values().sum();
    }
    let next_cursor = page
        .more
        .then(|| page.messages.last().map(encode_cursor))
        .flatten();
    Ok(Json(FeedList {
        messages: page.messages,
        next_cursor,
        more: page.more,
        unread,
        counts,
    }))
}

/// One notification in full.
#[utoipa::path(
    get,
    path = "/v1/feed/{id}",
    tag = "feed",
    security(("bearer" = [])),
    params(("id" = String, Path, description = "Message id: msg_…, or a mail message id")),
    responses(
        (status = 200, description = "The notification", body = FeedMessage),
        (status = 404, description = "No such notification, or outside the API key's sources"),
    )
)]
pub async fn get_feed_message(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(id): Path<String>,
) -> ApiResult<Json<FeedMessage>> {
    Ok(Json(visible_message(&state, &caller, &id)?))
}

#[derive(Deserialize, ToSchema, Default)]
pub struct ReadBody {
    /// Default true; `false` makes it unread again.
    read: Option<bool>,
}

/// Marks one notification read (or unread again). A mail message's own flag follows, so the
/// mailbox agrees with the feed.
#[utoipa::path(
    post,
    path = "/v1/feed/{id}/read",
    tag = "feed",
    security(("bearer" = [])),
    params(("id" = String, Path, description = "Message id")),
    request_body = inline(ReadBody),
    responses(
        (status = 200, description = "The updated notification", body = FeedMessage),
        (status = 403, description = "The API key lacks the write scope"),
        (status = 404, description = "No such notification, or outside the key's sources"),
    )
)]
pub async fn read_feed_message(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(id): Path<String>,
    body: Option<Json<ReadBody>>,
) -> ApiResult<Json<FeedMessage>> {
    let read = body.and_then(|b| b.0.read).unwrap_or(true);
    Ok(Json(set_read(&state, &caller, &id, read)?))
}

/// Takes one notification out of the feed. A mail notification is also moved to Archive, through
/// the same path the app uses.
#[utoipa::path(
    post,
    path = "/v1/feed/{id}/archive",
    tag = "feed",
    security(("bearer" = [])),
    params(("id" = String, Path, description = "Message id")),
    responses(
        (status = 204, description = "Archived"),
        (status = 403, description = "The API key lacks the write scope"),
        (status = 404, description = "No such notification, or outside the key's sources"),
        (status = 502, description = "The mail server refused the move to Archive"),
    )
)]
pub async fn archive_feed_message(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    archive(&state, &caller, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadMany {
    /// At most 200.
    ids: Vec<String>,
    /// Default true.
    #[serde(default)]
    read: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct ArchiveMany {
    /// At most 200.
    ids: Vec<String>,
}

/// Marks several notifications read or unread at once, as the app's list does.
#[utoipa::path(
    post,
    path = "/v1/feed/read",
    tag = "feed",
    security(("bearer" = [])),
    request_body = inline(ReadMany),
    responses(
        (status = 204, description = "Applied"),
        (status = 400, description = "More than 200 ids"),
        (status = 403, description = "The API key lacks the write scope"),
    )
)]
pub async fn read_feed_messages(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Json(body): Json<ReadMany>,
) -> ApiResult<StatusCode> {
    check_batch(&body.ids)?;
    let read = body.read.unwrap_or(true);
    for id in &body.ids {
        set_read(&state, &caller, id, read)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Archives several notifications at once, as the app's list does.
#[utoipa::path(
    post,
    path = "/v1/feed/archive",
    tag = "feed",
    security(("bearer" = [])),
    request_body = inline(ArchiveMany),
    responses(
        (status = 204, description = "Archived"),
        (status = 400, description = "More than 200 ids"),
        (status = 403, description = "The API key lacks the write scope"),
    )
)]
pub async fn archive_feed_messages(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Json(body): Json<ArchiveMany>,
) -> ApiResult<StatusCode> {
    check_batch(&body.ids)?;
    for id in &body.ids {
        archive(&state, &caller, id).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

fn check_batch(ids: &[String]) -> ApiResult<()> {
    if ids.len() > BATCH_LIMIT {
        return Err(ApiError::bad_request(format!(
            "at most {BATCH_LIMIT} ids at a time"
        )));
    }
    Ok(())
}

fn visible_message(state: &AppState, caller: &Caller, id: &str) -> ApiResult<FeedMessage> {
    let message = state.mail_db.feed(caller.user_id()).message(id)?;
    if caller.may_see(&message.source_id) {
        return Ok(message);
    }
    // The same answer as a missing one: a key must not learn what it cannot read.
    Err(ApiError::not_found())
}

fn set_read(state: &AppState, caller: &Caller, id: &str, read: bool) -> ApiResult<FeedMessage> {
    caller.require_write()?;
    visible_message(state, caller, id)?;
    let message = state.mail_db.feed(caller.user_id()).set_read(id, read)?;
    let _ = state.feed_events.send(FeedEvent::Read {
        user_id: caller.user_id(),
        id: message.id.clone(),
        source_id: message.source_id.clone(),
        seq: message.seq,
        read_at: message.read_at,
    });
    Ok(message)
}

/// Feed-level archive; a mail notification also moves to Archive through the mail engine, so the
/// mailbox and the feed agree. The move deletes the local row and re-projects it there, which is
/// why a mail notification has no updated row to answer with.
async fn archive(state: &AppState, caller: &Caller, id: &str) -> ApiResult<()> {
    caller.require_write()?;
    let message = visible_message(state, caller, id)?;
    let feed = state.mail_db.feed(caller.user_id());
    let source = feed.source(&message.source_id)?;
    if source.is_mail() {
        let engine = state.mailboxes.get(caller.user_id()).await?;
        engine.move_to(id, FolderKind::Archive).await?;
        let _ = state.feed_events.send(FeedEvent::Archived {
            user_id: caller.user_id(),
            id: id.to_string(),
            source_id: message.source_id.clone(),
            seq: message.seq,
            archived_at: Some(now_ms()),
        });
        return Ok(());
    }
    let message = feed.set_archived(id, true)?;
    let _ = state.feed_events.send(FeedEvent::Archived {
        user_id: caller.user_id(),
        id: message.id,
        source_id: message.source_id,
        seq: message.seq,
        archived_at: message.archived_at,
    });
    Ok(())
}

fn forbidden_source(source: &str) -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        ErrorCode::Auth,
        format!("this API key cannot read {source}"),
    )
}

fn truthy(value: Option<&str>) -> bool {
    value.is_some_and(|v| matches!(v.trim(), "" | "1" | "true" | "yes" | "on"))
}

fn archived(value: Option<&str>) -> ApiResult<Archived> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None | Some("0" | "false" | "hide") => Ok(Archived::Hide),
        Some("1" | "true" | "only") => Ok(Archived::Only),
        Some("any" | "all") => Ok(Archived::Any),
        Some(other) => Err(ApiError::bad_request(format!(
            "archived must be 0, 1 or any, not {other:?}"
        ))),
    }
}

/// `since` takes a message id, ms since the epoch, or an RFC 3339 time (a date means its start).
fn since_ms(feed: &mail_core::feed::Feed, value: &str) -> ApiResult<i64> {
    let value = value.trim();
    if let Ok(ms) = value.parse::<i64>() {
        return Ok(ms);
    }
    if let Some(ms) = rfc3339_ms(value) {
        return Ok(ms);
    }
    if let Ok(cursor) = feed.cursor_of(value) {
        return Ok(cursor.received_at);
    }
    Err(ApiError::bad_request(format!(
        "since must be a message id, milliseconds since the epoch or an RFC 3339 time, not \
         {value:?}"
    )))
}

/// Cursors are opaque: base64 of `received_at:id`, which is the feed's keyset.
fn encode_cursor(message: &FeedMessage) -> String {
    URL_SAFE_NO_PAD.encode(format!("{}:{}", message.received_at, message.id))
}

fn decode_cursor(value: &str) -> Option<Cursor> {
    let raw = String::from_utf8(URL_SAFE_NO_PAD.decode(value.trim()).ok()?).ok()?;
    let (at, id) = raw.split_once(':')?;
    Some(Cursor {
        received_at: at.parse().ok()?,
        id: id.to_string(),
    })
}

/// RFC 3339 in UTC or with an offset, or a bare `YYYY-MM-DD`. Hand-written: the server needs no
/// time zone database to read a timestamp an agent sends.
fn rfc3339_ms(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let number = |from: usize, len: usize| text.get(from..from + len)?.parse::<i64>().ok();
    let (year, month, day) = (number(0, 4)?, number(5, 2)?, number(8, 2)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (mut hour, mut minute, mut second, mut millis, mut offset) = (0, 0, 0, 0, 0);
    if bytes.len() > 10 {
        if bytes[10] != b'T' && bytes[10] != b' ' || bytes.len() < 19 {
            return None;
        }
        hour = number(11, 2)?;
        minute = number(14, 2)?;
        second = number(17, 2)?;
        let mut rest = 19;
        if bytes.get(rest) == Some(&b'.') {
            let start = rest + 1;
            rest = start;
            while bytes.get(rest).is_some_and(u8::is_ascii_digit) {
                rest += 1;
            }
            let digits = text.get(start..rest)?;
            let mut frac = String::from(digits);
            frac.truncate(3);
            while frac.len() < 3 {
                frac.push('0');
            }
            millis = frac.parse().ok()?;
        }
        match bytes.get(rest) {
            Some(b'Z') if bytes.len() == rest + 1 => {}
            Some(sign @ (b'+' | b'-')) if bytes.len() == rest + 6 => {
                let hours = number(rest + 1, 2)?;
                let minutes = number(rest + 4, 2)?;
                let seconds = hours * 3600 + minutes * 60;
                offset = if *sign == b'-' { -seconds } else { seconds };
            }
            _ => return None,
        }
    }
    let days = days_from_civil(year, month, day);
    Some((days * 86_400 + hour * 3_600 + minute * 60 + second - offset) * 1_000 + millis)
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm, public domain).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let year_of_era = y - era * 400;
    let shifted = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

// ------------------------------------------------------------------ streaming

#[derive(Deserialize)]
pub struct StreamParams {
    /// `ws` for a WebSocket instead of SSE.
    format: Option<String>,
    /// Only this source; the caller's scope always applies.
    source: Option<String>,
    /// Where to start: the feed's seq. `Last-Event-ID` overrides it on a reconnect.
    since: Option<i64>,
}

/// Which events a stream sends, and where it stopped.
struct StreamScope {
    user_id: i64,
    /// `None`: every source of the user.
    sources: Option<Vec<String>>,
    last_seq: i64,
}

impl StreamScope {
    /// Whether this stream carries the change. New messages are deduplicated against the last
    /// seq replayed; a `read` or `archived` event is about a row the client already has, so it is
    /// live-only and repeats nothing.
    fn accepts(&self, change: &FeedEvent) -> bool {
        if change.user_id() != self.user_id {
            return false;
        }
        if let Some(sources) = &self.sources
            && !sources.iter().any(|s| s == change.source_id())
        {
            return false;
        }
        match change {
            FeedEvent::Message { message, .. } => message.seq > self.last_seq,
            _ => true,
        }
    }

    /// The SSE event for one feed change, or `None` when it is not this stream's.
    fn event(&mut self, change: &FeedEvent) -> Option<Event> {
        if !self.accepts(change) {
            return None;
        }
        let (name, data, id) = match change {
            FeedEvent::Message { message, .. } => {
                self.last_seq = message.seq;
                ("message", serde_json::to_string(message), Some(message.seq))
            }
            FeedEvent::Read { id, read_at, .. } => (
                "read",
                serde_json::to_string(&json!({ "id": id, "readAt": read_at })),
                None,
            ),
            FeedEvent::Archived {
                id, archived_at, ..
            } => (
                "archived",
                serde_json::to_string(&json!({ "id": id, "archivedAt": archived_at })),
                None,
            ),
        };
        let mut event = Event::default().event(name).data(data.unwrap_or_default());
        if let Some(seq) = id {
            event = event.id(seq.to_string());
        }
        Some(event)
    }
}

/// Live notifications: SSE by default, a WebSocket with `?format=ws`. Events are `message`
/// (the same JSON as `/v1/feed/{id}`), `read` and `archived`; each carries the feed's seq as its
/// id, so `Last-Event-ID` or `?since=` resumes without a gap.
#[utoipa::path(
    get,
    path = "/v1/stream",
    tag = "feed",
    security(("bearer" = [])),
    params(
        ("format" = Option<String>, Query, description = "`ws` for a WebSocket; SSE otherwise"),
        ("source" = Option<String>, Query, description = "Only this source"),
        ("since" = Option<i64>, Query, description = "The feed seq to resume after"),
    ),
    responses(
        (status = 200, description = "text/event-stream of message, read and archived events"),
        (status = 401, description = "No valid session or API key"),
        (status = 403, description = "The API key cannot read the requested source"),
    )
)]
pub async fn stream(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Query(p): Query<StreamParams>,
    req: Request,
) -> ApiResult<Response> {
    let mut sources = caller.sources();
    if let Some(source) = &p.source {
        if !caller.may_see(source) {
            return Err(forbidden_source(source));
        }
        sources = Some(match sources {
            Some(scope) => scope.into_iter().filter(|s| s == source).collect(),
            None => vec![source.clone()],
        });
    }
    let last_seq = match req
        .headers()
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<i64>().ok())
    {
        Some(seq) => seq,
        None => p.since.unwrap_or(0),
    };
    let scope = StreamScope {
        user_id: caller.user_id(),
        sources,
        last_seq,
    };
    let feed = state.mail_db.feed(caller.user_id());
    let user_id = scope.user_id;
    let replay: VecDeque<FeedEvent> = feed
        .since(None, scope.last_seq, REPLAY_LIMIT)?
        .into_iter()
        .map(|message| FeedEvent::Message {
            user_id,
            message: Box::new(message),
        })
        .collect();
    let rx = state.feed_events.subscribe();

    if p.format.as_deref() == Some("ws") {
        let (mut parts, _) = req.into_parts();
        let ws = WebSocketUpgrade::from_request_parts(&mut parts, &state)
            .await
            .map_err(|_| {
                ApiError::bad_request(
                    "format=ws needs a WebSocket upgrade request (Upgrade: websocket)",
                )
            })?;
        return Ok(ws.on_upgrade(move |socket| run_ws(scope, replay, rx, socket)));
    }

    let stream = live_sse(scope, replay, rx);
    Ok(Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response())
}

fn live_sse(
    mut scope: StreamScope,
    replay: VecDeque<FeedEvent>,
    rx: Receiver<FeedEvent>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let pending: VecDeque<Event> = replay
        .iter()
        .filter_map(|change| scope.event(change))
        .collect();
    stream::unfold(
        (scope, pending, rx),
        |(mut scope, mut pending, mut rx)| async move {
            loop {
                if let Some(event) = pending.pop_front() {
                    return Some((Ok(event), (scope, pending, rx)));
                }
                match rx.recv().await {
                    Ok(change) => {
                        if let Some(event) = scope.event(&change) {
                            return Some((Ok(event), (scope, pending, rx)));
                        }
                    }
                    // A client that fell behind reconnects with `Last-Event-ID` and catches up.
                    Err(RecvError::Lagged(_) | RecvError::Closed) => return None,
                }
            }
        },
    )
}

async fn run_ws(
    mut scope: StreamScope,
    replay: VecDeque<FeedEvent>,
    mut rx: Receiver<FeedEvent>,
    mut socket: WebSocket,
) {
    // SSE puts the JSON in `data`; the WebSocket sends the feed event itself, which has the same
    // fields plus `type`.
    for change in &replay {
        if !scope.accepts(change) {
            continue;
        }
        if let FeedEvent::Message { message, .. } = change {
            scope.last_seq = message.seq;
        }
        let Ok(text) = serde_json::to_string(change) else {
            continue;
        };
        if socket.send(WsMessage::Text(text.into())).await.is_err() {
            return;
        }
    }
    let mut ping = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            change = rx.recv() => match change {
                Ok(change) => {
                    if !scope.accepts(&change) { continue }
                    if let FeedEvent::Message { message, .. } = &change {
                        scope.last_seq = message.seq;
                    }
                    let Ok(text) = serde_json::to_string(&change) else { continue };
                    if socket.send(WsMessage::Text(text.into())).await.is_err() { return }
                }
                Err(RecvError::Lagged(_) | RecvError::Closed) => return,
            },
            msg = socket.recv() => match msg {
                Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(_)) => {}
            },
            _ = ping.tick() => {
                if socket.send(WsMessage::Ping(Default::default())).await.is_err() { return }
            }
        }
    }
}

// ----------------------------------------------------------------- API keys

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewKey {
    /// Something you recognise later in Settings, e.g. "Claude Code on the laptop".
    name: String,
    /// Marks notifications read and archives them; reading is always allowed.
    #[serde(default)]
    write: bool,
    /// Empty: every source.
    #[serde(default)]
    source_ids: Vec<String>,
    /// Days until it stops working; never, when left out.
    expires_days: Option<u32>,
}

#[derive(Serialize, ToSchema)]
pub struct KeyList {
    keys: Vec<Key>,
}

/// The sources one caller may see.
#[derive(Serialize, ToSchema)]
pub struct SourceList {
    pub(crate) sources: Vec<crate::notify::SourceView>,
}

/// A new key, with its token: the only time the token is shown.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatedKey {
    key: Key,
    /// `nb_…`; send it as `Authorization: Bearer` or `X-API-Key`.
    token: String,
}

/// The API keys of this account, newest last. Revoked ones are kept for the record.
#[utoipa::path(
    get,
    path = "/v1/keys",
    tag = "keys",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Every key, without its token", body = KeyList),
        (status = 401, description = "No valid session"),
    )
)]
pub async fn list_keys(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> ApiResult<Json<KeyList>> {
    Ok(Json(KeyList {
        keys: state.db.api_keys(user.id)?,
    }))
}

/// Creates a key. The token is answered once and cannot be read again.
#[utoipa::path(
    post,
    path = "/v1/keys",
    tag = "keys",
    security(("bearer" = [])),
    request_body = inline(NewKey),
    responses(
        (status = 201, description = "The key and its token, shown only here", body = CreatedKey),
        (status = 400, description = "A key needs a name; unknown source ids"),
        (status = 401, description = "No valid session"),
    )
)]
pub async fn create_key(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(input): Json<NewKey>,
) -> ApiResult<(StatusCode, Json<CreatedKey>)> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request(
            "a key needs a name, to tell it apart later",
        ));
    }
    if name.chars().count() > 100 {
        return Err(ApiError::bad_request(
            "the name is longer than 100 characters",
        ));
    }
    if let Some(days) = input.expires_days
        && !(1..=3650).contains(&days)
    {
        return Err(ApiError::bad_request("expiresDays must be 1–3650"));
    }
    let known: Vec<String> = state
        .mail_db
        .feed(user.id)
        .sources()?
        .into_iter()
        .map(|s| s.id)
        .collect();
    if let Some(unknown) = input.source_ids.iter().find(|id| !known.contains(id)) {
        return Err(ApiError::bad_request(format!(
            "unknown source {unknown}; list the sources with GET /v1/sources"
        )));
    }
    let issued = apikeys::new_token();
    let mut scopes = vec!["read"];
    if input.write {
        scopes.push("write");
    }
    let expires_at = input
        .expires_days
        .map(|days| now_ms() + i64::from(days) * 86_400_000);
    let key = state.db.create_api_key(
        user.id,
        &issued,
        &apikeys::NewKey {
            name,
            scopes: &scopes,
            source_ids: &input.source_ids,
            expires_at,
        },
    )?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedKey {
            key,
            token: issued.token,
        }),
    ))
}

/// Revokes a key: it stops working at once and stays in the list for the record.
#[utoipa::path(
    delete,
    path = "/v1/keys/{id}",
    tag = "keys",
    security(("bearer" = [])),
    params(("id" = String, Path, description = "Key id")),
    responses(
        (status = 204, description = "Revoked"),
        (status = 404, description = "No such key"),
    )
)]
pub async fn revoke_key(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    if state.db.revoke_api_key(user.id, &id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

// ---------------------------------------------------------------------- MCP

/// `POST /mcp`: streamable HTTP MCP with the same key auth, answered from this server's own store
/// (no HTTP hop back into `/v1`).
pub fn mcp_router(state: Arc<AppState>) -> Router {
    let make: BackendFactory = {
        let state = state.clone();
        Arc::new(move |key: Option<&str>| {
            Arc::new(StoreNotify::new(state.clone(), key)) as Arc<dyn Notify>
        })
    };
    notify_mcp::router(make).layer(middleware::from_fn_with_state(state, require_key))
}

/// The MCP transport does its own bearer check for the standalone server; here the mount point
/// answers `401` before a tool runs, so a client learns its key is wrong the way it expects.
async fn require_key(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let (mut parts, body) = req.into_parts();
    match Agent::from_request_parts(&mut parts, &state).await {
        Ok(_) => next.run(Request::from_parts(parts, body)).await,
        Err(e) => e.into_response(),
    }
}

/// The tools' backend over the store, with the key's scope applied to every query.
struct StoreNotify {
    state: Arc<AppState>,
    scope: Option<Agent>,
}

impl StoreNotify {
    fn new(state: Arc<AppState>, key: Option<&str>) -> Self {
        let scope = key.and_then(|key| apikeys::resolve(&state, key).ok());
        Self { state, scope }
    }
}

/// The mount point already answered 401 for a bad key; this covers a tool call that somehow
/// arrives without one.
fn scope_of(scope: Option<Agent>) -> Result<Agent, McpError> {
    scope.ok_or_else(|| McpError::Status {
        status: 401,
        code: "ERR_AUTH".into(),
        message: "a valid API key (nb_…) is required".into(),
    })
}

fn mcp_error(e: mail_core::types::Error) -> McpError {
    let status = match e.code {
        ErrorCode::NotFound => 404,
        ErrorCode::Auth | ErrorCode::Unsupported | ErrorCode::InvalidInput => 403,
        ErrorCode::TooLarge => 413,
        _ => 500,
    };
    McpError::Status {
        status,
        code: serde_json::to_value(e.code)
            .ok()
            .and_then(|c| c.as_str().map(str::to_string))
            .unwrap_or_else(|| "ERR_INTERNAL".into()),
        message: e.message,
    }
}

fn to_mcp(message: FeedMessage) -> notify_mcp::Message {
    notify_mcp::Message {
        id: message.id,
        source_id: Some(message.source_id),
        sender: message.sender,
        title: Some(message.title),
        body_text: message.body_text.or(message.body_markdown),
        link: message.link,
        received_at: Some(Value::from(message.received_at)),
        priority: Some(message.priority),
        tags: message.tags.into_iter().map(Value::String).collect(),
        read_at: message.read_at.map(Value::from),
        archived_at: message.archived_at.map(Value::from),
        thread_id: message.thread_id,
    }
}

impl Notify for StoreNotify {
    fn messages(&self, query: ListQuery) -> notify_mcp::BoxFuture<Page> {
        let state = self.state.clone();
        let scope = self.scope.clone();
        Box::pin(async move {
            let agent = scope_of(scope)?;
            let feed = state.mail_db.feed(agent.user_id);
            let page = feed
                .list(&ListFilter {
                    sources: agent.sources.clone(),
                    source: query.source.clone(),
                    unread: query.unread,
                    // A search looks through what was handled too; the unread list does not.
                    archived: if query.unread {
                        Archived::Hide
                    } else {
                        Archived::Any
                    },
                    text: query.q.clone(),
                    since: None,
                    before: query.cursor.as_deref().and_then(decode_cursor),
                    limit: query.limit.min(LIST_LIMIT),
                })
                .map_err(mcp_error)?;
            let next = page
                .more
                .then(|| page.messages.last().map(encode_cursor))
                .flatten();
            Ok(Page {
                messages: page.messages.into_iter().map(to_mcp).collect(),
                next,
            })
        })
    }

    fn message(&self, id: String) -> notify_mcp::BoxFuture<notify_mcp::Message> {
        let state = self.state.clone();
        let scope = self.scope.clone();
        Box::pin(async move {
            let agent = scope_of(scope)?;
            let feed = state.mail_db.feed(agent.user_id);
            let message = feed.message(&id).map_err(mcp_error)?;
            if !agent.may_see(&message.source_id) {
                return Err(mcp_error(mail_core::types::Error::new(
                    ErrorCode::NotFound,
                    format!("no message with id {id}"),
                )));
            }
            Ok(to_mcp(message))
        })
    }

    fn mark_read(&self, id: String) -> notify_mcp::BoxFuture<notify_mcp::Message> {
        let state = self.state.clone();
        let scope = self.scope.clone();
        Box::pin(async move {
            let agent = scope_of(scope)?;
            if !agent.write {
                return Err(McpError::Status {
                    status: 403,
                    code: "ERR_AUTH".into(),
                    message: "this API key may only read; mark_read needs the write scope".into(),
                });
            }
            let feed = state.mail_db.feed(agent.user_id);
            let current = feed.message(&id).map_err(mcp_error)?;
            if !agent.may_see(&current.source_id) {
                return Err(mcp_error(mail_core::types::Error::new(
                    ErrorCode::NotFound,
                    format!("no message with id {id}"),
                )));
            }
            let message = feed.set_read(&id, true).map_err(mcp_error)?;
            let _ = state.feed_events.send(FeedEvent::Read {
                user_id: agent.user_id,
                id: message.id.clone(),
                source_id: message.source_id.clone(),
                seq: message.seq,
                read_at: message.read_at,
            });
            Ok(to_mcp(message))
        })
    }

    fn sources(&self) -> notify_mcp::BoxFuture<Vec<notify_mcp::Source>> {
        let state = self.state.clone();
        let scope = self.scope.clone();
        Box::pin(async move {
            let agent = scope_of(scope)?;
            let sources = state
                .mail_db
                .feed(agent.user_id)
                .sources()
                .map_err(mcp_error)?;
            Ok(sources
                .into_iter()
                .filter(|source| agent.may_see(&source.id))
                .map(|source| notify_mcp::Source {
                    id: source.id,
                    kind: Some(source.kind),
                    name: Some(source.name),
                    enabled: Some(source.enabled),
                    min_priority: Some(source.min_priority),
                })
                .collect())
        })
    }
}

// ------------------------------------------------------------------ OpenAPI

/// The server's API, as `GET /openapi.json` serves it and `GET /docs` renders it.
#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "nsh-mail server",
        description = "Mail and notification API. Every `/v1` path takes the session token from \
                       `POST /v1/auth/login` as `Authorization: Bearer`, and the agent paths \
                       (`/v1/feed`, `/v1/stream`, `/mcp`) also take an API key `nb_…` created in \
                       Settings → API keys, limited to the sources and scopes it was given.",
    ),
    tags(
        (name = "feed", description = "The notification feed: mail and everything else in one list"),
        (name = "keys", description = "API keys for agents"),
        (name = "sources", description = "Where notifications come from"),
        (name = "auth", description = "Sessions"),
        (name = "mail", description = "Mailboxes"),
    ),
    paths(
        crate::api::health,
        crate::api::signup,
        crate::api::login,
        crate::api::logout,
        crate::api::me,
        crate::api::update_me,
        crate::api::list_accounts,
        crate::api::list_messages,
        crate::api::get_message,
        crate::api::search,
        crate::notify::list_sources,
        crate::notify::get_source,
        list_feed,
        get_feed_message,
        read_feed_message,
        archive_feed_message,
        read_feed_messages,
        archive_feed_messages,
        list_keys,
        create_key,
        revoke_key,
        stream,
    ),
    components(schemas(
        ReadBody, ReadMany, ArchiveMany, NewKey, KeyList, CreatedKey, FeedList,
        crate::notify::SourceView,
        SourceList,
        crate::apikeys::Key,
        mail_core::feed::FeedMessage,
        mail_core::feed::Source,
    )),
    modifiers(&Extras),
)]
pub struct ApiDoc;

/// What the derive cannot express: the bearer scheme the paths refer to, and the server the
/// document is served from.
struct Extras;

impl utoipa::Modify for Extras {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::{
            Server,
            security::{Http, HttpAuthScheme, SecurityScheme},
        };
        openapi.servers = Some(vec![Server::new("/")]);
        let mut http = Http::new(HttpAuthScheme::Bearer);
        http.description = Some(
            "Session token from POST /v1/auth/login, or an API key nb_… (Settings → API keys)"
                .into(),
        );
        if let Some(components) = openapi.components.as_mut() {
            components
                .security_schemes
                .insert("bearer".into(), SecurityScheme::Http(http).into());
        }
    }
}

/// The document as `/openapi.json` serves it; CI writes the same bytes to `docs/openapi.json`.
pub fn openapi_json() -> String {
    ApiDoc::openapi()
        .to_pretty_json()
        .unwrap_or_else(|_| "{}".into())
}

use utoipa::OpenApi as _;
