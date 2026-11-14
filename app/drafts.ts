// Drafts being written, one per context: a new message, or a reply, reply-all
// or forward of one message. A draft outlives its composer (Escape, the
// backdrop, back) and, kept in the device's Storage, a restart; sending or
// discarding it ends it; sending with Undo holds it until the Undo has passed.
// Drafts are not synced to the mail server's Drafts folder. Staged uploads live
// in memory only, so a restart keeps a forward's attachments but not the files
// the user added. A message taken out of the outbox to edit ("edit:") is kept
// in memory only: its composer puts it back in the outbox when it closes.
import { cleanup, effect, signal, Storage, untracked, type Signal } from "nsh";
import type { Account, DraftAttachment } from "nsh-mail/native";
import { createAttachments, type Attachments } from "./lib/attachments";
import { draftFor, type ComposeInit, type DraftFields } from "./lib/format";
import { signatureBlock, withSignature } from "./lib/signature";
import { errorText, showToast } from "./store";

const KEY = "nshmail.drafts";
// The most drafts kept on the device; the oldest go first.
const KEEP = 50;
const SAVE_MS = 500;

interface Saved {
  from: string | null;
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  text: string;
  /** The forwarded attachments left in it; null while the user has not changed the files. */
  files: { name: string; size: number; ref: DraftAttachment }[] | null;
  at: number;
}

export interface Draft {
  /** The context's fields; `text` is without the signature (the quote, the forwarded message). */
  start: DraftFields;
  from: Signal<string | null>;
  to: Signal<string>;
  cc: Signal<string>;
  bcc: Signal<string>;
  subject: Signal<string>;
  text: Signal<string>;
  files: Attachments;
  /** Differs from what a new composer shows: worth keeping. */
  edited: () => boolean;
}

let saved: Record<string, Saved> = {};
const live = new Map<string, Draft>();
const keys = new WeakMap<Draft, string>();
const ended = new WeakSet<Draft>();

const keyOf = (init: ComposeInit) =>
  init.mode === "edit" && init.outgoing ? `edit:${init.outgoing.id}` : init.mode === "new" || !init.source ? "new" : `${init.mode}:${init.source.id}`;
const kept = (key: string) => !key.startsWith("edit:");

/** Reads the kept drafts; called once at start. */
export async function loadDrafts() {
  try {
    saved = JSON.parse((await Storage.getItem(KEY)) ?? "{}") ?? {};
  } catch {
    saved = {};
  }
}

let timer: ReturnType<typeof setTimeout> | undefined;
function saveNow() {
  clearTimeout(timer);
  timer = undefined;
  const newest = Object.entries(saved)
    .sort((a, b) => b[1].at - a[1].at)
    .slice(0, KEEP);
  saved = Object.fromEntries(newest);
  Storage.setItem(KEY, JSON.stringify(saved)).catch((e) => showToast(errorText(e), "error"));
}
const saveSoon = () => {
  clearTimeout(timer);
  timer = setTimeout(saveNow, SAVE_MS);
};

function snapshot(d: Draft): Saved | null {
  if (!d.edited()) return null;
  const files = d.files.edited()
    ? d.files.items().flatMap((a) => (a.ref?.kind === "forward" ? [{ name: a.name, size: a.size, ref: a.ref }] : []))
    : null;
  return { from: d.from(), to: d.to(), cc: d.cc(), bcc: d.bcc(), subject: d.subject(), text: d.text(), files, at: Date.now() };
}

function open(init: ComposeInit, accounts: Account[], defaultFrom: string | null): Draft {
  const key = keyOf(init);
  const kept = live.get(key);
  if (kept) return kept;
  const start = draftFor(init, accounts.map((a) => a.email));
  const from = start.accountId ?? defaultFrom;
  const fresh = init.mode === "edit" ? start.text : withSignature(start.text, signatureBlock(accounts.find((a) => a.id === from), init.mode));
  const s: Saved | undefined = saved[key];
  const outgoing = init.mode === "edit" ? init.outgoing : undefined;
  const files = outgoing
    ? createAttachments(outgoing.files.map((f) => ({ name: f.filename, size: f.size, sent: f.size, ref: { kind: "upload" as const, id: f.id } })))
    : s?.files
      ? createAttachments(s.files.map((f) => ({ ...f, sent: f.size })), true)
      : createAttachments();
  if (!s?.files && init.mode === "forward" && init.source) files.forward(init.source);
  const d: Draft = {
    start,
    from: signal(s && accounts.some((a) => a.id === s.from) ? s.from : from),
    to: signal(s?.to ?? start.to),
    cc: signal(s?.cc ?? start.cc),
    bcc: signal(s?.bcc ?? ""),
    subject: signal(s?.subject ?? start.subject),
    text: signal(s?.text ?? fresh),
    files,
    edited: () =>
      files.edited() ||
      d.from() !== from ||
      d.to() !== start.to ||
      d.cc() !== start.cc ||
      d.bcc() !== "" ||
      d.subject() !== start.subject ||
      d.text() !== fresh,
  };
  keys.set(d, key);
  live.set(key, d);
  return d;
}

/** The composer's draft for `init`: the one kept for that context, or a new one. Kept while edited. */
export function useDraft(init: ComposeInit, accounts: Account[], defaultFrom: string | null): Draft {
  const d = open(init, accounts, defaultFrom);
  const key = keys.get(d)!;
  effect(() => {
    const snap = snapshot(d);
    untracked(() => {
      if (ended.has(d) || !kept(key) || (!snap && !(key in saved))) return;
      if (snap) {
        saved[key] = snap;
        live.set(key, d);
      } else delete saved[key];
      saveSoon();
    });
  });
  cleanup(() => {
    if (live.get(key) === d && !d.edited()) live.delete(key);
    if (timer !== undefined) saveNow();
  });
  return d;
}

/** The draft went out, or the user discarded it: it is gone from here and the device. */
export function endDraft(d: Draft, how: "sent" | "discarded") {
  const key = keys.get(d)!;
  ended.add(d);
  if (how === "sent") d.files.sent();
  else d.files.drop();
  if (live.get(key) === d) live.delete(key);
  delete saved[key];
  saveNow();
}

/** Sent or discarded. */
export const isEnded = (d: Draft) => ended.has(d);

/** The draft is in the outbox with an Undo still open: gone from the device, kept here until
 * `release` (the Undo passed) or `restore` (the Undo was pressed: it is the context's draft again). */
export function holdDraft(d: Draft) {
  const key = keys.get(d)!;
  ended.add(d);
  if (live.get(key) === d) live.delete(key);
  delete saved[key];
  saveNow();
  return {
    restore() {
      ended.delete(d);
      live.set(key, d);
      const snap = snapshot(d);
      if (snap && kept(key)) {
        saved[key] = snap;
        saveNow();
      }
    },
    release() {
      d.files.sent();
    },
  };
}

/** Another session (signed out, a new mode): its drafts are not this one's. */
export async function clearDrafts() {
  clearTimeout(timer);
  timer = undefined;
  for (const d of live.values()) d.files.drop();
  live.clear();
  saved = {};
  await Storage.removeItem(KEY);
}
