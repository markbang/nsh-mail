// The outbox as the UI sees it: the undo delay (a setting), the messages that
// wait to go (scheduled, or failed and waiting for the user), and what Send
// with Undo and Schedule send do with a draft. The messages themselves wait in
// Rust (local mode: the app's database) or on the server (cloud mode).
import { signal, Storage } from "nsh";
import { t } from "nsh/i18n";
import { Mail, type Draft, type MailEvent, type Outgoing, type SendAt } from "nsh-mail/native";
import { sendDate } from "./lib/format";
import { dismissToast, errorText, mailStatus, onSession, showToast, UNDO_MS } from "./store";

const DELAY_KEY = "nshmail.undoSend";
/** The undo delays Settings offers, in seconds; 0 sends at once. */
export const UNDO_DELAYS = [0, 5, 10, 20, 30];
const DEFAULT_DELAY = 10;

const delay = signal(DEFAULT_DELAY);
const items = signal<Outgoing[]>([]);
const loaded = signal(false);

/** Seconds a sent message waits for Undo. */
export const undoDelay = () => delay();
export async function setUndoDelay(seconds: number) {
  delay.set(seconds);
  await Storage.setItem(DELAY_KEY, String(seconds));
}

/** What waits for the user: scheduled messages and failed ones (not those in their undo window), the next first. */
export const waiting = (account: string | null) => items().filter((o) => (o.scheduled || o.error) && (!account || o.draft.accountId === account));
export const outboxLoaded = () => loaded();

export async function loadOutbox() {
  try {
    items.set(await Mail.outbox());
    loaded.set(true);
  } catch (e) {
    showToast(errorText(e), "error");
  }
}

onSession(async () => {
  const raw = await Storage.getItem(DELAY_KEY);
  const saved = raw === null ? NaN : Number(raw);
  delay.set(UNDO_DELAYS.includes(saved) ? saved : DEFAULT_DELAY);
  items.set([]);
  loaded.set(false);
  if (mailStatus().mode !== "none") void loadOutbox();
});

// Sent from this composer with Undo still on screen: "Message sent" once they go.
const announced = new Set<string>();

function heard(e: MailEvent) {
  if (e.type !== "outbox") return;
  void loadOutbox();
  if (e.state === "sent") {
    if (announced.delete(e.id)) showToast(t("compose.sent"));
    else if (e.late) showToast(t("outbox.sentLate", { subject: e.subject || t("reader.noSubject") }));
  } else if (e.state === "failed") {
    announced.delete(e.id);
    showToast(t("outbox.failed", { subject: e.subject || t("reader.noSubject"), detail: e.error ? errorText(Object.assign(new Error(e.error.message), { code: e.error.code })) : "" }), "error");
  }
}
Mail.on(heard);

/** Eight in the morning tomorrow, and on the next Monday (unless that is tomorrow): Schedule send's presets. */
export function schedulePresets(now = Date.now()): { id: "tomorrow" | "monday"; at: number }[] {
  const at = (days: number) => {
    const d = new Date(now);
    d.setDate(d.getDate() + days);
    d.setHours(8, 0, 0, 0);
    return d.getTime();
  };
  const today = new Date(now).getDay();
  const toMonday = ((8 - today) % 7) || 7;
  return toMonday === 1 ? [{ id: "tomorrow", at: at(1) }] : [{ id: "tomorrow", at: at(1) }, { id: "monday", at: at(toMonday) }];
}

/**
 * Puts `draft` in the outbox: after the undo delay, or at `at` (Schedule send). A snackbar says
 * so with Undo, which takes it back out and calls `onUndo` with it (its files staged again);
 * `onKept` runs once the Undo is gone. Rejects, with nothing queued, when the outbox refuses it.
 */
export async function queueDraft(draft: Draft, at: number | null, hooks: { onUndo: (o: Outgoing) => void; onKept: () => void }) {
  const when: SendAt = at === null ? { delay: delay() } : { sendAt: at };
  const queued = await Mail.queue(draft, when);
  const window = at === null ? delay() * 1000 : UNDO_MS;
  if (at === null) announced.add(queued.id);
  let undone = false;
  const kept = setTimeout(() => !undone && hooks.onKept(), window);
  showToast(
    at === null ? t("compose.sendingUndo") : t("compose.scheduledFor", { when: sendDate(queued.sendAt) }),
    "neutral",
    {
      label: t("actions.undo"),
      run: async () => {
        dismissToast();
        try {
          const back = await Mail.cancel(queued.id);
          undone = true;
          clearTimeout(kept);
          announced.delete(queued.id);
          hooks.onUndo(back);
        } catch (e) {
          showToast((e as { code?: string })?.code === "ERR_NOT_FOUND" ? t("outbox.tooLate") : errorText(e), "error");
        }
      },
    },
    window,
  );
  void loadOutbox();
  return queued;
}

/** Takes a scheduled message out of the outbox for good, with Undo; its files go once the Undo passes. */
export async function cancelScheduled(id: string) {
  let back: Outgoing;
  try {
    back = await Mail.cancel(id);
  } catch (e) {
    return showToast(errorText(e), "error");
  }
  let undone = false;
  const drop = setTimeout(() => {
    if (!undone) for (const f of back.files) void Mail.unstage(f.id).catch(() => {});
  }, UNDO_MS);
  showToast(t("outbox.cancelled"), "neutral", {
    label: t("actions.undo"),
    run: async () => {
      undone = true;
      clearTimeout(drop);
      dismissToast();
      try {
        await Mail.queue(back.draft, { sendAt: Math.max(back.sendAt, Date.now()) });
      } catch (e) {
        showToast(errorText(e), "error");
      }
    },
  });
}
