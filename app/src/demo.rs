//! `DemoBackend`: three accounts and a few weeks of mail, kept in memory,
//! for the UI tests (debug builds only). Sending, flags, search and adding
//! accounts behave as the real backends do; refreshing delivers a new
//! message, and so does a timer while the app runs.

use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use mail_core::types::*;
use nsh_core::{prelude::*, time::unix_ms};

use crate::backend::{Backend, Events, MailFuture, invalid};

const MINUTE: f64 = 60_000.0;
const HOUR: f64 = 60.0 * MINUTE;
const DAY: f64 = 24.0 * HOUR;
/// How often a message arrives on its own.
const ARRIVAL: Duration = Duration::from_secs(90);

#[derive(Default)]
struct Data {
    accounts: Vec<Account>,
    messages: Vec<Message>,
    next_id: u64,
    arrivals: usize,
    events: Option<Events>,
}

impl Data {
    fn id(&mut self) -> String {
        self.next_id += 1;
        format!("m{}", self.next_id)
    }

    fn emit(&self, event: MailEvent) {
        if let Some(events) = &self.events {
            events(event);
        }
    }

    fn account(&self, id: &str) -> Result<&Account> {
        self.accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| not_found("account"))
    }

    fn add_account(&mut self, mut account: Account) -> Result<Account> {
        account.email = account.email.trim().to_lowercase();
        if self.accounts.iter().any(|a| a.email == account.email) {
            return Err(invalid(format!("{} is already added", account.email)));
        }
        account.id = format!("a-{}", account.email.replace(['@', '.'], "-"));
        self.accounts.push(account.clone());
        let id = self.id();
        let text = format!(
            "{} is now part of your unified inbox.\n\nNew mail appears here as it arrives. \
             Pull down (or press Refresh) to check right away.",
            account.email
        );
        let m = message(
            id,
            &account,
            FolderKind::Inbox,
            unix_ms(),
            ("nsh-mail", "hello@nsh-mail.app"),
            "Your account is connected",
            &text,
        );
        self.messages.push(m);
        Ok(account)
    }

    fn deliver(&mut self) -> Option<Summary> {
        if self.accounts.is_empty() {
            return None;
        }
        let (from, subject, text) = ARRIVALS[self.arrivals % ARRIVALS.len()];
        let account = self.accounts[self.arrivals % self.accounts.len()].clone();
        self.arrivals += 1;
        let id = self.id();
        let m = message(
            id,
            &account,
            FolderKind::Inbox,
            unix_ms(),
            from,
            subject,
            text,
        );
        let summary = m.summary.clone();
        self.messages.push(m);
        Some(summary)
    }

    /// Newest first; equal dates by id, so a page boundary is stable.
    fn sorted(&self, keep: impl Fn(&Message) -> bool) -> Vec<&Message> {
        let mut found: Vec<&Message> = self.messages.iter().filter(|m| keep(m)).collect();
        found.sort_by(|a, b| {
            let (a, b) = (&a.summary, &b.summary);
            b.date.cmp(&a.date).then_with(|| b.id.cmp(&a.id))
        });
        found
    }
}

/// In-memory mail (see the module's docs).
pub struct DemoBackend {
    data: Rc<RefCell<Data>>,
}

impl Default for DemoBackend {
    fn default() -> Self {
        Self {
            data: Rc::new(RefCell::new(seed(unix_ms()))),
        }
    }
}

fn not_found(what: &str) -> Error {
    Error::new(ErrorCode::NotFound, format!("{what} not found"))
}

fn ok<'a, T: 'a>(value: Result<T>) -> MailFuture<'a, T> {
    Box::pin(async move { value })
}

/// Waits as a server would, so loading states show.
async fn latency(ms: u64) {
    timer(Duration::from_millis(ms)).await;
}

fn endpoint(host: &str, port: u16, security: Security) -> Endpoint {
    Endpoint {
        host: host.to_owned(),
        port,
        security,
    }
}

fn oauth(provider: OAuthProvider) -> Auth {
    Auth::OAuth { provider }
}

impl Backend for DemoBackend {
    fn start(&self, events: Events) -> MailFuture<'_, ()> {
        self.data.borrow_mut().events = Some(events);
        // The arrivals outlive this call: they stop when the backend goes.
        let data = Rc::downgrade(&self.data);
        Owner::detached().run(|| {
            spawn(async move {
                loop {
                    timer(ARRIVAL).await;
                    let Some(data) = data.upgrade() else { return };
                    let mut data = data.borrow_mut();
                    if let Some(summary) = data.deliver() {
                        let account_id = summary.account_id.clone();
                        data.emit(MailEvent::NewMail { summary });
                        data.emit(MailEvent::Changed {
                            account_id,
                            folder: FolderKind::Inbox,
                        });
                    }
                }
            })
        });
        ok(Ok(()))
    }

    fn sign_out(&self) -> MailFuture<'_, ()> {
        *self.data.borrow_mut() = Data::default();
        ok(Ok(()))
    }

    fn accounts(&self) -> MailFuture<'_, Vec<Account>> {
        ok(Ok(self.data.borrow().accounts.clone()))
    }

    fn detect(&self, email: String) -> MailFuture<'_, Option<ServerSettings>> {
        Box::pin(async move {
            latency(300).await;
            let email = email.trim().to_lowercase();
            let domain = email.rsplit_once('@').map(|(_, d)| d.to_owned());
            let Some(domain) = domain.filter(|d| d.contains('.')) else {
                return Err(invalid("enter an email address"));
            };
            Ok(preset(&domain).map(|s| ServerSettings {
                username: email,
                ..s
            }))
        })
    }

    fn add_imap(&self, input: ImapInput) -> MailFuture<'_, Account> {
        Box::pin(async move {
            if parse_address(&input.email).is_none() {
                return Err(invalid("enter an email address"));
            }
            if input.imap.host.trim().is_empty() || input.smtp.host.trim().is_empty() {
                return Err(invalid("enter the IMAP and SMTP servers"));
            }
            if input.imap.port == 0 || input.smtp.port == 0 {
                return Err(invalid("enter the IMAP and SMTP ports"));
            }
            if input.password.is_empty() {
                return Err(invalid("enter the app password"));
            }
            latency(700).await;
            let username = input
                .username
                .clone()
                .filter(|u| !u.trim().is_empty())
                .unwrap_or_else(|| input.email.clone());
            self.data.borrow_mut().add_account(Account {
                email: input.email,
                name: input.name.filter(|n| !n.trim().is_empty()),
                provider: input.provider,
                username,
                auth: Auth::Password,
                imap: input.imap,
                smtp: input.smtp,
                id: String::new(),
            })
        })
    }

    fn add_oauth(&self, provider: OAuthProvider) -> MailFuture<'_, Account> {
        Box::pin(async move {
            latency(900).await;
            let (email, id) = match provider {
                OAuthProvider::Google => ("demo.reader@gmail.com", "gmail"),
                OAuthProvider::Microsoft => ("demo.reader@outlook.com", "outlook"),
            };
            let settings = preset(email.rsplit_once('@').map_or("", |(_, d)| d))
                .ok_or_else(|| not_found("preset"))?;
            self.data.borrow_mut().add_account(Account {
                id: String::new(),
                email: email.to_owned(),
                name: Some("Demo Reader".to_owned()),
                provider: Some(id.to_owned()),
                username: email.to_owned(),
                auth: oauth(provider),
                imap: settings.imap,
                smtp: settings.smtp,
            })
        })
    }

    fn remove_account(&self, id: String) -> MailFuture<'_, ()> {
        let mut data = self.data.borrow_mut();
        let result = data.account(&id).map(|_| ());
        if result.is_ok() {
            data.accounts.retain(|a| a.id != id);
            data.messages.retain(|m| m.summary.account_id != id);
        }
        ok(result)
    }

    fn list(&self, query: ListQuery) -> MailFuture<'_, Vec<Summary>> {
        Box::pin(async move {
            latency(120).await;
            let data = self.data.borrow();
            let found = data.sorted(|m| {
                m.summary.folder == query.folder
                    && query
                        .account
                        .as_ref()
                        .is_none_or(|a| &m.summary.account_id == a)
            });
            let start = match &query.before {
                Some(before) => found
                    .iter()
                    .position(|m| &m.summary.id == before)
                    .map_or(found.len(), |at| at + 1),
                None => 0,
            };
            Ok(found
                .into_iter()
                .skip(start)
                .take(query.limit.max(1) as usize)
                .map(|m| m.summary.clone())
                .collect())
        })
    }

    fn get(&self, id: String) -> MailFuture<'_, Message> {
        Box::pin(async move {
            latency(80).await;
            let data = self.data.borrow();
            data.messages
                .iter()
                .find(|m| m.summary.id == id)
                .cloned()
                .ok_or_else(|| not_found("message"))
        })
    }

    fn set_flags(&self, id: String, flags: Flags) -> MailFuture<'_, ()> {
        let mut data = self.data.borrow_mut();
        let result = match data.messages.iter_mut().find(|m| m.summary.id == id) {
            Some(Message { summary: m, .. }) => {
                m.seen = flags.seen.unwrap_or(m.seen);
                m.flagged = flags.flagged.unwrap_or(m.flagged);
                Ok((m.account_id.clone(), m.folder))
            }
            None => Err(not_found("message")),
        };
        ok(result.map(|(account_id, folder)| {
            data.emit(MailEvent::Changed { account_id, folder });
        }))
    }

    fn send(&self, draft: Draft) -> MailFuture<'_, ()> {
        Box::pin(async move {
            let recipients: Vec<&Address> =
                draft.to.iter().chain(&draft.cc).chain(&draft.bcc).collect();
            if draft.to.is_empty() {
                return Err(invalid("add a recipient"));
            }
            if let Some(bad) = recipients
                .iter()
                .find(|r| parse_address(&r.email).is_none())
            {
                return Err(invalid(format!("{} is not an email address", bad.email)));
            }
            latency(600).await;
            let mut data = self.data.borrow_mut();
            let from = data.account(&draft.account_id)?.clone();
            let id = data.id();
            let sender = (
                from.name.as_deref().unwrap_or(&from.email),
                from.email.as_str(),
            );
            let mut sent = message(
                id,
                &from,
                FolderKind::Sent,
                unix_ms(),
                sender,
                &draft.subject,
                &draft.text,
            );
            sent.summary.to = draft.to.clone();
            sent.cc = draft.cc.clone();
            sent.summary.seen = true;
            sent.in_reply_to = draft.in_reply_to.clone();
            sent.references = draft.references.clone();
            data.messages.push(sent.clone());
            // Mail to one's own accounts arrives in their inboxes too.
            let own: Vec<Account> = data
                .accounts
                .iter()
                .filter(|a| {
                    recipients
                        .iter()
                        .any(|r| r.email.eq_ignore_ascii_case(&a.email))
                })
                .cloned()
                .collect();
            for account in own {
                let mut copy = sent.clone();
                copy.summary.id = data.id();
                copy.summary.account_id = account.id.clone();
                copy.summary.folder = FolderKind::Inbox;
                copy.summary.seen = false;
                let summary = copy.summary.clone();
                data.messages.push(copy);
                data.emit(MailEvent::NewMail { summary });
            }
            Ok(())
        })
    }

    fn search(&self, text: String, limit: u32) -> MailFuture<'_, Vec<Summary>> {
        Box::pin(async move {
            latency(150).await;
            let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
            if words.is_empty() {
                return Ok(Vec::new());
            }
            let data = self.data.borrow();
            let found = data.sorted(
                |Message {
                     summary: s, text, ..
                 }| {
                    let from = s
                        .from
                        .as_ref()
                        .map(|a| format!("{} {}", a.name.as_deref().unwrap_or(""), a.email))
                        .unwrap_or_default();
                    let to: Vec<&str> = s.to.iter().map(|a| a.email.as_str()).collect();
                    let hay = format!(
                        "{} {from} {} {}",
                        s.subject,
                        to.join(" "),
                        text.as_deref().unwrap_or("")
                    )
                    .to_lowercase();
                    words.iter().all(|w| hay.contains(w))
                },
            );
            Ok(found
                .into_iter()
                .take(limit.max(1) as usize)
                .map(|m| m.summary.clone())
                .collect())
        })
    }

    fn unread(&self) -> MailFuture<'_, HashMap<AccountId, u32>> {
        Box::pin(async move {
            let mut counts = HashMap::new();
            for m in &self.data.borrow().messages {
                if m.summary.folder == FolderKind::Inbox && !m.summary.seen {
                    *counts.entry(m.summary.account_id.clone()).or_default() += 1;
                }
            }
            Ok(counts)
        })
    }

    fn refresh(&self) -> MailFuture<'_, ()> {
        Box::pin(async move {
            let ids: Vec<String> = self
                .data
                .borrow()
                .accounts
                .iter()
                .map(|a| a.id.clone())
                .collect();
            let state = |account_id: &String, syncing| MailEvent::SyncState {
                account_id: account_id.clone(),
                syncing,
                error: None,
            };
            for id in &ids {
                self.data.borrow().emit(state(id, true));
            }
            latency(800).await;
            let mut data = self.data.borrow_mut();
            if let Some(summary) = data.deliver() {
                data.emit(MailEvent::NewMail { summary });
            }
            for id in ids {
                data.emit(state(&id, false));
                data.emit(MailEvent::Changed {
                    account_id: id,
                    folder: FolderKind::Inbox,
                });
            }
            Ok(())
        })
    }
}

fn parse_address(raw: &str) -> Option<Address> {
    let raw = raw.trim();
    let (name, email) = match (raw.rfind('<'), raw.ends_with('>')) {
        (Some(open), true) => (
            Some(raw[..open].trim().trim_matches('"').to_owned()),
            raw[open + 1..raw.len() - 1].trim(),
        ),
        _ => (None, raw),
    };
    let (local, domain) = email.split_once('@')?;
    if local.is_empty() || !domain.contains('.') || email.contains(char::is_whitespace) {
        return None;
    }
    Some(Address {
        name: name.filter(|n| !n.is_empty()),
        email: email.to_owned(),
    })
}

fn message(
    id: String,
    account: &Account,
    folder: FolderKind,
    date: f64,
    from: (&str, &str),
    subject: &str,
    text: &str,
) -> Message {
    let snippet: String = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(140)
        .collect();
    Message {
        message_id: Some(format!("<{id}@demo.nsh-mail>")),
        summary: Summary {
            id,
            account_id: account.id.clone(),
            folder,
            from: Some(Address {
                name: Some(from.0.to_owned()),
                email: from.1.to_owned(),
            }),
            to: vec![Address {
                name: account.name.clone(),
                email: account.email.clone(),
            }],
            subject: subject.to_owned(),
            snippet,
            date: date as i64,
            seen: false,
            flagged: false,
            has_attachments: false,
        },
        cc: Vec::new(),
        reply_to: Vec::new(),
        in_reply_to: None,
        references: Vec::new(),
        text: Some(text.to_owned()),
        html: None,
        remote_images: Vec::new(),
        attachments: Vec::new(),
    }
}

fn preset(domain: &str) -> Option<ServerSettings> {
    let tls = |id: &str, imap: &str, smtp: &str, name: &str, help: Option<&str>| ServerSettings {
        provider: Some(id.to_owned()),
        display_name: Some(name.to_owned()),
        username: String::new(),
        imap: endpoint(imap, 993, Security::Tls),
        smtp: endpoint(smtp, 465, Security::Tls),
        auth: Auth::Password,
        app_password: true,
        help_url: help.map(str::to_owned),
    };
    Some(match domain {
        "gmail.com" | "googlemail.com" => ServerSettings {
            auth: oauth(OAuthProvider::Google),
            app_password: false,
            ..tls("gmail", "imap.gmail.com", "smtp.gmail.com", "Gmail", None)
        },
        "outlook.com" | "hotmail.com" | "live.com" => ServerSettings {
            smtp: endpoint("smtp.office365.com", 587, Security::StartTls),
            auth: oauth(OAuthProvider::Microsoft),
            app_password: false,
            ..tls("outlook", "outlook.office365.com", "", "Outlook", None)
        },
        "qq.com" | "foxmail.com" => tls(
            "qq",
            "imap.qq.com",
            "smtp.qq.com",
            "QQ Mail",
            Some("https://service.mail.qq.com/detail/0/75"),
        ),
        "163.com" => tls(
            "163",
            "imap.163.com",
            "smtp.163.com",
            "NetEase 163",
            Some("https://help.mail.163.com/faq.do?m=list&categoryID=90"),
        ),
        "126.com" => tls(
            "126",
            "imap.126.com",
            "smtp.126.com",
            "NetEase 126",
            Some("https://help.mail.163.com/faq.do?m=list&categoryID=90"),
        ),
        "icloud.com" | "me.com" | "mac.com" => ServerSettings {
            smtp: endpoint("smtp.mail.me.com", 587, Security::StartTls),
            ..tls(
                "icloud",
                "imap.mail.me.com",
                "",
                "iCloud Mail",
                Some("https://support.apple.com/en-us/102654"),
            )
        },
        "fastmail.com" | "fastmail.fm" => tls(
            "fastmail",
            "imap.fastmail.com",
            "smtp.fastmail.com",
            "Fastmail",
            Some("https://www.fastmail.help/hc/en-us/articles/360058752854"),
        ),
        "yahoo.com" => tls(
            "yahoo",
            "imap.mail.yahoo.com",
            "smtp.mail.yahoo.com",
            "Yahoo Mail",
            Some("https://help.yahoo.com/kb/SLN15241.html"),
        ),
        _ => return None,
    })
}

/// What arrives on refresh and on the timer, in turn.
const ARRIVALS: &[((&str, &str), &str, &str)] = &[
    (
        ("Maya Patel", "maya@northwind.dev"),
        "Quick check on the release notes",
        "Hi Lin,\n\nCould you take one more look at the release notes before 4pm? I moved the migration section up \
         and trimmed the intro.\n\nThanks,\nMaya",
    ),
    (
        ("GitHub", "notifications@github.com"),
        "[markbang/nsh-mail] CI passed on main",
        "All checks have passed on main.\n\n5 successful checks: core, server, linux, windows, web.",
    ),
    (
        ("顺丰速运", "service@sf-express.com"),
        "您的快件已到达自提点",
        "您好，您的快件 SF1428 5521 0937 已到达【科技园北区丰巢柜】，取件码 6127，请于 48 小时内取件。",
    ),
    (
        ("Daniel Okafor", "daniel.okafor@gmail.com"),
        "Saturday hike?",
        "Weather looks great for Saturday. Thinking the coastal trail, start around 8. You in?\n\nD.",
    ),
];

struct Seed {
    account: usize,
    folder: FolderKind,
    ago: f64,
    from: (&'static str, &'static str),
    subject: &'static str,
    text: &'static str,
    html: Option<&'static str>,
    seen: bool,
    flagged: bool,
    attachments: &'static [(&'static str, &'static str, f64)],
    remote_images: &'static [&'static str],
}

const fn seed_msg(
    account: usize,
    ago: f64,
    from: (&'static str, &'static str),
    subject: &'static str,
    text: &'static str,
) -> Seed {
    Seed {
        account,
        folder: FolderKind::Inbox,
        ago,
        from,
        subject,
        text,
        html: None,
        seen: true,
        flagged: false,
        attachments: &[],
        remote_images: &[],
    }
}

const WORK: usize = 0;
const GMAIL: usize = 1;
const OUTLOOK: usize = 2;

const DESIGN_REVIEW_HTML: &str = r#"<h2>Design review: unified inbox</h2>
<p>Hi team,</p>
<p>Thanks for the comments on the <b>unified inbox</b> proposal. Summary of where we landed:</p>
<ul>
<li>Account colors stay as small marks next to the sender, never as row backgrounds.</li>
<li>Unread is shown by weight and a dot; flagged by an icon in its own lane.</li>
<li>Remote images are <i>blocked by default</i>; the reader offers to load them.</li>
</ul>
<p><img alt="Inbox mockup"></p>
<p>The full notes are in the <a href="https://github.com/markbang/nsh-mail/blob/main/docs/design.md">design doc</a>.</p>
<p>— Maya</p>"#;

const NEWSLETTER_HTML: &str = r#"<h1>This Week in Rust 571</h1>
<p>Hello and welcome to another issue of <i>This Week in Rust</i>!</p>
<h3>Updates from the Rust project</h3>
<ul>
<li><a href="https://blog.rust-lang.org/">Announcing Rust 1.91.0</a></li>
<li>Async closures are now stable in more positions</li>
</ul>
<h3>Crate of the week</h3>
<p>This week's crate is <code>ammonia</code>, a whitelist-based HTML sanitizer.</p>
<blockquote>Sanitize first, render second.</blockquote>
<p><img alt=""></p>"#;

const RECEIPT_HTML: &str = r#"<h2>Your receipt from Fastmail</h2>
<table>
<tr><th>Item</th><th>Amount</th></tr>
<tr><td>Standard plan, 12 months</td><td>$60.00</td></tr>
<tr><td>Tax</td><td>$0.00</td></tr>
<tr><td><b>Total</b></td><td><b>$60.00</b></td></tr>
</table>
<p>Paid with Visa ending 4242 on 3 November.</p>"#;

const SEEDS: &[Seed] = &[
    Seed {
        seen: false,
        flagged: true,
        html: Some(DESIGN_REVIEW_HTML),
        remote_images: &["https://images.example.com/mockup-inbox.png"],
        ..seed_msg(
            WORK,
            12.0 * MINUTE,
            ("Maya Patel", "maya@northwind.dev"),
            "Design review notes: unified inbox",
            "Hi team,\n\nThanks for the comments on the unified inbox proposal. Summary of where we landed:\n\n- Account colors stay as small marks next to the sender, never as row backgrounds.\n- Unread is shown by weight and a dot; flagged by an icon in its own lane.\n- Remote images are blocked by default; the reader offers to load them.\n\nThe full notes are in the design doc.\n\n— Maya",
        )
    },
    Seed {
        seen: false,
        ..seed_msg(
            GMAIL,
            38.0 * MINUTE,
            ("Daniel Okafor", "daniel.okafor@gmail.com"),
            "Photos from the weekend",
            "Finally sorted the photos from Saturday. The one at the lighthouse came out great.\n\nI'll send the full album link tonight.",
        )
    },
    Seed {
        seen: false,
        attachments: &[("Q4-roadmap.pdf", "application/pdf", 482_113.0)],
        ..seed_msg(
            WORK,
            1.6 * HOUR,
            ("Jonas Weber", "jonas@northwind.dev"),
            "Q4 roadmap draft",
            "Attached is the first draft of the Q4 roadmap. The big items are offline search, push on Android and the server beta.\n\nComments by Friday please.\n\nJonas",
        )
    },
    Seed {
        seen: false,
        ..seed_msg(
            OUTLOOK,
            2.5 * HOUR,
            ("王芳", "wangfang@outlook.com"),
            "周五聚餐改到七点",
            "大家好，\n\n周五的聚餐改到晚上七点，地点不变，还是园区西门那家。到时候见！\n\n王芳",
        )
    },
    Seed {
        html: Some(NEWSLETTER_HTML),
        remote_images: &["https://this-week-in-rust.org/tracking-pixel.gif"],
        ..seed_msg(
            GMAIL,
            5.0 * HOUR,
            ("This Week in Rust", "newsletter@this-week-in-rust.org"),
            "This Week in Rust 571",
            "Hello and welcome to another issue of This Week in Rust!\n\nUpdates from the Rust project\n- Announcing Rust 1.91.0\n- Async closures are now stable in more positions\n\nCrate of the week: ammonia, a whitelist-based HTML sanitizer.",
        )
    },
    Seed {
        seen: false,
        ..seed_msg(
            WORK,
            7.0 * HOUR,
            ("GitHub", "notifications@github.com"),
            "[markbang/nsh-mail] Review requested: feat(app): client UI",
            "@markbang requested your review on #3: feat(app): client UI.\n\n+2,140 −12 across 41 files.",
        )
    },
    Seed {
        flagged: true,
        ..seed_msg(
            OUTLOOK,
            22.0 * HOUR,
            ("China Merchants Bank", "creditcard@cmbchina.com"),
            "信用卡电子账单（11月）",
            "尊敬的陈琳女士：\n\n您 11 月的信用卡账单已出，本期应还金额 ¥3,284.50，最后还款日 12 月 5 日。",
        )
    },
    Seed {
        html: Some(RECEIPT_HTML),
        ..seed_msg(
            WORK,
            1.1 * DAY,
            ("Fastmail", "billing@fastmail.com"),
            "Your receipt from Fastmail",
            "Your receipt from Fastmail\n\nStandard plan, 12 months: $60.00\nTax: $0.00\nTotal: $60.00\n\nPaid with Visa ending 4242 on 3 November.",
        )
    },
    seed_msg(
        GMAIL,
        1.4 * DAY,
        ("Mom", "chen.meiling@163.com"),
        "Re: Thanksgiving plans",
        "We'd love to have you both. Dad is already planning the menu. Let me know your train times when you book.\n\nLove, Mom",
    ),
    seed_msg(
        WORK,
        1.8 * DAY,
        ("Priya Raman", "priya@northwind.dev"),
        "Retro moved to Thursday",
        "Heads up: the sprint retro moves to Thursday 3pm because of the all-hands. Same room.",
    ),
    Seed {
        attachments: &[(
            "boarding-pass.pkpass",
            "application/vnd.apple.pkpass",
            61_440.0,
        )],
        ..seed_msg(
            GMAIL,
            2.2 * DAY,
            ("United Airlines", "receipts@united.com"),
            "Your trip to Chicago: check in now",
            "Your flight UA 1872 from SFO to ORD departs Thursday at 07:15. Check in now to get your boarding pass.",
        )
    },
    seed_msg(
        OUTLOOK,
        2.9 * DAY,
        ("LinkedIn", "messages-noreply@linkedin.com"),
        "You appeared in 14 searches this week",
        "See who's looking at your profile and what they searched for.",
    ),
    seed_msg(
        WORK,
        3.4 * DAY,
        ("Sentry", "noreply@sentry.io"),
        "New issue: TypeError in reader.tsx",
        "TypeError: Cannot read properties of undefined (reading 'html')\n\nreader.tsx in renderBody at line 88\n\n12 events, 3 users",
    ),
    seed_msg(
        GMAIL,
        4.1 * DAY,
        ("Daniel Okafor", "daniel.okafor@gmail.com"),
        "Book recommendation",
        "You have to read 'The Overstory'. Took me a month but worth every page.",
    ),
    seed_msg(
        OUTLOOK,
        5.0 * DAY,
        (
            "Microsoft account team",
            "account-security-noreply@accountprotection.microsoft.com",
        ),
        "New sign-in to your account",
        "We noticed a new sign-in to your Microsoft account from Linux, Chrome.\n\nIf this was you, you can ignore this message.",
    ),
    seed_msg(
        WORK,
        6.2 * DAY,
        ("Maya Patel", "maya@northwind.dev"),
        "Offsite agenda",
        "Day 1: roadmap and planning.\nDay 2: hack day, demos at 4pm.\n\nBring a laptop charger, the venue has few outlets.",
    ),
    seed_msg(
        GMAIL,
        7.5 * DAY,
        ("Spotify", "no-reply@spotify.com"),
        "Your weekly mixtape is ready",
        "Discover Weekly: 30 new songs picked for you.",
    ),
    seed_msg(
        OUTLOOK,
        9.0 * DAY,
        ("知乎", "noreply@zhihu.com"),
        "你关注的问题有了新回答",
        "「Rust 适合写桌面应用吗？」有 3 个新回答。",
    ),
    // Older mail, so the list has more than a page.
    seed_msg(
        WORK,
        10.0 * DAY,
        ("Jonas Weber", "jonas@northwind.dev"),
        "Weekly sync notes",
        "Notes from this week's sync are in the shared folder.",
    ),
    seed_msg(
        GMAIL,
        11.0 * DAY,
        ("Airbnb", "automated@airbnb.com"),
        "Reminder: leave a review",
        "How was your stay in Lisbon? Your review helps other travellers.",
    ),
    seed_msg(
        WORK,
        12.0 * DAY,
        ("GitHub", "notifications@github.com"),
        "[markbang/nsh-mail] Issue #1 opened: design doc",
        "markbang opened issue #1: design doc.",
    ),
    seed_msg(
        OUTLOOK,
        13.0 * DAY,
        ("王芳", "wangfang@outlook.com"),
        "下周的项目会议",
        "下周二上午十点开项目会议，请提前看一下需求文档。",
    ),
    seed_msg(
        GMAIL,
        14.0 * DAY,
        ("Mom", "chen.meiling@163.com"),
        "Recipe you asked for",
        "Here's grandma's dumpling recipe. The secret is the ginger water.",
    ),
    seed_msg(
        WORK,
        15.0 * DAY,
        ("Priya Raman", "priya@northwind.dev"),
        "Welcome to the team!",
        "Welcome aboard, Lin! Your laptop arrives Monday. Let me know if you need anything.",
    ),
    seed_msg(
        GMAIL,
        16.0 * DAY,
        ("Google", "no-reply@accounts.google.com"),
        "Security alert",
        "A new app password was created for your Google Account.",
    ),
    seed_msg(
        OUTLOOK,
        17.0 * DAY,
        ("Booking.com", "noreply@booking.com"),
        "Confirmation: Hotel Lux Lisboa",
        "Your booking is confirmed: 3 nights, 12–15 October.",
    ),
    seed_msg(
        WORK,
        18.0 * DAY,
        ("Sentry", "noreply@sentry.io"),
        "Weekly report",
        "Errors this week: 41 (−23%). Top issue: timeout in sync worker.",
    ),
    seed_msg(
        GMAIL,
        19.0 * DAY,
        ("Daniel Okafor", "daniel.okafor@gmail.com"),
        "Climbing gym",
        "New routes went up at the gym. Tuesday?",
    ),
    seed_msg(
        OUTLOOK,
        20.0 * DAY,
        ("Apple", "no_reply@email.apple.com"),
        "Your receipt from Apple",
        "iCloud+ 50 GB: ¥6.00",
    ),
    seed_msg(
        WORK,
        21.0 * DAY,
        ("Jonas Weber", "jonas@northwind.dev"),
        "Interview loop",
        "Can you join the interview loop on Wednesday for the backend role?",
    ),
    seed_msg(
        GMAIL,
        22.0 * DAY,
        ("Strava", "no-reply@strava.com"),
        "Your October recap",
        "You ran 84 km in October. Nice work!",
    ),
    seed_msg(
        OUTLOOK,
        23.0 * DAY,
        ("王芳", "wangfang@outlook.com"),
        "照片",
        "上次团建的照片我放到群相册了。",
    ),
    seed_msg(
        WORK,
        24.0 * DAY,
        ("Maya Patel", "maya@northwind.dev"),
        "Docs style guide",
        "Sharing the docs style guide we agreed on. Sentence case everywhere, please.",
    ),
    seed_msg(
        GMAIL,
        25.0 * DAY,
        ("Kickstarter", "no-reply@kickstarter.com"),
        "A project you backed has an update",
        "Update #12: Shipping starts next month.",
    ),
    seed_msg(
        WORK,
        26.0 * DAY,
        ("Priya Raman", "priya@northwind.dev"),
        "Laptop setup",
        "Your laptop is ready for pickup at the front desk.",
    ),
    // Other folders.
    Seed {
        folder: FolderKind::Sent,
        ..seed_msg(
            WORK,
            3.0 * HOUR,
            ("Lin Chen", "lin.chen@northwind.dev"),
            "Re: Q4 roadmap draft",
            "Looks good overall. I'd move offline search before the server beta: it helps both modes.\n\nLin",
        )
    },
    Seed {
        folder: FolderKind::Sent,
        ..seed_msg(
            GMAIL,
            1.5 * DAY,
            ("Lin Chen", "linchen.mail@gmail.com"),
            "Re: Thanksgiving plans",
            "We'll take the 10:05 train and arrive around 2. Can't wait!",
        )
    },
    Seed {
        folder: FolderKind::Drafts,
        ..seed_msg(
            WORK,
            4.0 * HOUR,
            ("Lin Chen", "lin.chen@northwind.dev"),
            "Notes for the offsite",
            "Ideas for the hack day:\n- Offline search\n- Keyboard shortcuts help",
        )
    },
    Seed {
        folder: FolderKind::Archive,
        ..seed_msg(
            WORK,
            30.0 * DAY,
            ("HR", "hr@northwind.dev"),
            "Your benefits enrollment",
            "Your benefits enrollment is complete.",
        )
    },
    Seed {
        folder: FolderKind::Archive,
        ..seed_msg(
            GMAIL,
            40.0 * DAY,
            ("Daniel Okafor", "daniel.okafor@gmail.com"),
            "Trip photos",
            "Here are the photos from Kyoto.",
        )
    },
    Seed {
        folder: FolderKind::Trash,
        ..seed_msg(
            OUTLOOK,
            6.0 * DAY,
            ("Promo", "deals@shop.example"),
            "48 hours only: 60% off",
            "Our biggest sale of the year ends soon.",
        )
    },
];

fn seed(now: f64) -> Data {
    let account = |id: &str,
                   email: &str,
                   name: &str,
                   provider: &str,
                   auth: Auth,
                   imap: &str,
                   smtp: (&str, u16, Security)| Account {
        id: id.into(),
        email: email.into(),
        name: Some(name.into()),
        provider: Some(provider.into()),
        username: email.into(),
        auth,
        imap: endpoint(imap, 993, Security::Tls),
        smtp: endpoint(smtp.0, smtp.1, smtp.2),
    };
    let accounts = vec![
        account(
            "a-work",
            "lin.chen@northwind.dev",
            "Lin Chen",
            "fastmail",
            Auth::Password,
            "imap.fastmail.com",
            ("smtp.fastmail.com", 465, Security::Tls),
        ),
        account(
            "a-gmail",
            "linchen.mail@gmail.com",
            "Lin Chen",
            "gmail",
            oauth(OAuthProvider::Google),
            "imap.gmail.com",
            ("smtp.gmail.com", 465, Security::Tls),
        ),
        account(
            "a-outlook",
            "chenlin@outlook.com",
            "陈琳",
            "outlook",
            oauth(OAuthProvider::Microsoft),
            "outlook.office365.com",
            ("smtp.office365.com", 587, Security::StartTls),
        ),
    ];
    let mut data = Data {
        accounts,
        ..Data::default()
    };
    for s in SEEDS {
        let account = data.accounts[s.account].clone();
        let id = data.id();
        let mut m = message(
            id,
            &account,
            s.folder,
            now - s.ago,
            s.from,
            s.subject,
            s.text,
        );
        if matches!(s.folder, FolderKind::Sent | FolderKind::Drafts) {
            m.summary.to = vec![Address {
                name: None,
                email: "team@northwind.dev".into(),
            }];
        }
        m.html = s.html.map(str::to_owned);
        m.remote_images = s.remote_images.iter().map(|u| (*u).to_owned()).collect();
        m.summary.seen = s.seen;
        m.summary.flagged = s.flagged;
        m.attachments = s
            .attachments
            .iter()
            .enumerate()
            .map(|(index, (name, mime, size))| Attachment {
                index: index as u32,
                filename: Some((*name).into()),
                mime_type: (*mime).into(),
                size: *size as u64,
                inline: false,
                content_id: None,
            })
            .collect();
        m.summary.has_attachments = !m.attachments.is_empty();
        data.messages.push(m);
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_parse_with_and_without_names() {
        assert_eq!(
            parse_address("Maya <maya@northwind.dev>")
                .unwrap()
                .name
                .as_deref(),
            Some("Maya")
        );
        assert_eq!(parse_address(" a@b.co ").unwrap().email, "a@b.co");
        assert!(parse_address("not an address").is_none());
        assert!(parse_address("a@localhost").is_none());
    }
}
