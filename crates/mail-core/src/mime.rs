//! MIME parsing (mail-parser) and HTML sanitizing (ammonia).

use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use mail_parser::{AddressList, MessageParser, MessagePart, MessageRef, PartKind};

use crate::store::NewMessage;
use crate::types::{Address, Attachment};

const SNIPPET_LEN: usize = 200;

/// Envelope from `BODY[HEADER]`, snippet from the first text part fetched on its own
/// (`content type`, `transfer encoding`, raw part bytes).
pub(crate) fn parse_summary(
    header: &[u8],
    text: Option<(&str, &str, &[u8])>,
    internal_date: Option<i64>,
) -> NewMessage {
    let mut m = envelope(header, internal_date);
    if let Some((content_type, encoding, data)) = text {
        let mut part = format!(
            "Content-Type: {content_type}\r\nContent-Transfer-Encoding: {encoding}\r\n\r\n"
        )
        .into_bytes();
        part.extend_from_slice(data);
        if let Some(p) = MessageParser::new().parse(&part) {
            m.snippet = p
                .root()
                .body_preview(SNIPPET_LEN)
                .map(|s| collapse_whitespace(&s))
                .unwrap_or_default();
        }
    }
    m
}

/// A whole message fetched during sync: envelope plus the cached body and its full text.
pub(crate) fn parse_full(raw: &[u8], internal_date: Option<i64>) -> NewMessage {
    let mut m = envelope(raw, internal_date);
    let body = parse_body(raw);
    m.has_attachments = !body.attachments.is_empty();
    m.index_text = Some(body.index_text);
    m.body = Some(raw.to_vec());
    m
}

fn envelope(raw: &[u8], internal_date: Option<i64>) -> NewMessage {
    let parser = MessageParser::new();
    let parsed = parser.parse(&raw);
    let Some(msg) = parsed.as_ref() else {
        return NewMessage {
            uid: 0,
            date: internal_date.unwrap_or(0),
            from: None,
            to: Vec::new(),
            cc: Vec::new(),
            subject: String::new(),
            snippet: String::new(),
            message_id: None,
            in_reply_to: None,
            references: Vec::new(),
            seen: false,
            flagged: false,
            has_attachments: false,
            size: 0,
            body: None,
            index_text: None,
        };
    };
    let m = msg.root();
    NewMessage {
        uid: 0,
        date: m
            .date()
            .filter(|d| d.is_valid())
            .map(|d| d.to_timestamp() * 1000)
            .or(internal_date)
            .unwrap_or(0),
        from: m.from().and_then(|l| addresses(Some(l)).into_iter().next()),
        to: addresses(m.to()),
        cc: addresses(m.cc()),
        subject: m.subject().unwrap_or_default().trim().to_string(),
        snippet: m
            .body_preview(SNIPPET_LEN)
            .map(|s| collapse_whitespace(&s))
            .unwrap_or_default(),
        message_id: m.message_id().map(str::to_string),
        in_reply_to: m.in_reply_to().and_then(|l| l.first().map(str::to_string)),
        references: m
            .references()
            .map(|l| l.iter().map(str::to_string).collect())
            .unwrap_or_default(),
        seen: false,
        flagged: false,
        has_attachments: m.has_attachments(),
        size: 0,
        body: None,
        index_text: None,
    }
}

pub(crate) struct Body {
    pub text: Option<String>,
    pub html: Option<String>,
    pub remote_images: Vec<String>,
    pub attachments: Vec<Attachment>,
    pub reply_to: Vec<Address>,
    /// Plain text for the search index.
    pub index_text: String,
}

pub(crate) fn parse_body(raw: &[u8]) -> Body {
    let parser = MessageParser::new();
    let Some(msg) = parser.parse(raw) else {
        let text = String::from_utf8_lossy(raw).into_owned();
        return Body {
            index_text: text.clone(),
            text: Some(text),
            html: None,
            remote_images: Vec::new(),
            attachments: Vec::new(),
            reply_to: Vec::new(),
        };
    };
    let m = msg.root();
    let text = first_text(&m);
    let (html, remote_images) = match m.html_body().next() {
        Some(part) if matches!(part.kind(), PartKind::Html) => match part.text() {
            Some(h) => {
                let (clean, remote) = sanitize_html(&h);
                (Some(clean), remote)
            }
            None => (None, Vec::new()),
        },
        _ => (None, Vec::new()),
    };
    let attachments = m
        .attachments()
        .enumerate()
        .map(|(i, part)| attachment_meta(i as u32, &part))
        .collect();
    Body {
        index_text: text.clone().unwrap_or_default(),
        text,
        html,
        remote_images,
        attachments,
        reply_to: addresses(m.reply_to()),
    }
}

fn attachment_meta(index: u32, part: &MessagePart<'_>) -> Attachment {
    Attachment {
        index,
        filename: part.attachment_name().map(str::to_string),
        mime_type: part
            .content_type()
            .map(|ct| match ct.subtype() {
                Some(sub) => format!("{}/{}", ct.ctype(), sub),
                None => ct.ctype().to_string(),
            })
            .unwrap_or_else(|| "application/octet-stream".into())
            .to_ascii_lowercase(),
        size: part.decoded_len() as u64,
        inline: part.is_inline(),
        content_id: part.content_id().map(str::to_string),
    }
}

/// Attachment `index` (as `parse_body` numbers them) and its decoded bytes.
pub(crate) fn attachment(raw: &[u8], index: u32) -> Option<(Attachment, Vec<u8>)> {
    let msg = MessageParser::new().parse(raw)?;
    let part = msg.root().attachments().nth(index as usize)?;
    Some((attachment_meta(index, &part), part.decoded().into_owned()))
}

fn first_text(m: &MessageRef<'_>) -> Option<String> {
    let part = m.text_body().next()?;
    let t = part.text()?;
    Some(match part.kind() {
        PartKind::Html => mail_parser::html_to_text(&t),
        _ => t.replace("\r\n", "\n"),
    })
}

fn addresses(list: Option<AddressList<'_>>) -> Vec<Address> {
    let Some(list) = list else {
        return Vec::new();
    };
    list.mailboxes()
        .filter_map(|mb| {
            Some(Address {
                name: mb
                    .name()
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(str::to_string),
                email: mb.address()?.trim().to_string(),
            })
        })
        .collect()
}

fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Removes scripts, styles, event handlers and unsafe URLs. Remote images (`http(s)://` or
/// protocol-relative `src`) are dropped to block tracking; their URLs are returned so the UI can
/// offer "load images".
pub(crate) fn sanitize_html(html: &str) -> (String, Vec<String>) {
    let remote = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = remote.clone();
    let clean = ammonia::Builder::default()
        .add_url_schemes(["cid"])
        .link_rel(Some("noopener noreferrer"))
        .attribute_filter(move |element, attribute, value| {
            if element == "img" && attribute == "src" {
                let v = value.trim();
                let lower = v.to_ascii_lowercase();
                if lower.starts_with("http://")
                    || lower.starts_with("https://")
                    || lower.starts_with("//")
                {
                    sink.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(v.to_string());
                    return None;
                }
            }
            Some(Cow::Borrowed(value))
        })
        .clean(html)
        .to_string();
    let remote = std::mem::take(&mut *remote.lock().unwrap_or_else(|e| e.into_inner()));
    (clean, remote)
}
