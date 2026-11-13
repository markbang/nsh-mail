// The app's state on the TSX side: mirrors of what Rust owns (status,
// accounts, the loaded page of messages) and what the screens choose
// (mailbox, selection, search). Signals stay private; pages call these.
import { batch, resource, signal, Storage } from "nsh";
import { t } from "nsh/i18n";
import { fileSize } from "./lib/format";
import { Mail, type Account, type Flags, type Folder, type MailEvent, type Status, type Summary } from "nsh-mail/native";

const PAGE = 30;
/** The most results a search shows. */
export const SEARCH_LIMIT = 50;
const NOTIFY_KEY = "nshmail.notify";

export interface Mailbox {
  /** One account's id; null is every account (the unified view). */
  account: string | null;
  folder: Folder;
}

const status = signal<Status>(Mail.status());
const started = signal(false);
const accountList = signal<Account[]>([]);
const box = signal<Mailbox>({ account: null, folder: "inbox" });
const items = signal<Summary[]>([]);
const listState = signal<"idle" | "loading" | "refreshing" | "more" | "error">("idle");
const more = signal(true);
const selected = signal<string | null>(null);
const query = signal("");
const results = signal<Summary[] | null>(null);
export interface ToastAction {
  label: string;
  run: () => void;
}
const toastMessage = signal<{ id: number; text: string; tone: "neutral" | "error"; action?: ToastAction } | null>(null);
const notifyOn = signal(true);
const unreadCounts = signal<Record<string, number>>({});
// Archived or deleted on screen, still undoable: hidden from the lists until the move is done.
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
export const mailbox = () => box();
export const messages = () => {
  const gone = leaving();
  return items().filter((m) => !gone[m.id]);
};
export const listStatus = () => listState();
export const hasMore = () => more();
export const selectedId = () => selected();
export const searchQuery = () => query();
export const searchResults = () => {
  const gone = leaving();
  const list = results();
  return list && list.filter((m) => !gone[m.id]);
};
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

let toastTimer: ReturnType<typeof setTimeout> | undefined;
let toastId = 0;
/** A message at the bottom; with an action (Undo) it stays a little longer. */
export function showToast(text: string, tone: "neutral" | "error" = "neutral", action?: ToastAction) {
  clearTimeout(toastTimer);
  toastMessage.set({ id: ++toastId, text, tone, action });
  toastTimer = setTimeout(() => toastMessage.set(null), action ? UNDO_MS : 3200);
}

export function dismissToast() {
  clearTimeout(toastTimer);
  toastMessage.set(null);
}

// Material: a snackbar with an action stays long enough to reach it.
const UNDO_MS = 8000;

/** Archives or deletes messages: they leave the lists at once, and move on the server once
 * the Undo has passed; a failure brings them back with the error. */
export function moveMessages(ids: string[], folder: "archive" | "trash") {
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
  if (ids.includes(selected() ?? "")) selected.set(null);
  let undone = false;
  const commit = setTimeout(async () => {
    if (undone) return;
    const failed: unknown[] = [];
    await Promise.all(ids.map((id) => Mail.moveTo(id, folder).catch((e: unknown) => failed.push(e))));
    await reload();
    if (results()) void runSearch(query());
    mark(false);
    void loadUnread();
    if (failed.length) showToast(errorText(failed[0]), "error");
  }, UNDO_MS);
  const key = folder === "archive" ? "archived" : "deleted";
  showToast(ids.length === 1 ? t(`actions.${key}One`) : t(`actions.${key}`, { count: ids.length }), "neutral", {
    label: t("actions.undo"),
    run: () => {
      undone = true;
      clearTimeout(commit);
      mark(false);
      dismissToast();
    },
  });
}

/** A failure as people read it: by its code, with the engine's detail. */
export function errorText(e: unknown): string {
  const code = (e as { code?: string })?.code;
  const message = e instanceof Error ? e.message : String(e);
  switch (code) {
    case "ERR_UNSUPPORTED":
      return t("errors.unsupported");
    case "ERR_AUTH":
      return t("errors.auth");
    case "ERR_NETWORK":
      return t("errors.network");
    case "ERR_INVALID_INPUT":
      return t("errors.invalid", { detail: message });
    case "ERR_OAUTH":
      return t("errors.oauth", { detail: message });
    case "ERR_TOO_LARGE":
      return t("compose.tooLarge", { limit: fileSize(Mail.attachmentLimit()) });
    default:
      return t("errors.generic", { detail: message });
  }
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
  if (e.type === "syncState" || e.type === "error") {
    status.set(Mail.status());
    return;
  }
  if (e.type === "newMail") newMailListener?.(e.summary);
  if (e.type === "changed" || e.type === "newMail") {
    status.set(Mail.status());
    if (status().mode === "none") return;
    void loadAccounts();
    void reload();
    void loadUnread();
    if (query().trim()) void runSearch(query());
  }
}

// The listener lives with the module: a hot update stops and adds it again.
Mail.on(heard);

/** Opens the engine and loads the first page. */
export async function start() {
  const saved = await Storage.getItem(NOTIFY_KEY);
  notifyOn.set(saved !== "off");
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
  started.set(true);
}

async function loadAccounts() {
  try {
    accountList.set(await Mail.accounts());
  } catch (e) {
    showToast(errorText(e), "error");
  }
}

// The counts are a hint next to the mailboxes: a failure keeps the last ones.
async function loadUnread() {
  try {
    unreadCounts.set(await Mail.unread());
  } catch {}
}

// Only the latest load of the list counts.
let loadToken = 0;

/** Loads the mailbox again, keeping as many messages as are shown. */
export async function reload(state: "loading" | "refreshing" = "loading") {
  const token = ++loadToken;
  const { account, folder } = box();
  listState.set(state);
  try {
    const page = await Mail.list({ folder, account, limit: Math.max(PAGE, items().length) });
    if (token !== loadToken) return;
    batch(() => {
      items.set(page);
      more.set(page.length >= Math.max(PAGE, items().length));
      listState.set("idle");
    });
  } catch (e) {
    if (token !== loadToken) return;
    listState.set("error");
    showToast(errorText(e), "error");
  }
}

export async function loadMore() {
  const last = items()[items().length - 1];
  if (!last || !more() || listState() !== "idle") return;
  const token = ++loadToken;
  const { account, folder } = box();
  listState.set("more");
  try {
    const page = await Mail.list({ folder, account, before: last.id, limit: PAGE });
    if (token !== loadToken) return;
    batch(() => {
      items.update((list) => [...list, ...page.filter((m) => !list.some((l) => l.id === m.id))]);
      more.set(page.length >= PAGE);
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
  });
  void reload();
}

function patch(id: string, flags: Flags) {
  const apply = <T extends { id: string; seen: boolean; flagged: boolean }>(m: T): T =>
    m.id === id ? { ...m, seen: flags.seen ?? m.seen, flagged: flags.flagged ?? m.flagged } : m;
  batch(() => {
    items.update((list) => list.map(apply));
    results.update((list) => list && list.map(apply));
    const open = current();
    if (open?.id === id) current.mutate(apply(open));
  });
}

/** Changes flags at once on screen, and puts them back if the engine refuses. */
export async function setFlags(id: string, flags: Flags) {
  const before = [...items(), ...(results() ?? [])].find((m) => m.id === id) ?? current();
  patch(id, flags);
  try {
    await Mail.setFlags(id, flags);
  } catch (e) {
    if (before) patch(id, { seen: before.seen, flagged: before.flagged });
    showToast(errorText(e), "error");
  }
  if (flags.seen !== undefined) void loadUnread();
}

/** Shows a message, marking it read. */
export function openMessage(id: string | null) {
  selected.set(id);
  const m = id && [...items(), ...(results() ?? [])].find((s) => s.id === id);
  if (m && !m.seen) void setFlags(id, { seen: true });
}

/** The message `step` rows away from the open one (j / k). */
export function stepMessage(step: number) {
  const list = results() ?? items();
  if (!list.length) return;
  const at = list.findIndex((m) => m.id === selected());
  const next = at < 0 ? 0 : Math.min(list.length - 1, Math.max(0, at + step));
  openMessage(list[next].id);
}

let searchToken = 0;
export async function runSearch(text: string) {
  query.set(text);
  const token = ++searchToken;
  if (!text.trim()) {
    results.set(null);
    return;
  }
  try {
    const found = await Mail.search(text, SEARCH_LIMIT);
    if (token === searchToken) results.set(found);
  } catch (e) {
    if (token === searchToken) showToast(errorText(e), "error");
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
  });
}

export async function setNotificationsEnabled(on: boolean) {
  notifyOn.set(on);
  await Storage.setItem(NOTIFY_KEY, on ? "on" : "off");
}

/** After onboarding or signing out: the engine's new session. */
export async function sessionChanged() {
  batch(() => {
    status.set(Mail.status());
    items.set([]);
    accountList.set([]);
    selected.set(null);
    results.set(null);
    query.set("");
    box.set({ account: null, folder: "inbox" });
    unreadCounts.set({});
  });
  if (status().mode !== "none") {
    await loadAccounts();
    await reload();
    void loadUnread();
  }
}
