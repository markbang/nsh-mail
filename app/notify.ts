// The notification inbox's state beside the mailboxes': the feed, its
// sources, the keys agents read it with and the notification rules. Data
// comes from the backend (`Mail.feed`, `Mail.sources`, `Mail.apiKeys`); this
// module keeps what the screens choose and what Undo awaits.
import { batch, signal, Storage } from "nsh";
import { t } from "nsh/i18n";
import { Mail, type ApiKey, type CreatedApiKey, type CreatedSource, type NewApiKey, type NewSource, type Notification, type Source } from "nsh-mail/native";
import { errorText, notificationsEnabled, onFeedChanged, onSession, showToast, UNDO_MS } from "./store";

/** A page of the feed. */
const PAGE = 40;
/** The most notifications a search shows. */
const FEED_LIMIT = 80;

const RULES_KEY = "nshmail.notify.rules";

/** Which of the three priority bands a notification is in. */
export type Band = "high" | "normal" | "low";
/** What a notification does: a system notification, a line in the feed only, or nothing. */
export type NotifyMode = "notify" | "silent" | "off";
/** One mode per priority band of a source. */
export type SourceRules = Record<Band, NotifyMode>;
type Rules = Record<string, SourceRules>;

export const band = (priority: number): Band => (priority >= 7 ? "high" : priority >= 3 ? "normal" : "low");

const sourceList = signal<Source[]>([]);
const items = signal<Notification[]>([]);
const counts = signal<Record<string, number>>({});
const unreadTotal = signal(0);
const more = signal(false);
const listState = signal<"idle" | "loading" | "refreshing" | "more" | "error">("idle");
const failure = signal<string | null>(null);
const filter = signal<string | null>(null);
const query = signal("");
const selected = signal<string | null>(null);
const section = signal<"mail" | "notifications">("mail");
const keys = signal<ApiKey[]>([]);
const keyState = signal<"idle" | "loading" | "error">("idle");
const rules = signal<Rules>({});
// Archived on screen, still undoable: hidden from the list until Undo has passed.
const leaving = signal<Record<string, true>>({});

/** Every source, in the order they were added. */
export const sources = () => sourceList();
export const sourceById = (id: string | null | undefined) => sourceList().find((s) => s.id === id);
/** The feed's rows: the chosen source, minus the ones archived with Undo still open. */
export const notifications = () => {
  const gone = leaving();
  return items().filter((n) => !gone[n.id]);
};
/** Unread notifications per source, over the whole feed (the filter chips' counts). */
export const sourceCounts = () => counts();
/** Unread notifications in the feed: the tab's and the rail's badge. */
export const unreadNotifications = () => unreadTotal();
export const feedStatus = () => listState();
/** Why the last load failed, as people read it. */
export const feedError = () => failure();
export const feedFilter = () => filter();
export const feedQuery = () => query();
export const openNotification = () => {
  const id = selected();
  return id ? notifications().find((n) => n.id === id) : undefined;
};
/** The wide layout's two views: the mailboxes, or every notification. */
export const notificationsSection = () => section() === "notifications";
export function showNotifications() {
  section.set("notifications");
}
export function showMail() {
  section.set("mail");
}
/** The keys agents read and write the feed with. */
export const apiKeys = () => keys();
export const apiKeysState = () => keyState();
/** A key's sources, as people read them. */
export function keySources(key: ApiKey) {
  if (!key.sourceIds.length) return t("keys.allSources");
  return key.sourceIds.map((id) => sourceById(id)?.name ?? id).join(", ");
}
/** The rules of a source, with the defaults filled in. */
export const rulesOf = (sourceId: string): SourceRules => rules()[sourceId] ?? { high: "notify", normal: "notify", low: "notify" };
/** What a notification does on this device: its source's rule for its band, and the
 * source's minimum priority (below it the server stores but never pushes). */
export function notifyMode(notification: Notification): NotifyMode {
  const source = sourceById(notification.sourceId);
  if (source && !source.enabled) return "silent";
  return modeFor(notification.sourceId, notification.priority);
}

/** The source a mail account is (the server names it `src_m_<account>`; the demo `src-mail-…`). */
export function mailSourceId(accountId: string): string {
  return sourceList().find((s) => s.kind === "mail" && s.config?.accountId === accountId)?.id ?? `src-mail-${accountId}`;
}

/** A source's rule for a priority, without its pause: mail has its own way in. */
export function modeFor(sourceId: string, priority: number): NotifyMode {
  const source = sourceById(sourceId);
  if (source && priority < source.minPriority) return "silent";
  return rulesOf(sourceId)[band(priority)];
}

/** The sources' rules out of storage, at the module's first import. */
export async function loadRules() {
  try {
    const saved = await Storage.getItem(RULES_KEY);
    if (saved) rules.set(JSON.parse(saved) as Rules);
  } catch {
    // Storage keeps the last rules: a broken value only loses the customisation.
  }
}

/** Keeps a source's band rule; the feed's notifications follow at once. */
export async function setRule(sourceId: string, at: Band, mode: NotifyMode) {
  const next: Rules = { ...rules(), [sourceId]: { ...rulesOf(sourceId), [at]: mode } };
  rules.set(next);
  try {
    await Storage.setItem(RULES_KEY, JSON.stringify(next));
  } catch {
    showToast(t("errors.storage"), "error");
  }
}

let heard: ((added: Notification[]) => void) | undefined;
/** Who hears about notifications that arrived on their own (the shell, for system
 * notifications). Mail's own rows notify through `onNewMail`. */
export function onNewNotifications(listener: (added: Notification[]) => void) {
  heard = listener;
}

/** The ids the last load held, to tell what arrived since. */
let known: Set<string> | null = null;

async function loadSources() {
  try {
    sourceList.set(await Mail.sources());
  } catch (e) {
    failure.set(errorText(e));
  }
}

export async function loadFeed() {
  await Promise.all([loadSources(), reloadFeed()]);
}

/** The feed's query as the screen has it. */
function queryFor(limit: number) {
  return {
    source: filter(),
    text: query().trim(),
    unread: false,
    before: null,
    limit: Math.min(limit, FEED_LIMIT),
  };
}

let loadToken = 0;

/** Loads the feed again, keeping as many notifications as are shown. */
export async function reloadFeed(state: "loading" | "refreshing" = "loading") {
  const token = ++loadToken;
  listState.set(state);
  try {
    const page = await Mail.feed(queryFor(Math.max(PAGE, items().length)));
    if (token !== loadToken) return;
    const added = known ? page.items.filter((n) => !known!.has(n.id) && !n.read) : [];
    known = new Set([...(known ?? []), ...page.items.map((n) => n.id)]);
    batch(() => {
      items.set(page.items);
      counts.set(page.counts);
      unreadTotal.set(page.unread);
      more.set(page.more);
      listState.set("idle");
      failure.set(null);
    });
    // Only the sources that push: mail has its own notification.
    heard?.(added.filter((n) => sourceById(n.sourceId)?.kind !== "mail"));
  } catch (e) {
    if (token !== loadToken) return;
    batch(() => {
      listState.set("error");
      failure.set(errorText(e));
    });
  }
}

export async function loadMoreFeed() {
  const last = items()[items().length - 1];
  if (!last || !more() || listState() !== "idle") return;
  const token = ++loadToken;
  listState.set("more");
  try {
    const page = await Mail.feed({ ...queryFor(PAGE), before: last.id });
    if (token !== loadToken) return;
    batch(() => {
      items.update((list) => [...list, ...page.items.filter((n) => !list.some((l) => l.id === n.id))]);
      counts.set(page.counts);
      unreadTotal.set(page.unread);
      more.set(page.more);
      listState.set("idle");
    });
  } catch (e) {
    if (token !== loadToken) return;
    batch(() => {
      listState.set("idle");
      showToast(errorText(e), "error");
    });
  }
}

export function setFeedFilter(source: string | null) {
  filter.set(source);
  selected.set(null);
  void reloadFeed();
}

let typeTimer: ReturnType<typeof setTimeout> | undefined;
/** The user typed: the field follows at once, the feed after a pause. */
export function typeFeedQuery(text: string) {
  query.set(text);
  clearTimeout(typeTimer);
  typeTimer = setTimeout(() => {
    selected.set(null);
    void reloadFeed();
  }, 200);
}

export function submitFeedQuery(text: string) {
  clearTimeout(typeTimer);
  query.set(text);
  selected.set(null);
  void reloadFeed();
}

export function clearFeedQuery() {
  if (!query()) return;
  clearTimeout(typeTimer);
  query.set("");
  void reloadFeed();
}

/** Opens a notification, marking it read. */
export function openNotificationAt(id: string) {
  selected.set(id);
  const n = notifications().find((x) => x.id === id);
  if (n && !n.read) void markNotifications([id], true);
}

/** Marks notifications read or unread, at once on screen. */
export async function markNotifications(ids: string[], read: boolean) {
  if (!ids.length) return;
  const patch = (n: Notification) => (ids.includes(n.id) ? { ...n, read } : n);
  const before = unreadTotal();
  const changed = items().filter((n) => ids.includes(n.id) && n.read !== read).length;
  batch(() => {
    items.update((list) => list.map(patch));
    unreadTotal.set(Math.max(0, read ? before - changed : before + changed));
  });
  try {
    await Mail.markRead(ids, read);
  } catch (e) {
    batch(() => {
      items.update((list) => list.map((n) => (ids.includes(n.id) ? { ...n, read: !read } : n)));
      unreadTotal.set(before);
    });
    showToast(errorText(e), "error");
    return;
  }
  void reloadFeed();
}

/** Archives notifications: they leave the feed at once, and go once Undo has passed. */
export function archiveNotifications(ids: string[]) {
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
    try {
      await Mail.archive(ids);
    } catch (e) {
      showToast(errorText(e), "error");
    }
    await reloadFeed();
    mark(false);
  }, UNDO_MS);
  showToast(ids.length === 1 ? t("notify.archivedOne") : t("notify.archived", { count: ids.length }), "neutral", {
    label: t("actions.undo"),
    run: () => {
      undone = true;
      clearTimeout(commit);
      mark(false);
    },
  });
}

/** Sends a test notification through a source, and reloads the feed with it. */
export async function testSource(source: Source) {
  try {
    await Mail.testSource(source.id);
    showToast(t("sources.testSent", { name: source.name }));
    await reloadFeed("refreshing");
  } catch (e) {
    showToast(errorText(e), "error");
  }
}

/** Adds a source; resolves with its ingest URL when it has one. */
export async function createSource(input: NewSource): Promise<CreatedSource> {
  const created = await Mail.createSource(input);
  sourceList.update((list) => [...list, created.source]);
  return created;
}

/** Pauses or resumes a source, or renames it and sets its colour. */
export async function updateSource(source: Source): Promise<Source> {
  const updated = await Mail.updateSource(source);
  batch(() => {
    sourceList.update((list) => list.map((s) => (s.id === updated.id ? updated : s)));
    // Its notifications follow the new colour and icon in the feed at once.
    void reloadFeed("refreshing");
  });
  return updated;
}

export async function setSourceEnabled(source: Source, enabled: boolean) {
  try {
    await updateSource({ ...source, enabled });
    showToast(enabled ? t("sources.resumed", { name: source.name }) : t("sources.paused", { name: source.name }));
  } catch (e) {
    showToast(errorText(e), "error");
  }
}

export async function deleteSource(source: Source) {
  try {
    await Mail.deleteSource(source.id);
    batch(() => {
      sourceList.update((list) => list.filter((s) => s.id !== source.id));
      items.update((list) => list.filter((n) => n.sourceId !== source.id));
      if (filter() === source.id) filter.set(null);
    });
    showToast(t("sources.deleted", { name: source.name }));
    await reloadFeed("refreshing");
  } catch (e) {
    showToast(errorText(e), "error");
    throw e;
  }
}

/** Gives a source a new ingest URL; the one before stops working. */
export async function rotateSource(source: Source): Promise<CreatedSource> {
  return Mail.rotateSourceToken(source.id);
}

export async function loadKeys() {
  keyState.set("loading");
  try {
    keys.set(await Mail.apiKeys());
    keyState.set("idle");
  } catch (e) {
    keyState.set("error");
    showToast(errorText(e), "error");
  }
}

/** Creates a key; resolves with its token, which is shown only here. */
export async function createKey(input: NewApiKey): Promise<CreatedApiKey> {
  const created = await Mail.createApiKey(input);
  keys.update((list) => [...list, created.key]);
  return created;
}

export async function revokeKey(key: ApiKey) {
  try {
    await Mail.revokeApiKey(key.id);
    keys.update((list) => list.filter((k) => k.id !== key.id));
    showToast(t("keys.revoked", { name: key.name }));
  } catch (e) {
    showToast(errorText(e), "error");
    throw e;
  }
}

/** The notifications a system notification may announce: their source's rules say so,
 * the app's switch is on, and they arrived after the first load. */
export function announceable(added: Notification[]) {
  if (!notificationsEnabled()) return [];
  return added.filter((n) => notifyMode(n) === "notify");
}

/** After onboarding, signing out or a new session: nothing of the last one stays. */
export async function feedSessionChanged() {
  batch(() => {
    sourceList.set([]);
    items.set([]);
    counts.set({});
    unreadTotal.set(0);
    more.set(false);
    listState.set("idle");
    failure.set(null);
    filter.set(null);
    query.set("");
    selected.set(null);
    section.set("mail");
    keys.set([]);
    keyState.set("idle");
    leaving.set({});
  });
  known = null;
  await loadFeed();
  if (apiKeys().length === 0) void loadKeys();
}

// The feed follows the engine: reloads when something arrives or a session starts.
onSession(() => void feedSessionChanged());
onFeedChanged(() => void loadFeed());
void loadRules();
