#!/usr/bin/env python3
"""Drives nsh-mail on a running emulator with adb and screenshots each screen,
in light and dark mode: inbox, swipe to archive and Undo, selection, pull to
refresh, reader, a conversation, compose, Schedule send, search, mailboxes, Scheduled, settings, a signature, a tab switch that must
keep the inbox's scroll position; then larger text and a tablet-sized screen.

Usage: android-ui.py <dir with the APK> <screenshot dir>
Finds views by their testID (the accessibility resource id) or their text.
"""
import glob, os, re, subprocess, sys, time, xml.etree.ElementTree as ET

PKG = "top.bangwu.nshmail"
apk_dir, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)
failures = []


def adb(*args, check=True):
    return subprocess.run(["adb", *args], check=check, capture_output=True, encoding="utf-8", errors="replace").stdout


def shot(name):
    time.sleep(1.2)
    with open(os.path.join(out, f"{name}.png"), "wb") as f:
        f.write(subprocess.run(["adb", "exec-out", "screencap", "-p"], check=True, capture_output=True).stdout)
    print("shot", name, flush=True)


def nodes():
    for _ in range(3):
        adb("shell", "uiautomator", "dump", "/sdcard/ui.xml", check=False)
        xml = adb("exec-out", "cat", "/sdcard/ui.xml", check=False)
        if xml.strip().startswith("<?xml"):
            return list(ET.fromstring(xml).iter("node"))
        time.sleep(1)
    return []


def matches(n, key):
    rid, text, desc = n.get("resource-id", ""), n.get("text", ""), n.get("content-desc", "")
    if key.startswith("id:"):
        return re.fullmatch(key[3:], rid) is not None
    return key in (text, desc)


def settled(key, tries=10):
    """`key`'s bounds once they stop moving: a cold start on a slow emulator is still sliding in."""
    last = bounds(key)
    for _ in range(tries):
        time.sleep(1)
        now = bounds(key)
        if now == last:
            return now
        last = now
    return last


def bounds(key, timeout=20):
    end = time.time() + timeout
    while time.time() < end:
        found = nodes()
        if dismiss_anr(found):
            continue
        for n in found:
            if matches(n, key):
                x1, y1, x2, y2 = map(int, re.findall(r"\d+", n.get("bounds")))
                if x2 > x1 and y2 > y1:
                    return x1, y1, x2, y2
        time.sleep(1)
    return None


def during(gesture, name, at=0.7):
    """Screenshots `name` while `gesture` (an `adb shell input` command) is under way."""
    p = subprocess.Popen(["adb", "shell", "input", *gesture])
    time.sleep(at)
    with open(os.path.join(out, f"{name}.png"), "wb") as f:
        f.write(subprocess.run(["adb", "exec-out", "screencap", "-p"], check=True, capture_output=True).stdout)
    p.wait()
    print("shot", name, flush=True)


def dismiss_anr(found):
    """The emulator's own apps (the launcher) sometimes stop responding: wait for them, keep going."""
    for n in found:
        if n.get("resource-id") == "android:id/aerr_wait":
            x1, y1, x2, y2 = map(int, re.findall(r"\d+", n.get("bounds")))
            adb("shell", "input", "tap", str((x1 + x2) // 2), str((y1 + y2) // 2))
            print("dismissed a system ANR dialog", flush=True)
            time.sleep(1)
            return True
    return False


def find(key, timeout=20):
    end = time.time() + timeout
    while time.time() < end:
        found = nodes()
        if dismiss_anr(found):
            continue
        for n in found:
            if matches(n, key):
                x1, y1, x2, y2 = map(int, re.findall(r"\d+", n.get("bounds")))
                if x2 > x1 and y2 > y1:
                    return (x1 + x2) // 2, (y1 + y2) // 2
        time.sleep(1)
    return None


def tap(key, timeout=20):
    at = find(key, timeout)
    if not at:
        fail(f"not found: {key}")
        return False
    adb("shell", "input", "tap", str(at[0]), str(at[1]))
    return True


def wait(key, timeout=30):
    if not find(key, timeout):
        fail(f"never showed: {key}")


def centre(node):
    """The middle of a uiautomator node."""
    x1, y1, x2, y2 = map(int, re.findall(r"\d+", node.get("bounds")))
    return (x1 + x2) // 2, (y1 + y2) // 2


def fail(what):
    failures.append(f"[{mode}] {what}")
    print("FAIL", what, flush=True)
    shot(f"{mode}-fail-{len(failures)}")


def back():
    adb("shell", "input", "keyevent", "4")
    time.sleep(0.8)


def back_to(key, tries=3, timeout=6):
    """Back until `key` shows: a page's keyboard takes the first Back, the page the next."""
    for _ in range(tries):
        if find(key, timeout):
            return True
        back()
    return bool(find(key, timeout))


def prop(name):
    return adb("shell", "getprop", name, check=False).strip()


def booted(timeout=240):
    """Boot is done for the user, not just the kernel: until then "Pixel is starting…" covers the screen."""
    adb("wait-for-device")
    end = time.time() + timeout
    while time.time() < end:
        if prop("sys.boot_completed") == "1" and prop("init.svc.bootanim") in ("stopped", "") and prop("sys.user.0.ce_available") == "true":
            break
        time.sleep(2)
    else:
        print("boot not reported complete after", timeout, "s, going on", flush=True)
    # Right after boot the system compiles and indexes: wait (bounded) for the load to drop.
    cpus = adb("shell", "nproc", check=False).strip()
    cpus = int(cpus) if cpus.isdigit() else 2
    end, load = time.time() + 120, None
    while time.time() < end:
        try:
            load = float(adb("shell", "cat", "/proc/loadavg", check=False).split()[0])
        except (IndexError, ValueError):
            break
        if load < cpus:
            break
        time.sleep(5)
    print("booted, load", load, "on", cpus, "cpus", flush=True)


def scroll_to(key, tries=4, timeout=3):
    """A row below the fold: swipes the page up until `key` shows."""
    for _ in range(tries):
        if find(key, timeout):
            return True
        adb("shell", "input", "swipe", str(w // 2), str(h * 8 // 10), str(w // 2), str(h * 4 // 10), "300")
        time.sleep(0.6)
    return bool(find(key, timeout))


def focused():
    for line in adb("shell", "dumpsys", "window", check=False).splitlines():
        if "mCurrentFocus" in line:
            return line.strip()
    return ""


def launch():
    adb("shell", "am", "force-stop", PKG)
    adb("shell", "monkey", "-p", PKG, "-c", "android.intent.category.LAUNCHER", "1")
    # The app's window has the focus (no system dialog over it) and its list stopped moving.
    end = time.time() + 60
    while PKG not in focused() and time.time() < end:
        dismiss_anr(nodes())
        time.sleep(1)
    wait("id:messages", 60)
    settled("id:message-.*")


def first_row():
    for n in nodes():
        if re.fullmatch(r"message-.*", n.get("resource-id", "")):
            return n.get("resource-id")
    return None


def swipe_archive(row, tries=3):
    """Swipes the first row away until it is archived: True once the Undo snackbar shows."""
    for attempt in range(1, tries + 1):
        x1, y1, x2, y2 = row
        y = (y1 + y2) // 2
        before = first_row()
        # Slow enough that the distance alone (past the 35% mark) archives, whatever the measured velocity.
        adb("shell", "input", "swipe", str((x1 + x2) // 2 - (x2 - x1) // 4), str(y), str(x2 - 60), str(y), "400")
        end = time.time() + 6
        while time.time() < end:
            found = nodes()
            if any(n.get("resource-id") == "toast-action" for n in found):
                return True
            rows = [n.get("resource-id") for n in found if n.get("resource-id", "").startswith("message-")]
            if before and rows and before not in rows:
                fail(f"archived {before}, but no Undo snackbar")
                return False
            time.sleep(0.5)
        print(f"swipe {attempt} did not archive, retrying", flush=True)
        row = settled("id:message-.*") or row
    fail(f"{tries} swipes never archived the first row")
    return False


booted()
apk = sorted(glob.glob(os.path.join(apk_dir, "*.apk")))[0]
adb("install", "-r", "-g", apk)
adb("shell", "settings", "put", "system", "font_scale", "1.0")
# The idle launcher behind the app sometimes stops responding on a loaded runner, and its
# dialog then covers the app every few seconds: take the launcher out and hide such dialogs.
adb("shell", "settings", "put", "global", "hide_error_dialogs", "1")
adb("shell", "pm", "disable-user", "--user", "0", "com.google.android.apps.nexuslauncher", check=False)
size = adb("shell", "wm", "size").split()[-1]
w, h = map(int, size.split("x"))

for mode in ("light", "dark"):
    adb("shell", "cmd", "uimode", "night", "yes" if mode == "dark" else "no")
    launch()
    shot(f"{mode}-inbox")

    # Swipe the first row right: Archive, with Undo. Swipes start mid-row: the screen's edges are the system's back gesture.
    row = settled("id:message-.*")
    if row:
        x1, y1, x2, y2 = row
        y = (y1 + y2) // 2
        mid = (x1 + x2) // 2
        quarter = (x2 - x1) // 4
        during(["swipe", str(mid - quarter), str(y), str(mid), str(y), "1500"], f"{mode}-swipe-archive", 1.0)
        settled("id:message-.*")
        if swipe_archive(row):
            shot(f"{mode}-archived-snackbar")
            tap("id:toast-action", 5)
        time.sleep(1)
        during(["swipe", str(mid + quarter), str(y), str(mid), str(y), "1500"], f"{mode}-swipe-delete", 1.0)
        time.sleep(1)

        # Long-press selects; a tap adds the next row; back ends it. A long-press the list takes for a
        # scroll must not fail the run: it is tried again.
        for attempt in range(1, 4):
            adb("shell", "input", "swipe", str(x1 + 200), str(y), str(x1 + 200), str(y), "900")
            time.sleep(0.5)
            adb("shell", "input", "tap", str(x1 + 200), str(y + (y2 - y1)))
            if find("id:end-selection", 4):
                break
            print(f"select {attempt} did not start a selection", flush=True)
            back()
        wait("id:end-selection", 5)
        shot(f"{mode}-selection")
        back()

        # Pull to refresh.
        during(["swipe", str(w // 2), str(y1), str(w // 2), str(y1 + h // 3), "1200"], f"{mode}-pull-refresh", 0.9)

    if tap("id:inbox-more"):
        shot(f"{mode}-inbox-menu")
        back()

    tap("id:message-.*")
    wait("id:reader")
    shot(f"{mode}-reader")
    adb("shell", "input", "swipe", str(w // 2), str(h * 7 // 10), str(w // 2), str(h * 3 // 10), "400")
    shot(f"{mode}-reader-scrolled")
    if tap("id:reader-more"):
        shot(f"{mode}-reader-menu")
        back()
    back()
    wait("id:messages")

    # A conversation: its latest message expanded, the earlier ones collapsed above; a tap expands one.
    if tap("Offsite in March: Lisbon or Porto?"):
        wait("id:thread-older")
        shot(f"{mode}-thread")
        if tap("id:thread-older"):
            wait("id:thread-newer")
            shot(f"{mode}-thread-older")
        back()
        wait("id:messages")

    # The FAB menu: New message, Add account.
    if tap("id:fab-menu"):
        wait("id:fab-add-account", 5)
        shot(f"{mode}-fab-menu")
    if tap("id:compose"):
        wait("id:compose-to")
        shot(f"{mode}-compose")
        # Schedule send from the top bar's clock: the presets' sheet, then the snackbar with Undo.
        scheduled = False
        if tap("id:compose-to"):
            adb("shell", "input", "text", "maya@northwind.dev")
            if tap("id:send-later") and find("Tomorrow morning", 5):
                shot(f"{mode}-schedule-menu")
                scheduled = tap("Tomorrow morning")
                if scheduled:
                    wait("id:toast-action", 5)
                    shot(f"{mode}-scheduled-snackbar")
        if not scheduled:
            back()
            back()
    wait("id:messages")

    # Scroll the inbox, switch tabs and back: the inbox keeps its place.
    adb("shell", "input", "swipe", str(w // 2), str(h * 7 // 10), str(w // 2), str(h * 3 // 10), "300")
    shot(f"{mode}-inbox-scrolled")

    if tap("Search"):
        time.sleep(1)
        shot(f"{mode}-search")
        adb("shell", "input", "text", "roadmap")
        wait("id:results")
        back()  # the keyboard
        shot(f"{mode}-search-results")
        # A mail laid out for 600px fits the phone's width.
        if tap("id:search"):
            adb("shell", "input", "keyevent", "123", *["67"] * 8)  # end, then delete "roadmap"
            adb("shell", "input", "text", "receipt")
            wait("id:results")
            back()
            # The first tap may only take the focus from the search field.
            for _ in range(2):
                if tap("Your receipt from Fastmail") and find("id:reader", 8):
                    shot(f"{mode}-reader-fit")
                    back()
                    break
            else:
                fail("the receipt never opened")
        # A search that finds nothing says so, and clears back to the hint.
        if tap("id:search"):
            adb("shell", "input", "keyevent", "123", *["67"] * 10)
            adb("shell", "input", "text", "zzzzqqq")
            wait("id:search-empty", 15)
            back()
            shot(f"{mode}-search-none")

    if tap("Mailboxes"):
        wait("id:mailbox-all")
        shot(f"{mode}-mailboxes")
        if tap("id:mailbox-scheduled"):
            wait("id:scheduled")
            shot(f"{mode}-scheduled")
            if tap("Mailboxes"):
                tap("id:mailbox-all")
        # An empty folder on the phone: the one draft is deleted, and Drafts says so with its next step.
        if tap("Mailboxes") and tap("id:mailbox-drafts"):
            wait("id:messages", 15)
            row = settled("id:message-.*")
            if row:
                x1, y1, x2, y2 = row
                adb("shell", "input", "swipe", str(x1 + 200), str((y1 + y2) // 2), str(x1 + 200), str((y1 + y2) // 2), "900")
                time.sleep(0.5)
                if tap("id:picked-delete"):
                    wait("id:empty-compose", 15)
                    shot(f"{mode}-empty-folder")
        if tap("Mailboxes") and tap("id:mailbox-all"):
            wait("id:messages", 15)

    # The notification inbox: the feed, a chip, a notification, its sources.
    if tap("Notifications"):
        wait("id:notify-list", 20)
        shot(f"{mode}-notifications")
        if tap("id:notify-more"):
            time.sleep(0.6)
            shot(f"{mode}-notifications-menu")
            if tap("Sources"):
                wait("id:sources", 15)
                shot(f"{mode}-sources")
                if tap("id:sources-add-top"):
                    wait("id:add-source", 10)
                    shot(f"{mode}-source-kinds")
                    back_to("id:sources")
                back_to("id:notifications")
            else:
                back()
        # A notification opens on its own page; Back returns to the feed.
        if tap("id:notify-[nm][0-9]+"):
            wait("id:notify-reader", 15)
            shot(f"{mode}-notification")
            back()
        wait("id:notify-list", 15)

    if tap("Settings"):
        wait("id:settings")
        shot(f"{mode}-settings")
        # The first account's signature, then a new message that carries it.
        if tap("id:account-more-0") and tap("Signature…") and tap("id:signature-text"):
            adb("shell", "input", "text", "'**Lin%sChen**'")
            adb("shell", "input", "keyevent", "66")
            adb("shell", "input", "text", "'Northwind%s-%s[northwind.dev](https://northwind.dev)'")
            back()  # the keyboard
            wait("id:signature-preview")
            shot(f"{mode}-signature")
            # The first tap may only take the focus from the field.
            for _ in range(2):
                if tap("id:signature-save") and find("id:settings", 8):
                    break
            else:
                fail("the signature page never closed")
        # The notification settings: the keys agents read the feed with, and the rules.
        if scroll_to("id:settings-keys"):
            if tap("id:settings-keys"):
                wait("id:api-keys", 15)
                shot(f"{mode}-api-keys")
                if tap("id:keys-add-top"):
                    wait("id:new-key", 10)
                    shot(f"{mode}-new-key")
                    back_to("id:api-keys")
                back_to("id:settings")
        if scroll_to("id:settings-rules"):
            if tap("id:settings-rules"):
                wait("id:notify-rules", 15)
                shot(f"{mode}-notify-rules")
                back_to("id:settings")

    if tap("Mail"):
        wait("id:messages")
        shot(f"{mode}-tab-back-to-mail")
        if tap("id:fab-menu") and tap("id:compose"):
            wait("id:compose-to")
            shot(f"{mode}-compose-signature")
            back()
            back()
            wait("id:messages")

    # The appearance changes while the app runs.
    other = "light" if mode == "dark" else "dark"
    adb("shell", "cmd", "uimode", "night", "yes" if other == "dark" else "no")
    time.sleep(3)
    shot(f"{mode}-then-{other}-live")
    adb("shell", "cmd", "uimode", "night", "yes" if mode == "dark" else "no")
    time.sleep(3)

    # Signing out shows the onboarding. The confirmation is a system dialog the emulator is slow to
    # draw, so the whole step is tried again rather than failing the run on its timing.
    if tap("Settings"):
        for attempt in range(1, 4):
            tap("id:sign-out")
            time.sleep(1)
            if attempt == 1:
                shot(f"{mode}-sign-out-dialog")
            # The dialog's own button: the one on Cancel's row, not the page's under the dialog.
            nodes_now = nodes()
            cancel = next((n for n in nodes_now if n.get("text", "") == "Cancel"), None)
            choices = [n for n in nodes_now if n.get("text", "").lower() == "sign out"]
            if choices:
                pick = min(choices, key=lambda n: abs(centre(n)[1] - centre(cancel)[1])) if cancel is not None else choices[-1]
                x, y = centre(pick)
                adb("shell", "input", "tap", str(x), str(y))
            if find("id:onboarding", 10):
                shot(f"{mode}-onboarding")
                break
            print(f"sign out {attempt} did not show the onboarding", flush=True)
        else:
            fail("signing out never showed the onboarding")

mode = "large"
# Larger text (Android's font size at 130%), then a tablet-sized screen (1280 x 800 dp).
adb("shell", "settings", "put", "system", "font_scale", "1.3")
launch()
shot("large-text-inbox")
tap("id:message-.*")
wait("id:reader")
shot("large-text-reader")
adb("shell", "settings", "put", "system", "font_scale", "1.0")
adb("shell", "wm", "size", "2560x1600")
adb("shell", "wm", "density", "320")
launch()
tap("id:message-.*")
shot("tablet")
adb("shell", "wm", "size", "reset")
adb("shell", "wm", "density", "reset")

adb("logcat", "-d", "-s", "nsh:*", "AndroidRuntime:E", check=False)
with open(os.path.join(out, "logcat.txt"), "w") as f:
    f.write(adb("logcat", "-d", check=False)[-400000:])

if failures:
    print("\n".join(failures))
    sys.exit(1)
