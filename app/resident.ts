// Staying around on the desktops: closing the window hides it to the tray,
// and new mail shows a system notification while the app is not in front.
import { cleanup, effect, lifecycle, Notifications } from "nsh";
import { t } from "nsh/i18n";
import { currentWindow, isAvailable, Tray, type WindowRef } from "nsh/desktop";
import type { Summary } from "nsh-mail/native";
import icon from "./assets/tray.svg";
import { displayName } from "./lib/format";
import { notificationsEnabled, onNewMail } from "./store";

const PREFIX = "message:";

/** Call once while the root renders: tray, close-to-tray and notifications. */
export function stayResident(openFromNotification: (id: string) => void) {
  // Only a component's render may ask for its window: keep it for later.
  const win: WindowRef | null = isAvailable ? currentWindow() : null;
  /** The window is shown and has the keyboard, or (phones, web) the app is in front. */
  const inFront = () => {
    if (!win) return lifecycle() === "active";
    const state = win.state();
    return !!state && state.visible && state.focused && !state.minimized;
  };
  if (win) {
    cleanup(win.on("closePress", () => win.hide()));
    effect(() => {
      Tray.set({
        icon,
        tooltip: "nsh-mail",
        onClick: () => win.focus(),
        menu: [{ label: t("tray.open"), onSelect: () => win.focus() }, { type: "separator" }, { role: "quit", label: t("tray.quit") }],
      });
    });
    cleanup(() => Tray.set(null));
  }

  onNewMail((s: Summary) => {
    if (!notificationsEnabled() || inFront()) return;
    void Notifications.show({ id: `${PREFIX}${s.id}`, title: displayName(s.from), body: s.subject || t("reader.noSubject") }).catch(() => undefined);
  });
  try {
    cleanup(
      Notifications.on("press", ({ id }) => {
        if (!id.startsWith(PREFIX)) return;
        win?.focus();
        openFromNotification(id.slice(PREFIX.length));
      }),
    );
  } catch {
    // A host that grants no notifications: nothing to listen to.
  }
}
