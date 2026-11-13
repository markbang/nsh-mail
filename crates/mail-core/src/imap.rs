//! IMAP over TLS or STARTTLS (never plaintext): login, folder discovery, folder sync, body fetch,
//! flag store, append.

use std::collections::HashSet;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};
use std::time::Duration;

use async_imap::imap_proto::{BodyStructure, ContentEncoding, SectionPath};
use async_imap::types::{Fetch, Flag, NameAttribute};
use base64::Engine as _;
use futures::TryStreamExt;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

use crate::error::err;
use crate::store::{FolderRow, Store};
use crate::types::*;

pub(crate) type Session = async_imap::Session<Stream>;

/// async-imap ends a response stream at EOF without an error, so a command on a connection the
/// server closed "succeeds" with an empty answer (a SELECT with no messages and no UIDVALIDITY,
/// which made sync clear the folder). Every command expects a tagged reply, so EOF is an error.
///
/// It also records whether the connection is gone, so a command that failed on a dead pooled
/// connection can be told apart from one the server answered with an error.
#[derive(Debug)]
pub(crate) struct Stream {
    inner: TlsStream<TcpStream>,
    closed: bool,
}

impl Stream {
    fn new(inner: TlsStream<TcpStream>) -> Self {
        Self {
            inner,
            closed: false,
        }
    }

    pub(crate) fn closed(&self) -> bool {
        self.closed
    }

    fn check<T>(&mut self, r: std::io::Result<T>) -> std::io::Result<T> {
        use std::io::ErrorKind::*;
        if let Err(e) = &r
            && matches!(
                e.kind(),
                UnexpectedEof | ConnectionReset | ConnectionAborted | BrokenPipe | NotConnected
            )
        {
            self.closed = true;
        }
        r
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let r = ready!(Pin::new(&mut self.inner).poll_read(cx, buf));
        let r = r.and_then(|()| {
            if buf.filled().len() == before && buf.remaining() > 0 {
                Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "the server closed the connection",
                ))
            } else {
                Ok(())
            }
        });
        Poll::Ready(self.check(r))
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let r = ready!(Pin::new(&mut self.inner).poll_write(cx, buf));
        Poll::Ready(self.check(r))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let r = ready!(Pin::new(&mut self.inner).poll_flush(cx));
        Poll::Ready(self.check(r))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// Newest messages fetched per folder on first sync (and after a long gap).
const WINDOW: u32 = 500;
/// Messages up to this size are downloaded whole during sync (cached for offline reading and
/// fully indexed); larger ones get their header and their first text part for the snippet.
const FULL_FETCH_MAX: u32 = 32 * 1024;
/// Text parts larger than this get no snippet until the message is opened.
const TEXT_PART_MAX: u32 = 256 * 1024;
/// UIDs per FETCH command, keeping command lines and responses bounded.
const FETCH_BATCH: usize = 100;

pub(crate) enum Login<'a> {
    Password {
        username: &'a str,
        password: &'a str,
    },
    XOAuth2 {
        username: &'a str,
        token: &'a str,
    },
}

pub(crate) struct Conn {
    pub session: Session,
    pub idle: bool,
    pub condstore: bool,
    /// RFC 6851 MOVE; without it a move is COPY, \Deleted and EXPUNGE.
    pub moves: bool,
    /// RFC 4315 UID EXPUNGE, which expunges only the moved message.
    pub uidplus: bool,
}

pub(crate) async fn connect(ep: &Endpoint, login: Login<'_>) -> Result<Conn> {
    let tcp = tokio::time::timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((ep.host.as_str(), ep.port)),
    )
    .await?
    .map_err(|e| {
        err(
            ErrorCode::Network,
            format!("cannot reach {}:{}: {e}", ep.host, ep.port),
        )
    })?;
    // IMAP is small request/response; with Nagle each command waits on the server's delayed ACK (~40 ms).
    tcp.set_nodelay(true)?;
    let client = match ep.security {
        Security::Tls => {
            let mut c = async_imap::Client::new(Stream::new(tls_wrap(tcp, &ep.host).await?));
            greeting(&mut c).await?;
            c
        }
        Security::StartTls => {
            async_imap::Client::new(Stream::new(tls_wrap(starttls(tcp).await?, &ep.host).await?))
        }
    };
    let session = match login {
        Login::Password { username, password } => client.login(username, password).await,
        Login::XOAuth2 { username, token } => {
            client
                .authenticate(
                    "XOAUTH2",
                    XOAuth2 {
                        response: Some(xoauth2(username, token)),
                    },
                )
                .await
        }
    };
    let mut session = session.map_err(|(e, _)| match e {
        async_imap::error::Error::No(msg) | async_imap::error::Error::Bad(msg) => err(
            ErrorCode::Auth,
            format!("IMAP login to {} failed: {msg}", ep.host),
        ),
        other => other.into(),
    })?;
    let caps = session.capabilities().await?;
    if caps.has_str("ID") {
        // NetEase (163/126) refuses SELECT from clients that do not identify themselves.
        let _ = session
            .id([
                ("name", Some("nsh-mail")),
                ("version", Some(env!("CARGO_PKG_VERSION"))),
            ])
            .await;
    }
    Ok(Conn {
        idle: caps.has_str("IDLE"),
        condstore: caps.has_str("CONDSTORE"),
        moves: caps.has_str("MOVE"),
        uidplus: caps.has_str("UIDPLUS"),
        session,
    })
}

async fn tls_wrap(tcp: TcpStream, host: &str) -> Result<TlsStream<TcpStream>> {
    let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|_| {
        err(
            ErrorCode::InvalidInput,
            format!("invalid server name {host}"),
        )
    })?;
    tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio_rustls::TlsConnector::from(crate::tls::client_config()).connect(name, tcp),
    )
    .await?
    .map_err(|e| {
        err(
            ErrorCode::Tls,
            format!("TLS handshake with {host} failed: {e}"),
        )
    })
}

async fn greeting(c: &mut async_imap::Client<Stream>) -> Result<()> {
    match tokio::time::timeout(CONNECT_TIMEOUT, c.read_response()).await?? {
        Some(_) => Ok(()),
        None => Err(err(
            ErrorCode::Network,
            "IMAP server closed the connection before greeting",
        )),
    }
}

/// Plain greeting + `STARTTLS`, by hand: nothing but these two lines ever crosses the wire unencrypted.
async fn starttls(tcp: TcpStream) -> Result<TcpStream> {
    let mut r = BufReader::new(tcp);
    let mut line = String::new();
    let exchange = async {
        r.read_line(&mut line).await?;
        if !line.starts_with("* OK") {
            return Err(err(
                ErrorCode::Protocol,
                format!("unexpected IMAP greeting: {}", line.trim()),
            ));
        }
        r.get_mut().write_all(b"S0 STARTTLS\r\n").await?;
        loop {
            line.clear();
            if r.read_line(&mut line).await? == 0 {
                return Err(err(
                    ErrorCode::Network,
                    "IMAP server closed the connection during STARTTLS",
                ));
            }
            if let Some(rest) = line.strip_prefix("S0 ") {
                return if rest.starts_with("OK") {
                    Ok(())
                } else {
                    Err(err(
                        ErrorCode::Tls,
                        format!("server refused STARTTLS: {}", rest.trim()),
                    ))
                };
            }
        }
    };
    tokio::time::timeout(CONNECT_TIMEOUT, exchange).await??;
    Ok(r.into_inner())
}

pub(crate) fn xoauth2(user: &str, token: &str) -> String {
    format!("user={user}\x01auth=Bearer {token}\x01\x01")
}

struct XOAuth2 {
    response: Option<String>,
}

impl async_imap::Authenticator for XOAuth2 {
    type Response = String;
    // The second challenge carries the server's error JSON; an empty reply ends the exchange.
    fn process(&mut self, _: &[u8]) -> String {
        self.response.take().unwrap_or_default()
    }
}

/// Maps the account's mailboxes to the folder kinds we sync: RFC 6154 attributes first, then
/// common names (decoded from modified UTF-7) for servers without SPECIAL-USE.
pub(crate) async fn discover(session: &mut Session) -> Result<Vec<(FolderKind, String)>> {
    let names: Vec<_> = session
        .list(Some(""), Some("*"))
        .await?
        .try_collect()
        .await?;
    let mut by_attr: Vec<(FolderKind, String)> = Vec::new();
    let mut by_name: Vec<(FolderKind, String)> = Vec::new();
    for n in &names {
        let attrs = n.attributes();
        if attrs.iter().any(|a| matches!(a, NameAttribute::NoSelect))
            || attrs
                .iter()
                .any(|a| matches!(a, NameAttribute::Extension(e) if e.eq_ignore_ascii_case("\\NonExistent")))
        {
            continue;
        }
        let name = n.name().to_string();
        if name.eq_ignore_ascii_case("INBOX") {
            by_attr.push((FolderKind::Inbox, name));
            continue;
        }
        let attr_kind = attrs.iter().find_map(|a| match a {
            NameAttribute::Sent => Some(FolderKind::Sent),
            NameAttribute::Drafts => Some(FolderKind::Drafts),
            NameAttribute::Archive => Some(FolderKind::Archive),
            NameAttribute::Trash => Some(FolderKind::Trash),
            // Gmail has no Archive: archiving takes a message out of the inbox, into All Mail.
            NameAttribute::All => Some(FolderKind::Archive),
            _ => None,
        });
        if let Some(kind) = attr_kind {
            by_attr.push((kind, name));
        } else {
            let decoded = decode_utf7(&name);
            let leaf = match n.delimiter() {
                Some(d) if !d.is_empty() => {
                    decoded.rsplit(d).next().unwrap_or(&decoded).to_string()
                }
                _ => decoded.clone(),
            };
            if let Some(kind) = kind_by_name(&leaf) {
                by_name.push((kind, name));
            }
        }
    }
    let mut out: Vec<(FolderKind, String)> = Vec::new();
    for (kind, name) in by_attr.into_iter().chain(by_name) {
        if !out.iter().any(|(k, _)| *k == kind) {
            out.push((kind, name));
        }
    }
    Ok(out)
}

fn kind_by_name(leaf: &str) -> Option<FolderKind> {
    let l = leaf.trim().to_lowercase();
    Some(match l.as_str() {
        "sent" | "sent items" | "sent messages" | "sent mail" | "已发送" | "已发送邮件" => {
            FolderKind::Sent
        }
        "drafts" | "draft" | "草稿箱" | "草稿" => FolderKind::Drafts,
        "archive" | "archives" | "归档" | "存档" => FolderKind::Archive,
        "trash" | "deleted items" | "deleted messages" | "bin" | "已删除" | "已删除邮件" => {
            FolderKind::Trash
        }
        _ => return None,
    })
}

/// RFC 3501 §5.1.3 modified UTF-7 mailbox names (`&` + base64 of UTF-16BE, `,` for `/`).
fn decode_utf7(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let Some(end) = tail.find('-') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let chunk = &tail[..end];
        if chunk.is_empty() {
            out.push('&');
        } else {
            let b64 = chunk.replace(',', "/");
            match base64::engine::general_purpose::STANDARD_NO_PAD.decode(b64.as_bytes()) {
                Ok(bytes) => {
                    let units: Vec<u16> = bytes
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| u16::from_be_bytes(*c))
                        .collect();
                    out.push_str(&String::from_utf16_lossy(&units));
                }
                Err(_) => {
                    out.push('&');
                    out.push_str(chunk);
                    out.push('-');
                }
            }
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

pub(crate) struct FolderSync {
    pub inserted: Vec<Summary>,
    /// Anything visible changed (inserted, removed, flags).
    pub changed: bool,
    /// First sync of this folder (or after UIDVALIDITY changed): no new-mail notifications.
    pub initial: bool,
}

/// Brings one folder up to date: UIDVALIDITY check, flags of known messages (CONDSTORE when
/// available), removed messages, then messages above the highest UID held, capped to the newest
/// `WINDOW`.
pub(crate) async fn sync_folder(
    conn: &mut Conn,
    store: &Arc<Store>,
    folder: FolderRow,
) -> Result<FolderSync> {
    let use_condstore = conn.condstore;
    let session = &mut conn.session;
    let mbox = if use_condstore {
        session.select_condstore(&folder.name).await?
    } else {
        session.select(&folder.name).await?
    };
    let mut f = folder;
    let mut changed = false;
    if f.uidvalidity.is_some() && f.uidvalidity != mbox.uid_validity {
        let id = f.id;
        store.run(move |s| s.clear_folder(id)).await?;
        f.uidnext = None;
        f.highestmodseq = None;
        changed = true;
    }
    f.uidvalidity = mbox.uid_validity;
    let initial = f.uidnext.is_none();

    let folder_id = f.id;
    let local: Vec<u32> = store.run(move |s| s.uids(folder_id)).await?;
    // New mail is what lies above the highest UID held (RFC 4549 §4.3.1), never the server's
    // UIDNEXT: a server may hand out a UID before the message shows (GreenMail does while it
    // stores a large one), and a sync in between would skip that message for good.
    let known_next = local.last().map_or(1, |u| u + 1);
    let mut flag_updates = Vec::new();
    let mut vanished = Vec::new();
    if let Some(&min) = local.first() {
        let range = format!("{min}:{}", known_next.saturating_sub(1).max(min));
        if mbox.exists == 0 {
            vanished = local.clone();
        } else if let (true, Some(old), Some(new)) =
            (use_condstore, f.highestmodseq, mbox.highest_modseq)
        {
            if old != new {
                let query = format!("(UID FLAGS) (CHANGEDSINCE {old})");
                for fetch in collect(session.uid_fetch(&range, &query).await?).await? {
                    if let Some(uid) = fetch.uid {
                        flag_updates.push(flag_tuple(uid, &fetch));
                    }
                }
            }
            let present = session.uid_search(format!("UID {range}")).await?;
            vanished = local
                .iter()
                .copied()
                .filter(|u| !present.contains(u))
                .collect();
        } else {
            let mut present = HashSet::new();
            for fetch in collect(session.uid_fetch(&range, "(UID FLAGS)").await?).await? {
                if let Some(uid) = fetch.uid {
                    present.insert(uid);
                    flag_updates.push(flag_tuple(uid, &fetch));
                }
            }
            vanished = local
                .iter()
                .copied()
                .filter(|u| !present.contains(u))
                .collect();
        }
    }

    let mut new_messages = Vec::new();
    let mut max_uid = known_next.saturating_sub(1);
    if mbox.exists > 0 {
        let unseen = mbox
            .uid_next
            .map_or(u32::MAX, |n| n.saturating_sub(known_next));
        let items = "(UID FLAGS INTERNALDATE RFC822.SIZE BODYSTRUCTURE)";
        let fetched = if initial || unseen > WINDOW {
            let start = mbox.exists - WINDOW.min(mbox.exists) + 1;
            collect(session.fetch(format!("{start}:*"), items).await?).await?
        } else if unseen > 0 {
            collect(session.uid_fetch(format!("{known_next}:*"), items).await?).await?
        } else {
            Vec::new()
        };
        let mut metas = Vec::new();
        for fetch in fetched {
            let Some(uid) = fetch.uid else { continue };
            if uid < known_next || local.binary_search(&uid).is_ok() {
                continue;
            }
            max_uid = max_uid.max(uid);
            metas.push(Meta::from_fetch(uid, &fetch));
        }
        new_messages = fetch_new(session, metas).await?;
    }
    f.uidnext = Some(max_uid + 1);
    f.highestmodseq = mbox.highest_modseq;

    let state = f.clone();
    let (inserted, flag_changes) = store
        .run(move |s| {
            let flag_changes = s.update_flags(folder_id, &flag_updates)?;
            s.delete_uids(folder_id, &vanished)?;
            let inserted = s.insert_messages(folder_id, &new_messages)?;
            s.set_folder_state(&state)?;
            Ok((inserted, flag_changes + vanished.len()))
        })
        .await?;
    Ok(FolderSync {
        changed: changed || flag_changes > 0 || !inserted.is_empty(),
        inserted,
        initial,
    })
}

async fn collect(
    stream: impl futures::Stream<Item = async_imap::error::Result<Fetch>>,
) -> Result<Vec<Fetch>> {
    Ok(stream.try_collect().await?)
}

fn flag_tuple(uid: u32, fetch: &Fetch) -> (u32, bool, bool) {
    let mut seen = false;
    let mut flagged = false;
    for f in fetch.flags() {
        match f {
            Flag::Seen => seen = true,
            Flag::Flagged => flagged = true,
            _ => {}
        }
    }
    (uid, seen, flagged)
}

struct Meta {
    uid: u32,
    seen: bool,
    flagged: bool,
    size: u32,
    internal_date: Option<i64>,
    shape: Shape,
}

impl Meta {
    fn from_fetch(uid: u32, fetch: &Fetch) -> Self {
        let (_, seen, flagged) = flag_tuple(uid, fetch);
        Self {
            uid,
            seen,
            flagged,
            size: fetch.size.unwrap_or(0),
            internal_date: fetch.internal_date().map(|d| d.timestamp_millis()),
            shape: fetch.bodystructure().map(Shape::of).unwrap_or_default(),
        }
    }
}

/// Downloads new messages in batches: small ones whole, others as header + first text part.
/// (Partial fetches like `BODY[TEXT]<0.4096>` are avoided: some servers, GreenMail among them,
/// answer them with responses imap-proto cannot parse.)
async fn fetch_new(
    session: &mut Session,
    metas: Vec<Meta>,
) -> Result<Vec<crate::store::NewMessage>> {
    let mut groups: Vec<(String, Vec<&Meta>)> = Vec::new();
    for m in &metas {
        let items = if m.size <= FULL_FETCH_MAX {
            "(UID BODY.PEEK[])".to_string()
        } else {
            match &m.shape.text {
                Some(t) => format!("(UID BODY.PEEK[HEADER] BODY.PEEK[{}])", t.section),
                None => "(UID BODY.PEEK[HEADER])".to_string(),
            }
        };
        match groups.iter_mut().find(|(i, _)| *i == items) {
            Some((_, g)) => g.push(m),
            None => groups.push((items, vec![m])),
        }
    }
    let mut out = Vec::new();
    for (items, group) in groups {
        for batch in group.chunks(FETCH_BATCH) {
            let set = batch
                .iter()
                .map(|m| m.uid.to_string())
                .collect::<Vec<_>>()
                .join(",");
            for fetch in collect(session.uid_fetch(&set, &items).await?).await? {
                let Some(meta) = batch.iter().find(|m| Some(m.uid) == fetch.uid) else {
                    continue;
                };
                let mut m = match fetch.body() {
                    Some(raw) => crate::mime::parse_full(raw, meta.internal_date),
                    None => {
                        let text = meta.shape.text.as_ref().and_then(|t| {
                            let data = match t.path.is_empty() {
                                true => fetch.text(),
                                false => fetch.section(&SectionPath::Part(t.path.clone(), None)),
                            }?;
                            Some((t.content_type.as_str(), t.encoding.as_str(), data))
                        });
                        let mut m = crate::mime::parse_summary(
                            fetch.header().unwrap_or_default(),
                            text,
                            meta.internal_date,
                        );
                        m.has_attachments = meta.shape.has_attachments;
                        m
                    }
                };
                m.uid = meta.uid;
                m.seen = meta.seen;
                m.flagged = meta.flagged;
                m.size = meta.size;
                out.push(m);
            }
        }
    }
    out.sort_by_key(|m| m.uid);
    Ok(out)
}

struct TextPart {
    /// Empty for a single-part message (fetched as `TEXT`).
    path: Vec<u32>,
    section: String,
    content_type: String,
    encoding: String,
}

#[derive(Default)]
struct Shape {
    text: Option<TextPart>,
    has_attachments: bool,
}

impl Shape {
    /// The first inline text/plain part (else text/html) and whether anything is attached.
    fn of(bs: &BodyStructure<'_>) -> Self {
        let mut shape = Shape::default();
        let (mut plain, mut html) = (None, None);
        walk(
            bs,
            &mut Vec::new(),
            "mixed",
            &mut shape,
            &mut plain,
            &mut html,
        );
        shape.text = plain.or(html);
        shape
    }
}

fn walk(
    bs: &BodyStructure<'_>,
    path: &mut Vec<u32>,
    parent: &str,
    shape: &mut Shape,
    plain: &mut Option<TextPart>,
    html: &mut Option<TextPart>,
) {
    match bs {
        BodyStructure::Multipart { common, bodies, .. } => {
            for (i, b) in bodies.iter().enumerate() {
                path.push(i as u32 + 1);
                walk(
                    b,
                    path,
                    &common.ty.subtype.to_ascii_lowercase(),
                    shape,
                    plain,
                    html,
                );
                path.pop();
            }
        }
        BodyStructure::Text { common, other, .. } => {
            let attached = common
                .disposition
                .as_ref()
                .is_some_and(|d| d.ty.eq_ignore_ascii_case("attachment"));
            let sub = common.ty.subtype.to_ascii_lowercase();
            let slot = match sub.as_str() {
                "plain" => &mut *plain,
                "html" => &mut *html,
                _ => {
                    shape.has_attachments |= attached;
                    return;
                }
            };
            if attached {
                shape.has_attachments = true;
            } else if slot.is_none() && other.octets <= TEXT_PART_MAX {
                let mut ct = format!("text/{sub}");
                for (k, v) in common.ty.params.iter().flatten() {
                    ct.push_str(&format!("; {k}=\"{}\"", v.replace('"', "")));
                }
                *slot = Some(TextPart {
                    section: if path.is_empty() {
                        "TEXT".into()
                    } else {
                        path.iter()
                            .map(u32::to_string)
                            .collect::<Vec<_>>()
                            .join(".")
                    },
                    path: path.clone(),
                    content_type: ct,
                    encoding: match &other.transfer_encoding {
                        ContentEncoding::SevenBit => "7bit".into(),
                        ContentEncoding::EightBit => "8bit".into(),
                        ContentEncoding::Binary => "binary".into(),
                        ContentEncoding::Base64 => "base64".into(),
                        ContentEncoding::QuotedPrintable => "quoted-printable".into(),
                        ContentEncoding::Other(o) => o.to_string(),
                    },
                });
            }
        }
        // Inline images of an HTML body are not attachments.
        BodyStructure::Basic { .. } if parent == "related" => {}
        BodyStructure::Basic { .. } | BodyStructure::Message { .. } => shape.has_attachments = true,
    }
}

pub(crate) async fn fetch_body(session: &mut Session, folder: &str, uid: u32) -> Result<Vec<u8>> {
    session.examine(folder).await?;
    let fetches = collect(
        session
            .uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")
            .await?,
    )
    .await?;
    fetches
        .iter()
        .find(|f| f.uid == Some(uid))
        .and_then(|f| f.body())
        .map(<[u8]>::to_vec)
        .ok_or_else(|| {
            err(
                ErrorCode::NotFound,
                "the message no longer exists on the server",
            )
        })
}

pub(crate) async fn store_flags(
    session: &mut Session,
    folder: &str,
    uid: u32,
    flags: Flags,
) -> Result<()> {
    session.select(folder).await?;
    for (value, name) in [(flags.seen, "\\Seen"), (flags.flagged, "\\Flagged")] {
        if let Some(on) = value {
            let op = if on { "+FLAGS.SILENT" } else { "-FLAGS.SILENT" };
            let _: Vec<Fetch> = collect(
                session
                    .uid_store(uid.to_string(), format!("{op} ({name})"))
                    .await?,
            )
            .await?;
        }
    }
    Ok(())
}

/// Moves one message to `to`. Without MOVE, a server without UIDPLUS expunges every
/// message marked \Deleted in the folder, as its own clients would.
pub(crate) async fn move_message(conn: &mut Conn, from: &str, uid: u32, to: &str) -> Result<()> {
    conn.session.select(from).await?;
    let uid = uid.to_string();
    if conn.moves {
        conn.session.uid_mv(&uid, to).await?;
        return Ok(());
    }
    conn.session.uid_copy(&uid, to).await?;
    let _: Vec<Fetch> = collect(
        conn.session
            .uid_store(&uid, "+FLAGS.SILENT (\\Deleted)")
            .await?,
    )
    .await?;
    if conn.uidplus {
        let _: Vec<u32> = conn.session.uid_expunge(&uid).await?.try_collect().await?;
    } else {
        let _: Vec<u32> = conn.session.expunge().await?.try_collect().await?;
    }
    Ok(())
}

pub(crate) async fn append(session: &mut Session, folder: &str, raw: &[u8]) -> Result<()> {
    session.append(folder, Some("(\\Seen)"), None, raw).await?;
    Ok(())
}

pub(crate) async fn create(session: &mut Session, folder: &str) -> Result<()> {
    session.create(folder).await?;
    Ok(())
}
