//! SMTP submission with lettre over implicit TLS or required STARTTLS.

use std::time::Duration;

use base64::Engine as _;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart, header::ContentType};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

use crate::engine::StagedFile;
use crate::error::err;
use crate::types::*;

pub(crate) enum SmtpAuth {
    Password { username: String, password: String },
    XOAuth2 { username: String, token: String },
}

fn transport(ep: &Endpoint, auth: SmtpAuth) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    let tls = TlsParameters::builder(ep.host.clone())
        .dangerous_accept_invalid_certs(crate::tls::insecure())
        .dangerous_accept_invalid_hostnames(crate::tls::insecure())
        .build_rustls()
        .map_err(|e| {
            err(
                ErrorCode::Tls,
                format!("TLS setup for {} failed: {e}", ep.host),
            )
        })?;
    let (credentials, mechanisms) = match auth {
        SmtpAuth::Password { username, password } => (
            Credentials::new(username, password),
            vec![Mechanism::Plain, Mechanism::Login],
        ),
        SmtpAuth::XOAuth2 { username, token } => {
            (Credentials::new(username, token), vec![Mechanism::Xoauth2])
        }
    };
    Ok(
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(ep.host.as_str())
            .port(ep.port)
            .tls(match ep.security {
                Security::Tls => Tls::Wrapper(tls),
                Security::StartTls => Tls::Required(tls),
            })
            .credentials(credentials)
            .authentication(mechanisms)
            .timeout(Some(Duration::from_secs(60)))
            .build(),
    )
}

/// Connects and authenticates without sending anything.
pub(crate) async fn verify(ep: &Endpoint, auth: SmtpAuth) -> Result<()> {
    transport(ep, auth)?
        .test_connection()
        .await
        .map_err(|e| login_error(ep, e))?;
    Ok(())
}

pub(crate) async fn send(ep: &Endpoint, auth: SmtpAuth, message: &lettre::Message) -> Result<()> {
    transport(ep, auth)?
        .send(message.clone())
        .await
        .map_err(|e| login_error(ep, e))?;
    Ok(())
}

fn login_error(ep: &Endpoint, e: lettre::transport::smtp::Error) -> Error {
    let mut out: Error = e.into();
    out.message = format!("{} ({}:{})", out.message, ep.host, ep.port);
    out
}

fn mailbox(a: &Address) -> Result<Mailbox> {
    let email = a.email.trim().parse().map_err(|_| {
        err(
            ErrorCode::InvalidInput,
            format!("invalid email address: {}", a.email),
        )
    })?;
    Ok(Mailbox::new(
        a.name.clone().filter(|n| !n.is_empty()),
        email,
    ))
}

fn angle(id: &str) -> String {
    let id = id.trim();
    if id.starts_with('<') {
        id.to_string()
    } else {
        format!("<{id}>")
    }
}

pub(crate) fn build(
    account: &Account,
    draft: &Draft,
    files: Vec<StagedFile>,
) -> Result<lettre::Message> {
    if draft.to.is_empty() && draft.cc.is_empty() && draft.bcc.is_empty() {
        return Err(err(
            ErrorCode::InvalidInput,
            "the message has no recipients",
        ));
    }
    let from = mailbox(&Address {
        name: account.name.clone(),
        email: account.email.clone(),
    })?;
    let domain = account
        .email
        .rsplit('@')
        .next()
        .unwrap_or("localhost")
        .to_string();
    let mut b = lettre::Message::builder()
        .from(from)
        .subject(draft.subject.clone())
        .message_id(Some(format!("<{}@{}>", crate::store::new_id(), domain)));
    for a in &draft.to {
        b = b.to(mailbox(a)?);
    }
    for a in &draft.cc {
        b = b.cc(mailbox(a)?);
    }
    for a in &draft.bcc {
        b = b.bcc(mailbox(a)?);
    }
    if let Some(parent) = &draft.in_reply_to {
        b = b.in_reply_to(angle(parent));
        let mut refs: Vec<String> = draft.references.iter().map(|r| angle(r)).collect();
        let parent = angle(parent);
        if !refs.contains(&parent) {
            refs.push(parent);
        }
        b = b.references(refs.join(" "));
    }
    let text = || {
        SinglePart::builder()
            .header(ContentType::TEXT_PLAIN)
            .body(draft.text.clone())
    };
    let built = match (&draft.html, files.is_empty()) {
        (Some(html), true) => b.multipart(MultiPart::alternative_plain_html(
            draft.text.clone(),
            html.clone(),
        )),
        (None, true) => b.singlepart(text()),
        (html, false) => {
            let mut mixed = match html {
                Some(html) => MultiPart::mixed().multipart(MultiPart::alternative_plain_html(
                    draft.text.clone(),
                    html.clone(),
                )),
                None => MultiPart::mixed().singlepart(text()),
            };
            for file in files {
                mixed = mixed.singlepart(attachment(file));
            }
            b.multipart(mixed)
        }
    };
    built.map_err(|e| {
        err(
            ErrorCode::InvalidInput,
            format!("cannot build the message: {e}"),
        )
    })
}

/// An attachment part. The name goes in `Content-Disposition` as RFC 2231 (lettre) and in
/// `Content-Type`'s `name` as RFC 2047, which older clients read instead.
fn attachment(file: StagedFile) -> SinglePart {
    let filename: String = file
        .filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let filename = if filename.trim().is_empty() {
        "attachment".to_string()
    } else {
        filename
    };
    let essence = file
        .mime_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let essence = if essence.contains('/') && ContentType::parse(&essence).is_ok() {
        essence
    } else {
        "application/octet-stream".into()
    };
    let content_type =
        ContentType::parse(&format!("{essence}; name=\"{}\"", encoded_word(&filename)))
            .or_else(|_| ContentType::parse(&essence))
            .unwrap_or(ContentType::TEXT_PLAIN);
    Attachment::new(filename).body(file.data, content_type)
}

/// `name` as RFC 2047 encoded words when it is not plain ASCII; otherwise quoted-string safe.
fn encoded_word(name: &str) -> String {
    if name.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        return name.replace(['"', '\\'], "_");
    }
    // Each word stays under 75 characters: 45 bytes of UTF-8 encode to 60.
    let mut words = Vec::new();
    let mut chunk = String::new();
    for c in name.chars() {
        if chunk.len() + c.len_utf8() > 45 {
            words.push(std::mem::take(&mut chunk));
        }
        chunk.push(c);
    }
    words.push(chunk);
    words
        .iter()
        .map(|w| {
            format!(
                "=?utf-8?b?{}?=",
                base64::engine::general_purpose::STANDARD.encode(w)
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}
