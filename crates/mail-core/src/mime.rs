//! MIME parsing (mail-parser) and HTML sanitizing (ammonia).

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mail_parser::{AddressList, MessageParser, MessagePart, MessageRef, PartKind};

use crate::engine::lock;
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
    let (index_text, has_attachments) = index(raw);
    m.has_attachments = has_attachments;
    m.index_text = Some(index_text);
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
}

/// The search index's text and whether anything is attached: what syncing needs of a body.
pub(crate) fn index(raw: &[u8]) -> (String, bool) {
    match MessageParser::new().parse(raw) {
        Some(msg) => (
            first_text(&msg.root()).unwrap_or_default(),
            msg.root().attachments().len() > 0,
        ),
        None => (String::from_utf8_lossy(raw).into_owned(), false),
    }
}

pub(crate) fn parse_body(raw: &[u8]) -> Body {
    let parser = MessageParser::new();
    let Some(msg) = parser.parse(raw) else {
        return Body {
            text: Some(String::from_utf8_lossy(raw).into_owned()),
            html: None,
            remote_images: Vec::new(),
            attachments: Vec::new(),
            reply_to: Vec::new(),
        };
    };
    let m = msg.root();
    let text = first_text(&m);
    let mut budget = INLINE_BUDGET;
    let (html, remote_images, shown) = match html_of(&m, &mut budget) {
        Some(h) => {
            let (clean, remote, shown) = sanitize_html(&h, &inline_images(&msg, &mut budget));
            (Some(clean), remote, shown)
        }
        None => (None, Vec::new(), Vec::new()),
    };
    // Images the HTML shows are not listed again as attachments; indices stay those of `attachment`.
    let attachments = m
        .attachments()
        .enumerate()
        .filter(|(_, part)| {
            !part
                .content_id()
                .is_some_and(|id| shown.iter().any(|s| s == id.trim()))
        })
        .map(|(i, part)| attachment_meta(i as u32, &part))
        .collect();
    Body {
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

/// The HTML body: its HTML parts in reading order, with the text parts and inline images between
/// them (Apple Mail's `multipart/mixed`). `None` when the message has no HTML part: it reads as text.
fn html_of(m: &MessageRef<'_>, budget: &mut usize) -> Option<String> {
    let parts: Vec<_> = m.html_body().collect();
    if !parts.iter().any(|p| matches!(p.kind(), PartKind::Html)) {
        return None;
    }
    let mut html = String::new();
    for part in parts {
        match part.kind() {
            PartKind::Html => html.push_str(&part.text().unwrap_or_default()),
            PartKind::Text => {
                html.push_str(&mail_parser::text_to_html(&part.text().unwrap_or_default()))
            }
            _ => {
                if let Some(uri) = data_uri(&part, budget) {
                    html.push_str(&format!("<img src=\"{uri}\">"));
                }
            }
        }
    }
    Some(html)
}

/// Decoded bytes inlined as `data:` URIs, over all of a message's images: the JSON of one message
/// stays a few megabytes.
const INLINE_BUDGET: usize = 10 * 1024 * 1024;

/// `cid:` targets (Content-ID without brackets) as `data:` URIs.
fn inline_images(msg: &mail_parser::Message<'_>, budget: &mut usize) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for part in msg.root().parts() {
        if let Some(id) = part.content_id()
            && let Some(uri) = data_uri(&part, budget)
        {
            out.insert(id.trim().to_string(), uri);
        }
    }
    out
}

/// An image part as a `data:` URI, while `budget` (decoded bytes) lasts.
fn data_uri(part: &MessagePart<'_>, budget: &mut usize) -> Option<String> {
    use base64::Engine as _;
    let ct = part.content_type()?;
    let sub = ct.subtype()?;
    if !ct.ctype().eq_ignore_ascii_case("image") || part.decoded_len() > *budget {
        return None;
    }
    let mime = format!("image/{sub}").to_ascii_lowercase();
    if !mime
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"/+.-".contains(&b))
    {
        return None;
    }
    *budget -= part.decoded_len();
    let data = base64::engine::general_purpose::STANDARD.encode(part.decoded());
    Some(format!("data:{mime};base64,{data}"))
}

/// Mail HTML for a view without scripts: scripts, event handlers, `javascript:` URLs, forms,
/// frames, objects, `<meta>`, `<link>` and `<base>` go; styles, classes and layout attributes
/// stay. `cid:` images become the `data:` URIs of `inline`. Remote URLs (images, CSS `url()`,
/// `@import`) stay, for the reader to block until the user asks; they are returned so it can
/// offer that, with the Content-IDs of the inline images shown.
pub(crate) fn sanitize_html(
    html: &str,
    inline: &HashMap<String, String>,
) -> (String, Vec<String>, Vec<String>) {
    let remote = Arc::new(Mutex::new(Vec::<String>::new()));
    let shown = Arc::new(Mutex::new(Vec::<String>::new()));
    let (sink, shown_sink) = (remote.clone(), shown.clone());
    let inline = inline.clone();
    let mut builder = ammonia::Builder::default();
    builder
        .add_tags(["style", "font", "tfoot"])
        .clean_content_tags(["script", "title"].into())
        .add_generic_attributes([
            "style",
            "class",
            "id",
            "dir",
            "align",
            "valign",
            "width",
            "height",
            "bgcolor",
            "background",
            "color",
            "face",
            "size",
            "border",
            "cellpadding",
            "cellspacing",
            "colspan",
            "rowspan",
        ])
        .add_url_schemes(["cid", "data"])
        .link_rel(Some("noopener noreferrer"))
        .attribute_filter(move |element, attribute, value| {
            let v = value.trim();
            let lower = v.to_ascii_lowercase();
            let image = element == "img" && attribute == "src";
            let url = matches!(attribute, "href" | "src" | "cite" | "xlink:href");
            if url && lower.starts_with("data:") && !(image && lower.starts_with("data:image/")) {
                return None;
            }
            if image && let Some(id) = lower.strip_prefix("cid:").map(|_| &v[4..]) {
                let id = percent_decode(id);
                let found = inline
                    .get_key_value(id.as_str())
                    .or_else(|| inline.iter().find(|(k, _)| k.eq_ignore_ascii_case(&id)));
                let (key, uri) = found?;
                lock(&shown_sink).push(key.clone());
                return Some(Cow::Owned(uri.clone()));
            }
            if (image || attribute == "background") && is_remote(&lower) {
                lock(&sink).push(v.to_string());
            }
            Some(Cow::Borrowed(value))
        });
    let clean = builder.clean(html).to_string();
    let mut remote = std::mem::take(&mut *lock(&remote));
    remote.extend(css_urls(&clean));
    let shown = std::mem::take(&mut *lock(&shown));
    (clean, remote, shown)
}

fn is_remote(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("//")
}

/// The remote URLs of CSS in `html` (`url(…)` and `@import "…"`, in `<style>` and `style`).
fn css_urls(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    for key in ["url(", "@import"] {
        for (at, _) in lower.match_indices(key) {
            let rest = html[at + key.len()..].trim_start();
            let rest = rest.trim_start_matches(['"', '\'', '(']).trim_start();
            let end = rest
                .find(|c: char| c == '"' || c == '\'' || c == ')' || c.is_whitespace() || c == ';')
                .unwrap_or(rest.len());
            let url = rest[..end].replace("&amp;", "&");
            if is_remote(&url.to_ascii_lowercase()) {
                out.push(url);
            }
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match (
            b[i],
            b.get(i + 1).copied().and_then(hex),
            b.get(i + 2).copied().and_then(hex),
        ) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, _, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
