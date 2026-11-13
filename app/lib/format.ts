// Formatting for people: dates, names and addresses, reply quoting.
import { locale } from "nsh";
import { t } from "nsh/i18n";
import type { Address, Message, Outgoing } from "nsh-mail/native";

const DAY = 86_400_000;

/** A list's date: the time today, the weekday this week, else the date. */
export function listDate(ms: number, now = Date.now()): string {
  const d = new Date(ms);
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  if (ms >= today.getTime()) return new Intl.DateTimeFormat(locale(), { hour: "numeric", minute: "2-digit" }).format(d);
  if (ms >= today.getTime() - DAY) return t("date.yesterday");
  if (ms >= today.getTime() - 6 * DAY) return new Intl.DateTimeFormat(locale(), { weekday: "short" }).format(d);
  const sameYear = d.getFullYear() === today.getFullYear();
  return new Intl.DateTimeFormat(locale(), sameYear ? { month: "short", day: "numeric" } : { year: "numeric", month: "short", day: "numeric" }).format(d);
}

/** When a message will go: the weekday, date and time ("Mon, Nov 16, 8:00 AM"). */
export function sendDate(ms: number): string {
  return new Intl.DateTimeFormat(locale(), { weekday: "short", month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }).format(new Date(ms));
}

/** A reader's date: day and time in full. */
export function fullDate(ms: number): string {
  return new Intl.DateTimeFormat(locale(), { dateStyle: "medium", timeStyle: "short" }).format(new Date(ms));
}

const knownProviders = new Set(["gmail", "outlook", "icloud", "yahoo", "fastmail", "qq", "163", "126"]);

/** A mail provider's name in the UI's language ("QQ Mail", "QQ 邮箱"); `fallback` for presets without a translation. */
export function providerName(id: string | null | undefined, fallback: string): string {
  return id && knownProviders.has(id) ? t(`provider.${id}`) : fallback;
}

export function displayName(a: Address | null | undefined): string {
  return a ? a.name?.trim() || a.email : t("reader.unknownSender");
}

/** How a list names who wrote in a conversation: "Maya, me, Jonas"; the first and the last two when more
 * than three ("Priya … Jonas, Maya"); a lone sender in full. `own` holds the user's addresses, in lower case. */
export function participantNames(people: Address[], own: Set<string>): string {
  const name = (a: Address) => {
    if (own.has(a.email.toLowerCase())) return t("list.me");
    const full = displayName(a);
    if (people.length === 1) return full;
    return full.includes("@") ? full.split("@")[0] : full.split(/\s+/)[0];
  };
  if (people.length <= 3) return people.map(name).join(", ");
  return `${name(people[0])} … ${people.slice(-2).map(name).join(", ")}`;
}

/** "Name <email>", as a recipient field takes it. */
export function recipient(a: Address): string {
  return a.name ? `${a.name} <${a.email}>` : a.email;
}

export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Splits a recipient field on commas and semicolons. */
export function splitRecipients(text: string): string[] {
  return text
    .split(/[,;\n]/)
    .map((r) => r.trim())
    .filter(Boolean);
}

/** "Name <a@b.c>" or "a@b.c" as an address. */
export function parseRecipient(raw: string): Address {
  const m = raw.match(/^\s*"?([^"<]*?)"?\s*<([^>]+)>\s*$/);
  return m ? { name: m[1].trim() || null, email: m[2].trim() } : { name: null, email: raw.trim() };
}

/** A plausible address, as the composer checks it before sending. */
export function isAddress(raw: string): boolean {
  const m = raw.match(/<([^>]+)>\s*$/);
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test((m ? m[1] : raw).trim());
}

export type ComposeMode = "new" | "reply" | "replyAll" | "forward" | "edit";

export interface ComposeInit {
  mode: ComposeMode;
  /** The message replied to or forwarded. */
  source?: Message;
  /** "edit": a message taken out of the outbox to change it. */
  outgoing?: Outgoing;
}

export interface DraftFields {
  accountId: string | null;
  to: string;
  cc: string;
  subject: string;
  text: string;
  inReplyTo: string | null;
  references: string[];
}

const prefixed = (prefix: string, subject: string) =>
  new RegExp(`^${prefix}:`, "i").test(subject.trim()) ? subject.trim() : `${prefix}: ${subject.trim()}`;

/** The composer's fields for a new message, a reply or a forward. */
export function draftFor(init: ComposeInit, ownEmails: string[]): DraftFields {
  const o = init.outgoing?.draft;
  if (init.mode === "edit" && o) {
    return {
      accountId: o.accountId,
      to: o.to.map(recipient).join(", "),
      cc: (o.cc ?? []).map(recipient).join(", "),
      subject: o.subject,
      text: o.text,
      inReplyTo: o.inReplyTo ?? null,
      references: o.references ?? [],
    };
  }
  const m = init.source;
  if (!m || init.mode === "new") {
    return { accountId: null, to: "", cc: "", subject: "", text: "", inReplyTo: null, references: [] };
  }
  const header = t("compose.quoteHeader", { date: fullDate(m.date), name: displayName(m.from) });
  const body = m.text ?? "";
  const quoted = body
    .trimEnd()
    .split("\n")
    .map((line) => (line ? `> ${line}` : ">"))
    .join("\n");
  if (init.mode === "forward") {
    const forwarded = [
      t("compose.forwardHeader"),
      `${t("reader.from")}: ${m.from ? recipient(m.from) : ""}`,
      `${t("reader.date")}: ${fullDate(m.date)}`,
      `${t("compose.subject")}: ${m.subject}`,
      `${t("reader.to")}: ${m.to.map(recipient).join(", ")}`,
      "",
      body,
    ].join("\n");
    return {
      accountId: m.accountId,
      to: "",
      cc: "",
      subject: prefixed("Fwd", m.subject),
      text: `\n\n${forwarded}`,
      inReplyTo: null,
      references: [],
    };
  }
  const own = new Set(ownEmails.map((e) => e.toLowerCase()));
  const replyTo = m.replyTo[0] ?? m.from ?? { email: "" };
  const others =
    init.mode === "replyAll"
      ? [...m.to, ...m.cc].filter((a) => !own.has(a.email.toLowerCase()) && a.email.toLowerCase() !== replyTo.email.toLowerCase())
      : [];
  return {
    accountId: m.accountId,
    to: recipient(replyTo),
    cc: others.map(recipient).join(", "),
    subject: prefixed("Re", m.subject),
    // The reply goes above a blank line; the quote ends its last line.
    text: `\n\n${header}\n${quoted}\n`,
    inReplyTo: m.messageId ?? null,
    references: m.messageId ? [...m.references, m.messageId] : m.references,
  };
}
