// The app's state on the TSX side: mirrors of what Rust owns (status,
// accounts, the loaded page of messages) and what the screens choose
// (mailbox, selection, search). Signals stay private; pages call these.
import { batch, resource, signal, Storage } from "nsh";
import { t } from "nsh/i18n";
import { fileSize } from "./lib/format";
import { clearDrafts, loadDrafts } from "./drafts";
import { Mail, type Account, type Address, type Backfill, type ErrorInfo, type Flags, type Folder, type MailEvent, type Signature, type Status, type Summary, type Thread } from "nsh-mail/native";

const PAGE = 30;
/** The most results a search shows. */
export const SEARCH_LIMIT = 50;
const NOTIFY_KEY = "nshmail.notify";
const THREADS_KEY = "nshmail.threads";

/** A row of a list: a conversation, or one message (search results, conversations turned off). */
export interface Row {
  /** The thread's id, or the message's. */
  key: string;
  /** A conversation rather than one message. */
  thread: boolean;
  threadId: string;
  latest: Summary;
  subject: string;
  count: number;
  unread: number;
  /** Who wrote, oldest first. */
  participants: Address[];
  hasAttachments: boolean;
  flagged: boolean;
}

const threadRow = (t: Thread): Row => ({
  key: t.id,
  thread: true,
  threadId: t.id,
  latest: t.latest,
  subject: t.subject,
  count: t.count,
  unread: t.unread,
  participants: t.participants,
  hasAttachments: t.hasAttachments,
  flagged: t.flagged,
});

const messageRow = (m: Summary): Row => ({
  key: m.id,
  thread: false,
  threadId: m.threadId,
  latest: m,
  subject: m.subject,
  count: 1,
  unread: m.seen ? 0 : 1,
  participants: m.from ? [m.from] : [],
  hasAttachments: m.hasAttachments,
  flagged: m.flagged,
});

export interface Mailbox {
  /** One account's id; null is every account (the unified view). */
  account: string | null;
  /** A folder, or the messages waiting in the outbox (../outbox.ts). */
  folder: Folder | "scheduled";
}

const status = signal<Status>(Mail.status());
const started = signal(false);
const accountList = signal<Account[]>([]);
const box = signal<Mailbox>({ account: null, folder: "inbox" });
const items = signal<Row[]>([]);
const listState = signal<"idle" | "loading" | "refreshing" | "more" | "error">("idle");
const more = signal(true);
const selected = signal<string | null>(null);
// The open conversation, as soon as it is chosen (its messages load after).
const selectedThread = signal<string | null>(null);
const query = signal("");
const results = signal<Row[] | null>(null);
const searchState = signal<"idle" | "loading" | "error">("idle");
const searchFailure = signal<string | null>(null);
export interface ToastAction {
  label: string;
  run: () => void;
}
const toastMessage = signal<{ id: number; text: string; tone: "neutral" | "error"; action?: ToastAction } | null>(null);
const notifyOn = signal(true);
const threadsOn = signal(true);
const unreadCounts = signal<Record<string, number>>({});
// Archived or deleted on screen, still undoable: hidden from the lists until the move is done. By
// row key; a conversation's id also hides its messages among search results.
const leaving = signal<Record<string, true>>({});
// Long-press selection on phones.
const picked = signal<Record<string, true>>({});

/** The engine's status (mode, sync). */
export const mailStatus = () => status();
/** Whether `start` finished, so the shell knows which screen to show. */
export const isStarted = () => started();
export const accounts = () => accountList();
/** The position of an account, which picks its color. */
export const accountIndex = (id: string) => accountList().findIndex((a) => a.id === id);
export const accountById = (id: string | null | undefined) => accountList().find((a) => a.id === id);
/** An account as people call it: its name, or its address when the name is missing or shared. */
export const accountName = (id: string | null | undefined) => {
  const a = accountById(id);
  if (!a) return undefined;
  const name = a.name?.trim();
  return name && accountList().filter((o) => o.name?.trim() === name).length === 1 ? name : a.email;
};
/** The user's own addresses, in lower case: "me" in a conversation. */
export const ownAddresses = () => new Set(accountList().map((a) => a.email.toLowerCase()));
export const mailbox = () => box();
const shown = (row: Row, gone: Record<string, true>) => !gone[row.key] && !gone[row.threadId];
/** The mailbox's rows: conversations, or messages when they are turned off. */
export const rows = () => {
  const gone = leaving();
  return items().filter((r) => shown(r, gone));
};
export const listStatus = () => listState();
export const hasMore = () => more();
/** The message the reader shows (in a conversation, the one expanded). */
export const selectedId = () => selected();
/** The row the reader shows: its conversation, or its message. */
export const selectedKey = () => (threadsOn() ? (selectedThread() ?? current()?.threadId ?? null) : selected());
export const searchQuery = () => query();
export const searchResults = () => {
  const gone = leaving();
  const list = results();
  return list && list.filter((r) => shown(r, gone));
};
/** A search is shown: the user typed something, or its results are on screen. */
export const searchActive = () => results() !== null || !!query().trim();
export const searchStatus = () => searchState();
/** Why the last search failed, as people read it. */
export const searchError = () => searchFailure();
export const conversationsEnabled = () => threadsOn();
/** Archived or deleted, its Undo still open. */
export const isLeaving = (key: string) => !!leaving()[key];
export const pickedIds = () => Object.keys(picked());
export const isPicked = (id: string) => !!picked()[id];
export const picking = () => pickedIds().length > 0;
export function togglePicked(id: string) {
  picked.update((p) => {
    const next = { ...p };
    if (next[id]) delete next[id];
    else next[id] = true;
    return next;
  });
}
export function clearPicked() {
  picked.set({});
}
export const toast = () => toastMessage();
export const notificationsEnabled = () => notifyOn();
/** Unread messages in an account's inbox; every inbox together when `id` is null. */
export const unreadCount = (id: string | null) => {
  const counts = unreadCounts();
  return id ? (counts[id] ?? 0) : accountList().reduce((sum, a) => sum + (counts[a.id] ?? 0), 0);
};

/** The open message, loaded whole. */
export const current = resource(selected, (id) => Mail.get(id));
/** The message the reader shows: the selected one once loaded; while another message of the same
 * conversation loads, the one before it, so the conversation stays on screen. */
export const shownMessage = () => {
  const m = current();
  const id = selected();
  if (!m || !id) return undefined;
  if (m.id === id) return m;
  const thread = conversation();
  return threadsOn() && thread?.some((s) => s.id === id) && thread.some((s) => s.id === m.id) ? m : undefined;
};
/** The open message's conversation, oldest first, while conversations are on. */
export const conversation = resource(
  () => (threadsOn() ? current()?.threadId : undefined) || undefined,
  (id) => Mail.thread(id),
);

let toastTimer: ReturnType<typeof setTimeout> | undefined;
let toastId = 0;
/** A message at the bottom; with an action (Undo) it stays a little longer, or `ms`. */
export function showToast(text: string, tone: "neutral" | "error" = "neutral", action?: ToastAction, ms?: number) {
  clearTimeout(toastTimer);
  toastMessage.set({ id: ++toastId, text, tone, action });
  toastTimer = setTimeout(() => toastMessage.set(null), ms ?? (action ? UNDO_MS : 3200));
}

export function dismissToast() {
  clearTimeout(toastTimer);
  toastMessage.set(null);
}

// Material: a snackbar with an action stays long enough to reach it.
export const UNDO_MS = 8000;

/** Archives or deletes rows (`thread`: conversations by id, else messages): they leave the
 * lists at once, and move on the server once the Undo has passed; a failure brings them back
 * with the error. */
export function moveMessages(ids: string[], folder: "archive" | "trash", thread = false) {
  if (!ids.length) return;
  const mark = (on: boolean) =>
    leaving.update((l) => {
      const next = { ...l };
      for (const id of ids) {
        if (on) next[id] = true;
        else delete next[id];
      }
      return next;
    });
  mark(true);
  if (ids.includes((thread ? selectedKey() : selected()) ?? "")) close();
  let undone = false;
  const commit = setTimeout(async () => {
    if (undone) return;
    const failed: unknown[] = [];
    const move = (id: string) => (thread ? Mail.moveThread(id, folder) : Mail.moveTo(id, folder));
    await Promise.all(ids.map((id) => move(id).catch((e: unknown) => failed.push(e))));
    await reload();
    if (results()) void runSearch(query());
    mark(false);
    void loadUnread();
    if (failed.length) showToast(errorText(failed[0]), "error");
  }, UNDO_MS);
  const key = `${folder === "archive" ? "archived" : "deleted"}${thread ? "Threads" : ""}`;
  showToast(ids.length === 1 ? t(`actions.${folder === "archive" ? "archived" : "deleted"}One`) : t(`actions.${key}`, { count: ids.length }), "neutral", {
    label: t("actions.undo"),
    run: () => {
      undone = true;
      clearTimeout(commit);
      mark(false);
      dismissToast();
    },
  });
}

/** Archives or deletes what the reader shows: its conversation, or its message. */
export function moveOpen(folder: "archive" | "trash") {
  const key = selectedKey();
  if (key) moveMessages([key], folder, threadsOn());
}

function close() {
  batch(() => {
    selected.set(null);
    selectedThread.set(null);
  });
}

/** A failure as people read it: by its code, with the engine's detail. */
export function errorInfoText(e: ErrorInfo): string {
  switch (e.code) {
    case "ERR_UNSUPPORTED":
      return t("errors.unsupported");
    case "ERR_AUTH":
      return t("errors.auth");
    case "ERR_NETWORK":
      return t("errors.network");
    case "ERR_INVALID_INPUT":
      return t("errors.invalid", { detail: e.message });
    case "ERR_OAUTH":
      return t("errors.oauth", { detail: e.message });
    case "ERR_TOO_LARGE":
      return t("compose.tooLarge", { limit: fileSize(Mail.attachmentLimit()) });
    default:
      return t("errors.generic", { detail: e.message });
  }
}

export function errorText(e: unknown): string {
  return errorInfoText({ code: (e as { code?: string })?.code ?? "", message: e instanceof Error ? e.message : String(e) });
}

let sessionListener: (() => void) | undefined;
/** Who reloads with each session (the outbox). */
export function onSession(listener: () => void) {
  sessionListener = listener;
}

let newMailListener: ((s: Summary) => void) | undefined;
/** Who hears new mail (the shell, for system notifications). */
export function onNewMail(listener: (s: Summary) => void) {
  newMailListener = listener;
}

function heard(e: MailEvent) {
  if (e.type === "error" && !e.accountId) {
    if (e.error.code === "ERR_SECRETS") showToast(t("errors.secrets"));
    else showToast(t("errors.generic", { detail: e.error.message }), "error");
  }
  if (e.type === "syncState" || e.type === "syncProgress" || e.type === "error") {
    status.set(Mail.status());
    return;
  }
  if (e.type === "newMail") newMailListener?.(e.summary);
  if (e.type === "changed" || e.type === "newMail") {
    status.set(Mail.status());
    if (status().mode === "none") return;
    void loadAccounts();
    syncArrived();
    void loadUnread();
    if (query().trim()) void runSearch(query());
    if (conversation()) conversation.refetch();
  }
}

// A sync reports each folder it touched, in bursts: the list follows them once, a moment later.
let syncTimer: ReturnType<typeof setTimeout> | undefined;

function syncArrived() {
  clearTimeout(syncTimer);
  syncTimer = setTimeout(() => void followSync(), SYNC_MS);
}

const SYNC_MS = 300;
/** Above this many rows the list takes a sync's changes in its first page instead of loading again. */
const MERGE_ABOVE = 200;

/** New mail and changes elsewhere reach the list: a short list loads again whole (a row a sync took
 * out of the view goes too); a long one merges its first page, so a sync costs the same however far
 * the user scrolled, and the rows below keep their identity and their place. */
async function followSync() {
  if (listState() !== "idle") return;
  if (items().length > MERGE_ABOVE) await mergeHead();
  else await reload();
}

// The listener lives with the module: a hot update stops and adds it again.
Mail.on(heard);

/** Opens the engine and loads the first page. */
export async function start() {
  const [saved, threads] = await Promise.all([Storage.getItem(NOTIFY_KEY), Storage.getItem(THREADS_KEY)]);
  notifyOn.set(saved !== "off");
  threadsOn.set(threads !== "off");
  await loadDrafts();
  try {
    status.set(await Mail.init());
  } catch (e) {
    showToast(errorText(e), "error");
  }
  if (status().mode !== "none") {
    await loadAccounts();
    await reload();
    void loadUnread();
  }
  sessionListener?.();
  started.set(true);
}

async function loadAccounts() {
  try {
    accountList.set(await Mail.accounts());
  } catch (e) {
    showToast(errorText(e), "error");
  }
}

/** Keeps an account's signature; rejects with the reason. */
export async function setSignature(id: string, signature: Signature) {
  const account = await Mail.setSignature(id, signature);
  accountList.update((list) => list.map((a) => (a.id === id ? account : a)));
}

// The counts are a hint next to the mailboxes: a failure keeps the last ones.
async function loadUnread() {
  try {
    unreadCounts.set(await Mail.unread());
  } catch {}
}

// Only the latest load of the list counts.
let loadToken = 0;
/** The most rows one page call returns: the server's own limit (`MAX_LIMIT`). */
const CHUNK = 200;

/** A page of older mail from the server, when the store ran out; null on failure (the list
 * still shows what is stored). */
async function olderPage(folder: Folder, account: string | null): Promise<Backfill | null> {
  try {
    return await Mail.older(folder, account);
  } catch {
    return null;
  }
}

/** Loads the mailbox again, keeping as many messages as are shown. */
export async function reload(state: "loading" | "refreshing" = "loading") {
  const token = ++loadToken;
  const { account, folder } = box();
  if (folder === "scheduled") {
    batch(() => {
      items.set([]);
      more.set(false);
      listState.set("idle");
    });
    return;
  }
  listState.set(state);
  const target = Math.max(PAGE, items().length);
  try {
    let page = await listUpTo({ folder, account }, target);
    // A short list may just be all that is stored: older mail waits on the server.
    let pending = false;
    if (page.length < target) {
      const older = await olderPage(folder, account);
      pending = older?.pending ?? false;
      if (older?.inserted) {
        page = await listUpTo({ folder, account }, target);
        if (token !== loadToken) return;
      }
    }
    if (token !== loadToken) return;
    batch(() => {
      items.set(merge(items(), page));
      more.set(page.length >= target || pending);
      listState.set("idle");
    });
  } catch (e) {
    if (token !== loadToken) return;
    listState.set("error");
    showToast(errorText(e), "error");
  }
}

async function listPage(q: { folder: Folder; account: string | null; before?: string; limit: number }): Promise<Row[]> {
  if (threadsOn()) return (await Mail.threads(q)).map(threadRow);
  return (await Mail.list(q)).map(messageRow);
}

/** The mailbox's first `limit` rows, in pages of [`CHUNK`]. */
async function listUpTo(q: { folder: Folder; account: string | null }, limit: number): Promise<Row[]> {
  const rows: Row[] = [];
  while (rows.length < limit) {
    const want = Math.min(CHUNK, limit - rows.length);
    const page = await listPage({ ...q, before: rows[rows.length - 1]?.key, limit: want });
    rows.push(...page);
    if (page.length < want) break;
  }
  return rows;
}

const sameAddress = (a: Address | null | undefined, b: Address | null | undefined) => a?.email === b?.email && a?.name === b?.name;

const sameMessage = (a: Summary, b: Summary) =>
  a.id === b.id &&
  a.date === b.date &&
  a.folder === b.folder &&
  a.seen === b.seen &&
  a.flagged === b.flagged &&
  a.hasAttachments === b.hasAttachments &&
  a.subject === b.subject &&
  a.snippet === b.snippet &&
  sameAddress(a.from, b.from) &&
  a.to.length === b.to.length &&
  a.to.every((address, i) => sameAddress(address, b.to[i]));

/** What a row shows, for telling a row a load brought back from the one on screen. */
const sameRow = (a: Row, b: Row) =>
  a.key === b.key &&
  a.count === b.count &&
  a.unread === b.unread &&
  a.flagged === b.flagged &&
  a.hasAttachments === b.hasAttachments &&
  a.subject === b.subject &&
  a.participants.length === b.participants.length &&
  sameMessage(a.latest, b.latest);

/** `fresh` with the rows `old` already shows kept as they are: unchanged rows keep their identity,
 * so the list repaints only what a load changed. */
function merge(old: Row[], fresh: Row[]): Row[] {
  if (!old.length) return fresh;
  const before = new Map(old.map((r) => [r.key, r]));
  return fresh.map((row) => {
    const known = before.get(row.key);
    return known && sameRow(known, row) ? known : row;
  });
}

/** The first page, merged into the rows already loaded: new rows go where their date puts them. */
async function mergeHead() {
  const { account, folder } = box();
  if (folder === "scheduled") return;
  const token = loadToken;
  try {
    const page = await listPage({ folder, account, limit: PAGE });
    if (token !== loadToken || listState() !== "idle") return;
    items.update((list) => {
      const fresh = new Map(page.map((r) => [r.key, r]));
      const rows = list.map((row) => {
        const next = fresh.get(row.key);
        if (!next) return row;
        fresh.delete(row.key);
        return sameRow(row, next) ? row : next;
      });
      // What the page has and the list does not arrived since: kept in newest-first order.
      for (const row of fresh.values()) {
        const at = rows.findIndex((r) => r.latest.date < row.latest.date || (r.latest.date === row.latest.date && r.key < row.key));
        rows.splice(at < 0 ? rows.length : at, 0, row);
      }
      return rows;
    });
  } catch {
    // A hint only: the rows stay as they were, and a refresh loads them again.
  }
}

export async function loadMore() {
  const last = items()[items().length - 1];
  const { account, folder } = box();
  if (!last || !more() || listState() !== "idle" || folder === "scheduled") return;
  const token = ++loadToken;
  listState.set("more");
  try {
    let page = await listPage({ folder, account, before: last.key, limit: PAGE });
    let pending = false;
    if (page.length < PAGE) {
      const older = await olderPage(folder, account);
      pending = older?.pending ?? false;
      if (older?.inserted) {
        page = await listPage({ folder, account, before: last.key, limit: PAGE });
        if (token !== loadToken) return;
      }
    }
    if (token !== loadToken) return;
    batch(() => {
      items.update((list) => {
        const known = new Set(list.map((r) => r.key));
        return [...list, ...page.filter((r) => !known.has(r.key))];
      });
      more.set(page.length >= PAGE || pending);
      listState.set("idle");
    });
  } catch (e) {
    if (token !== loadToken) return;
    listState.set("idle");
    showToast(errorText(e), "error");
  }
}

/** Syncs now (pull to refresh, the toolbar's Refresh). */
export async function refresh() {
  listState.set("refreshing");
  try {
    await Mail.refresh();
  } catch (e) {
    showToast(errorText(e), "error");
  }
  await reload("refreshing");
}

export function openMailbox(next: Mailbox) {
  batch(() => {
    box.set(next);
    picked.set({});
    items.set([]);
    more.set(true);
    selected.set(null);
    selectedThread.set(null);
  });
  void reload();
}

// A message's flags changed: its rows, its conversation and the reader follow.
function patch(id: string, flags: Flags) {
  const apply = <T extends { id: string; seen: boolean; flagged: boolean }>(m: T): T =>
    m.id === id ? { ...m, seen: flags.seen ?? m.seen, flagged: flags.flagged ?? m.flagged } : m;
  const applyRow = (r: Row): Row => {
    if (r.latest.id !== id || r.thread) return r;
    const latest = apply(r.latest);
    return { ...r, latest, unread: latest.seen ? 0 : 1, flagged: latest.flagged };
  };
  batch(() => {
    items.update((list) => list.map(applyRow));
    results.update((list) => list && list.map(applyRow));
    const messages = conversation();
    if (messages?.some((m) => m.id === id)) conversation.mutate(messages.map(apply));
    const open = current();
    if (open?.id === id) current.mutate(apply(open));
  });
}

/** Changes a message's flags at once on screen, and puts them back if the engine refuses. */
export async function setFlags(id: string, flags: Flags) {
  const before = [...items().map((r) => r.latest), ...(results() ?? []).map((r) => r.latest), ...(conversation() ?? [])].find((m) => m.id === id) ?? current();
  patch(id, flags);
  try {
    await Mail.setFlags(id, flags);
  } catch (e) {
    if (before) patch(id, { seen: before.seen, flagged: before.flagged });
    showToast(errorText(e), "error");
  }
  if (flags.seen !== undefined) void loadUnread();
}

/** Marks a conversation read or unread, or flags it (its latest message), at once on screen. */
export async function setThreadFlags(threadId: string, flags: Flags) {
  const applyRow = (r: Row): Row =>
    r.threadId !== threadId
      ? r
      : {
          ...r,
          unread: flags.seen === undefined ? r.unread : flags.seen ? 0 : r.count,
          flagged: flags.flagged ?? r.flagged,
          latest: { ...r.latest, seen: flags.seen ?? r.latest.seen },
        };
  const applyMessage = <T extends Summary>(m: T): T => (m.threadId === threadId ? { ...m, seen: flags.seen ?? m.seen } : m);
  batch(() => {
    items.update((list) => list.map(applyRow));
    results.update((list) => list && list.map((r) => ({ ...r, latest: applyMessage(r.latest), unread: applyMessage(r.latest).seen ? 0 : 1 })));
    const messages = conversation();
    if (messages) conversation.mutate(messages.map(applyMessage));
    const open = current();
    if (open) current.mutate(applyMessage(open));
  });
  try {
    await Mail.setThreadFlags(threadId, flags);
  } catch (e) {
    // The engine's state, not the guess: the rows and the conversation load again.
    showToast(errorText(e), "error");
    void reload();
    if (conversation()) conversation.refetch();
  }
  if (flags.seen !== undefined) void loadUnread();
}

/** Opens a row: a conversation shows its latest message expanded and is marked read. */
export function openRow(row: Row) {
  if (!row.thread) {
    openMessage(row.key);
    return;
  }
  batch(() => {
    selected.set(row.latest.id);
    selectedThread.set(row.threadId);
  });
  if (row.unread > 0) void setThreadFlags(row.threadId, { seen: true });
}

/** Shows a message (in its conversation, while they are on), marking it read. */
export function openMessage(id: string | null) {
  const m = id ? [...items().map((r) => r.latest), ...(results() ?? []).map((r) => r.latest), ...(conversation() ?? [])].find((s) => s.id === id) : undefined;
  batch(() => {
    selected.set(id);
    selectedThread.set(threadsOn() ? (m?.threadId ?? null) : null);
  });
  if (id && m && !m.seen) void setFlags(id, { seen: true });
}

/** The row `step` rows away from the open one (j / k). */
export function stepMessage(step: number) {
  const list = results() ?? rows();
  if (!list.length) return;
  const open = results() ? selected() : selectedKey();
  const at = list.findIndex((r) => r.key === open);
  const next = at < 0 ? 0 : Math.min(list.length - 1, Math.max(0, at + step));
  openRow(list[next]);
}

let searchToken = 0;
export async function runSearch(text: string) {
  query.set(text);
  const token = ++searchToken;
  if (!text.trim()) {
    batch(() => {
      results.set(null);
      searchState.set("idle");
      searchFailure.set(null);
    });
    return;
  }
  searchState.set("loading");
  searchFailure.set(null);
  try {
    const found = await Mail.search(text, SEARCH_LIMIT);
    if (token !== searchToken) return;
    batch(() => {
      results.set(found.map(messageRow));
      searchState.set("idle");
    });
  } catch (e) {
    if (token !== searchToken) return;
    batch(() => {
      searchState.set("error");
      searchFailure.set(errorText(e));
    });
  }
}

let searchTimer: ReturnType<typeof setTimeout> | undefined;
/** The user typed in a search field: the field follows at once, the search after a pause. */
export function typeSearch(text: string) {
  query.set(text);
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => void runSearch(text), 200);
}

export function clearSearch() {
  clearTimeout(searchTimer);
  searchToken++;
  batch(() => {
    query.set("");
    results.set(null);
    searchState.set("idle");
    searchFailure.set(null);
  });
}

/** Groups the lists by conversation, or lists every message on its own. */
export async function setConversationsEnabled(on: boolean) {
  threadsOn.set(on);
  openMailbox(box());
  await Storage.setItem(THREADS_KEY, on ? "on" : "off");
}

export async function setNotificationsEnabled(on: boolean) {
  notifyOn.set(on);
  await Storage.setItem(NOTIFY_KEY, on ? "on" : "off");
}

/** After onboarding or signing out: the engine's new session. */
export async function sessionChanged() {
  await clearDrafts();
  batch(() => {
    status.set(Mail.status());
    items.set([]);
    accountList.set([]);
    selected.set(null);
    selectedThread.set(null);
    results.set(null);
    query.set("");
    searchState.set("idle");
    searchFailure.set(null);
    box.set({ account: null, folder: "inbox" });
    unreadCounts.set({});
  });
  if (status().mode !== "none") {
    await loadAccounts();
    await reload();
    void loadUnread();
  }
  sessionListener?.();
}
