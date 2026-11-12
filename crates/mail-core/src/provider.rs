//! Server settings for an email address: built-in presets, then Mozilla's ISPDB autoconfig.

use crate::types::*;

struct Preset {
    id: &'static str,
    name: &'static str,
    domains: &'static [&'static str],
    imap: (&'static str, u16, Security),
    smtp: (&'static str, u16, Security),
    oauth: Option<OAuthProvider>,
    app_password: bool,
    help_url: Option<&'static str>,
}

use Security::{StartTls, Tls};

const PRESETS: &[Preset] = &[
    Preset {
        id: "gmail",
        name: "Gmail",
        domains: &["gmail.com", "googlemail.com"],
        imap: ("imap.gmail.com", 993, Tls),
        smtp: ("smtp.gmail.com", 465, Tls),
        oauth: Some(OAuthProvider::Google),
        app_password: false,
        help_url: None,
    },
    Preset {
        id: "outlook",
        name: "Outlook",
        domains: &[
            "outlook.com",
            "hotmail.com",
            "live.com",
            "msn.com",
            "outlook.cn",
            "hotmail.co.uk",
            "hotmail.fr",
            "hotmail.de",
            "hotmail.it",
            "hotmail.es",
            "live.cn",
            "live.co.uk",
            "live.fr",
            "live.de",
        ],
        imap: ("outlook.office365.com", 993, Tls),
        smtp: ("smtp.office365.com", 587, StartTls),
        oauth: Some(OAuthProvider::Microsoft),
        app_password: false,
        help_url: None,
    },
    Preset {
        id: "icloud",
        name: "iCloud Mail",
        domains: &["icloud.com", "me.com", "mac.com"],
        imap: ("imap.mail.me.com", 993, Tls),
        smtp: ("smtp.mail.me.com", 587, StartTls),
        oauth: None,
        app_password: true,
        help_url: Some("https://support.apple.com/en-us/102654"),
    },
    Preset {
        id: "yahoo",
        name: "Yahoo Mail",
        domains: &[
            "yahoo.com",
            "ymail.com",
            "rocketmail.com",
            "yahoo.co.uk",
            "yahoo.fr",
            "yahoo.de",
        ],
        imap: ("imap.mail.yahoo.com", 993, Tls),
        smtp: ("smtp.mail.yahoo.com", 465, Tls),
        oauth: None,
        app_password: true,
        help_url: Some("https://help.yahoo.com/kb/SLN15241.html"),
    },
    Preset {
        id: "fastmail",
        name: "Fastmail",
        domains: &["fastmail.com", "fastmail.fm"],
        imap: ("imap.fastmail.com", 993, Tls),
        smtp: ("smtp.fastmail.com", 465, Tls),
        oauth: None,
        app_password: true,
        help_url: Some("https://www.fastmail.help/hc/en-us/articles/360058752854"),
    },
    Preset {
        id: "qq",
        name: "QQ 邮箱",
        domains: &["qq.com", "vip.qq.com", "foxmail.com"],
        imap: ("imap.qq.com", 993, Tls),
        smtp: ("smtp.qq.com", 465, Tls),
        oauth: None,
        app_password: true,
        help_url: Some("https://service.mail.qq.com/detail/0/75"),
    },
    Preset {
        id: "163",
        name: "网易 163 邮箱",
        domains: &["163.com"],
        imap: ("imap.163.com", 993, Tls),
        smtp: ("smtp.163.com", 465, Tls),
        oauth: None,
        app_password: true,
        help_url: Some("https://help.mail.163.com/faq.do?m=list&categoryID=90"),
    },
    Preset {
        id: "126",
        name: "网易 126 邮箱",
        domains: &["126.com"],
        imap: ("imap.126.com", 993, Tls),
        smtp: ("smtp.126.com", 465, Tls),
        oauth: None,
        app_password: true,
        help_url: Some("https://help.mail.163.com/faq.do?m=list&categoryID=90"),
    },
    Preset {
        id: "yandex",
        name: "Yandex Mail",
        domains: &["yandex.com", "yandex.ru", "ya.ru", "yandex.by", "yandex.kz"],
        imap: ("imap.yandex.com", 993, Tls),
        smtp: ("smtp.yandex.com", 465, Tls),
        oauth: None,
        app_password: true,
        help_url: Some("https://yandex.com/support/id/en/authorization/app-passwords"),
    },
    Preset {
        id: "zoho",
        name: "Zoho Mail",
        domains: &["zoho.com", "zohomail.com"],
        imap: ("imap.zoho.com", 993, Tls),
        smtp: ("smtp.zoho.com", 465, Tls),
        oauth: None,
        app_password: true,
        help_url: Some(
            "https://www.zoho.com/mail/help/adminconsole/two-factor-authentication.html",
        ),
    },
    Preset {
        id: "gmx",
        name: "GMX",
        domains: &["gmx.com", "gmx.us", "gmx.co.uk"],
        imap: ("imap.gmx.com", 993, Tls),
        smtp: ("mail.gmx.com", 587, StartTls),
        oauth: None,
        app_password: false,
        help_url: Some("https://support.gmx.com/pop-imap/toggle.html"),
    },
    Preset {
        id: "gmx",
        name: "GMX",
        domains: &["gmx.net", "gmx.de", "gmx.at", "gmx.ch"],
        imap: ("imap.gmx.net", 993, Tls),
        smtp: ("mail.gmx.net", 587, StartTls),
        oauth: None,
        app_password: false,
        help_url: Some("https://hilfe.gmx.net/pop-imap/einschalten.html"),
    },
];

fn split(email: &str) -> Option<(&str, String)> {
    let (local, domain) = email.trim().rsplit_once('@')?;
    if local.is_empty() || domain.is_empty() {
        return None;
    }
    Some((local, domain.to_ascii_lowercase()))
}

/// Settings from the built-in table only (no network).
pub fn preset(email: &str) -> Option<ServerSettings> {
    let (_, domain) = split(email)?;
    let p = PRESETS
        .iter()
        .find(|p| p.domains.contains(&domain.as_str()))?;
    let ep = |(host, port, security): (&str, u16, Security)| Endpoint {
        host: host.to_string(),
        port,
        security,
    };
    Some(ServerSettings {
        provider: Some(p.id.to_string()),
        display_name: Some(p.name.to_string()),
        username: email.trim().to_string(),
        imap: ep(p.imap),
        smtp: ep(p.smtp),
        auth: match p.oauth {
            Some(provider) => Auth::OAuth { provider },
            None => Auth::Password,
        },
        app_password: p.app_password,
        help_url: p.help_url.map(str::to_string),
    })
}

/// Presets first, then `https://autoconfig.thunderbird.net/v1.1/<domain>`. `Ok(None)` when
/// nothing is known about the domain.
pub async fn detect(http: &reqwest::Client, email: &str) -> Result<Option<ServerSettings>> {
    if let Some(s) = preset(email) {
        return Ok(Some(s));
    }
    let Some((_, domain)) = split(email) else {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            format!("not an email address: {email}"),
        ));
    };
    let url = format!("https://autoconfig.thunderbird.net/v1.1/{domain}");
    let resp = http.get(&url).send().await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let xml = resp.error_for_status()?.text().await?;
    Ok(parse_ispdb(&xml, email))
}

/// Picks the first IMAP and SMTP server with TLS (preferred) or STARTTLS; plaintext entries are
/// ignored.
pub fn parse_ispdb(xml: &str, email: &str) -> Option<ServerSettings> {
    let (local, domain) = split(email)?;
    let doc = roxmltree::Document::parse(xml).ok()?;
    let provider = doc
        .descendants()
        .find(|n| n.has_tag_name("emailProvider"))?;
    let child_text = |n: roxmltree::Node, tag: &str| {
        n.children()
            .find(|c| c.has_tag_name(tag))
            .and_then(|c| c.text())
            .map(|t| t.trim().to_string())
    };
    let fill = |s: &str| {
        s.replace("%EMAILADDRESS%", email.trim())
            .replace("%EMAILLOCALPART%", local)
            .replace("%EMAILDOMAIN%", &domain)
    };
    let entry = |n: roxmltree::Node| -> Option<(Endpoint, String, bool)> {
        let security = match child_text(n, "socketType")?.to_ascii_uppercase().as_str() {
            "SSL" | "TLS" => Tls,
            "STARTTLS" => StartTls,
            _ => return None,
        };
        let ep = Endpoint {
            host: fill(&child_text(n, "hostname")?),
            port: child_text(n, "port")?.parse().ok()?,
            security,
        };
        let username = fill(&child_text(n, "username").unwrap_or_else(|| "%EMAILADDRESS%".into()));
        let oauth = n
            .children()
            .filter(|c| c.has_tag_name("authentication"))
            .any(|c| c.text().is_some_and(|t| t.trim() == "OAuth2"));
        Some((ep, username, oauth))
    };
    let pick = |tag: &str, kind: &str| {
        let mut candidates = provider
            .children()
            .filter(|n| n.has_tag_name(tag) && n.attribute("type") == Some(kind))
            .filter_map(entry);
        let first = candidates.next()?;
        if first.0.security == Tls {
            return Some(first);
        }
        Some(candidates.find(|c| c.0.security == Tls).unwrap_or(first))
    };
    let (imap, username, imap_oauth) = pick("incomingServer", "imap")?;
    let (smtp, _, _) = pick("outgoingServer", "smtp")?;
    let oauth = match imap.host.as_str() {
        h if imap_oauth && (h.ends_with(".gmail.com") || h.ends_with(".googlemail.com")) => {
            Some(OAuthProvider::Google)
        }
        h if imap_oauth && (h.ends_with(".office365.com") || h.ends_with(".outlook.com")) => {
            Some(OAuthProvider::Microsoft)
        }
        _ => None,
    };
    Some(ServerSettings {
        provider: None,
        display_name: child_text(provider, "displayName"),
        username,
        imap,
        smtp,
        auth: match oauth {
            Some(provider) => Auth::OAuth { provider },
            None => Auth::Password,
        },
        app_password: false,
        help_url: None,
    })
}
