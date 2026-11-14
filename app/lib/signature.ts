// Signatures: plain text with **bold**, *italic* and [links](https://…). The
// composer shows the plain form under the "-- " delimiter (RFC 3676), editable
// like the rest of the body; sending turns the body into an HTML part as well,
// with the signature's formatting, as long as its block is still as added.
import type { Account } from "nsh-mail/native";
import type { ComposeInit, ComposeMode } from "./format";

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
const SAFE_URL = /^(https?:\/\/|mailto:)\S+$/i;
const BARE_URL = /https?:\/\/[^\s<>"]*[^\s<>".,;:!?)\]'*]/g;
const INLINE = /\[([^\]\n]+)\]\(([^)\s]+)\)|\*\*([^*\n]+)\*\*|\*([^*\n]+)\*|https?:\/\/[^\s<>"]*[^\s<>".,;:!?)\]'*]/g;

const linkify = (escaped: string) => escaped.replace(BARE_URL, (url) => `<a href="${url}">${url}</a>`);

/** The signature's plain text and its HTML (inline, lines broken with `<br>`). */
export function renderSignature(source: string): { text: string; html: string } {
  const src = source.replace(/\r\n?/g, "\n").trim();
  let text = "";
  let html = "";
  let last = 0;
  for (const m of src.matchAll(INLINE)) {
    const before = src.slice(last, m.index);
    text += before;
    html += esc(before);
    last = m.index! + m[0].length;
    const [whole, label, url, bold, italic] = m;
    if (label !== undefined) {
      if (SAFE_URL.test(url)) {
        const shown = url.replace(/^mailto:/i, "");
        text += label === url || label === shown ? shown : `${label} <${shown}>`;
        html += `<a href="${esc(url)}">${esc(label)}</a>`;
      } else {
        text += whole;
        html += esc(whole);
      }
    } else if (bold !== undefined) {
      text += bold;
      html += `<b>${esc(bold)}</b>`;
    } else if (italic !== undefined) {
      text += italic;
      html += `<i>${esc(italic)}</i>`;
    } else {
      text += whole;
      html += `<a href="${esc(whole)}">${esc(whole)}</a>`;
    }
  }
  text += src.slice(last);
  html += esc(src.slice(last));
  return { text, html: html.replace(/\n/g, "<br>") };
}

/** Which of the account's choices a composer follows: an outbox message edited is a new message or a reply. */
export const signatureMode = (init: ComposeInit): ComposeMode =>
  init.mode === "edit" ? (init.outgoing?.draft.inReplyTo ? "reply" : "new") : init.mode;

/** The block the composer adds for `account` in `mode`: "-- " and the signature's text; "" for none. */
export function signatureBlock(account: Account | undefined, mode: ComposeMode): string {
  const s = account?.signature;
  if (!s?.text.trim() || !(mode === "new" ? s.newMessages : s.replies)) return "";
  return `-- \n${renderSignature(s.text).text}`;
}

/** A new composer's body: the signature above the quote or forwarded message (as Gmail places it). */
export const withSignature = (start: string, block: string) => (block ? `\n\n${block}${start}` : start);

/** Where `block` sits in `text` as it was added (`\n\n` before it, a line end or the end after), or -1. */
function findBlock(text: string, block: string) {
  const needle = `\n\n${block}`;
  for (let at = text.indexOf(needle); at >= 0; at = text.indexOf(needle, at + 1)) {
    const end = at + needle.length;
    if (end === text.length || text[end] === "\n") return at;
  }
  return -1;
}

/** The body after the From account changed: the old account's signature, if left as added,
 * gives way to the new one's. A signature the user edited stays. */
export function swapSignature(text: string, start: string, from: Account | undefined, to: Account | undefined, mode: ComposeMode) {
  const old = signatureBlock(from, mode);
  const next = signatureBlock(to, mode);
  if (old === next) return text;
  const insert = next ? `\n\n${next}` : "";
  if (old) {
    const at = findBlock(text, old);
    return at < 0 ? text : text.slice(0, at) + insert + text.slice(at + 2 + old.length);
  }
  const at = start && text.endsWith(start) ? text.length - start.length : text.length;
  return text.slice(0, at) + insert + text.slice(at);
}

/** Plain text as HTML: lines kept, `>` quotes as blockquotes, links linked. */
function textHtml(text: string): string {
  const lines = text.split("\n");
  const out: string[] = [];
  for (let i = 0; i < lines.length; ) {
    const quoted = lines[i].startsWith(">");
    const group: string[] = [];
    while (i < lines.length && lines[i].startsWith(">") === quoted) group.push(lines[i++]);
    out.push(
      quoted
        ? `<blockquote style="margin:0 0 0 .8ex;border-left:1px solid #ccc;padding-left:1ex">${textHtml(group.map((l) => l.replace(/^> ?/, "")).join("\n"))}</blockquote>`
        : group.map((l) => linkify(esc(l))).join("<br>"),
    );
  }
  return out.join("");
}

/** The HTML part of a body holding `account`'s signature as added, or null: then the message goes as plain text. */
export function bodyHtml(text: string, account: Account | undefined, mode: ComposeMode): string | null {
  const block = signatureBlock(account, mode);
  const at = block ? findBlock(text, block) : -1;
  if (at < 0) return null;
  const signature = `<div class="signature">-- <br>${renderSignature(account!.signature.text).html}</div>`;
  const before = text.slice(0, at);
  const after = text.slice(at + 2 + block.length);
  return `<div>${textHtml(before)}${before ? "<br><br>" : ""}${signature}${textHtml(after)}</div>`;
}
