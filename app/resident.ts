// Staying around on the desktops: closing the window hides it to the tray,
// and new mail shows a system notification while the app is not in front.
import { cleanup, effect, lifecycle, Notifications, platform } from "nsh";
import { t } from "nsh/i18n";
import { currentWindow, isAvailable, Tray, type WindowRef } from "nsh/desktop";
import type { Summary } from "nsh-mail/native";
import icon from "./assets/tray.svg";
// Linux tints an SVG for the app's light or dark scheme, not the panel's (black on GNOME's dark bar): the full-color icon reads on any panel.
import linuxIcon from "./assets/tray-linux.png";
import { displayName } from "./lib/format";
import { notificationsEnabled, onNewMail } from "./store";
import { announceable, modeFor, onNewNotifications, sourceById } from "./notify";

const PREFIX = "message:";
const NOTE_PREFIX = "notification:";

/** Call once while the root renders: tray, close-to-tray and notifications. */
export function stayResident(openFromNotification: (id: string) => void, openNote: (id: string) => void) {
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
        icon: platform.os === "linux" ? linuxIcon : icon,
        tooltip: "nsh-mail",
        onClick: () => win.focus(),
        menu: [{ label: t("tray.open"), onSelect: () => win.focus() }, { type: "separator" }, { role: "quit", label: t("tray.quit") }],
      });
    });
    cleanup(() => Tray.set(null));
  }

  onNewMail((s: Summary) => {
    // Mail is a source too: its rules say whether its notifications reach the device.
    if (!notificationsEnabled() || inFront() || modeFor(`src-mail-${s.accountId}`, 4) !== "notify") return;
    void Notifications.show({ id: `${PREFIX}${s.id}`, title: displayName(s.from), body: s.subject || t("reader.noSubject") }).catch(() => undefined);
  });
  onNewNotifications((added) => {
    if (inFront()) return;
    // At most three at once: a burst is one glance, not a stack of banners.
    for (const n of announceable(added).slice(0, 3)) {
      void Notifications.show({ id: `${NOTE_PREFIX}${n.id}`, title: n.sender || sourceById(n.sourceId)?.name || "", body: n.title }).catch(() => undefined);
    }
  });
  try {
    cleanup(
      Notifications.on("press", ({ id }) => {
        win?.focus();
        if (id.startsWith(NOTE_PREFIX)) {
          openNote(id.slice(NOTE_PREFIX.length));
          return;
        }
        if (id.startsWith(PREFIX)) openFromNotification(id.slice(PREFIX.length));
      }),
    );
  } catch {
    // A host that grants no notifications: nothing to listen to.
  }
}
