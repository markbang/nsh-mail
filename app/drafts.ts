// Drafts being written, one per context: a new message, or a reply, reply-all
// or forward of one message. A draft outlives its composer (Escape, the
// backdrop, back) and, kept in the device's Storage, a restart; sending or
// discarding it ends it. Drafts are not synced to the mail server's Drafts
// folder. Staged uploads live in memory only, so a restart keeps a forward's
// attachments but not the files the user added.
import { cleanup, effect, signal, Storage, untracked, type Signal } from "nsh";
import type { Account, DraftAttachment } from "nsh-mail/native";
import { createAttachments, type Attachments } from "./lib/attachments";
import { draftFor, type ComposeInit, type DraftFields } from "./lib/format";
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
  /** What a new composer for this context shows. */
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

const keyOf = (init: ComposeInit) => (init.mode === "new" || !init.source ? "new" : `${init.mode}:${init.source.id}`);

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
  const s: Saved | undefined = saved[key];
  const files = s?.files ? createAttachments(s.files.map((f) => ({ ...f, sent: f.size })), true) : createAttachments();
  if (!s?.files && init.mode === "forward" && init.source) files.forward(init.source);
  const d: Draft = {
    start,
    from: signal(s && accounts.some((a) => a.id === s.from) ? s.from : from),
    to: signal(s?.to ?? start.to),
    cc: signal(s?.cc ?? start.cc),
    bcc: signal(s?.bcc ?? ""),
    subject: signal(s?.subject ?? start.subject),
    text: signal(s?.text ?? start.text),
    files,
    edited: () =>
      files.edited() ||
      d.from() !== from ||
      d.to() !== start.to ||
      d.cc() !== start.cc ||
      d.bcc() !== "" ||
      d.subject() !== start.subject ||
      d.text() !== start.text,
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
      if (ended.has(d) || (!snap && !(key in saved))) return;
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

/** Another session (signed out, a new mode): its drafts are not this one's. */
export async function clearDrafts() {
  clearTimeout(timer);
  timer = undefined;
  for (const d of live.values()) d.files.drop();
  live.clear();
  saved = {};
  await Storage.removeItem(KEY);
}
