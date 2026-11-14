#!/usr/bin/env python3
"""Drives nsh-mail on a running emulator with adb and screenshots each screen,
in light and dark mode: inbox, swipe to archive and Undo, selection, pull to
refresh, reader, a conversation, compose, Schedule send, search, mailboxes, Scheduled, settings, a signature, a tab switch that must
keep the inbox's scroll position; then larger text and a tablet-sized screen.

Usage: android-ui.py <dir with the APK> <screenshot dir>
Every view is found by its testID (the accessibility resource id) or its text, and every lookup waits
for it: nothing is tapped or typed into before the screen shows it. A failure leaves its screenshot
and the dump beside it in the screenshot dir, which the job uploads.
"""
import glob, os, re, subprocess, sys, time, traceback, xml.etree.ElementTree as ET

PKG = "top.bangwu.nshmail"
apk_dir, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)
failures = []
mode = "start"
dump_xml = ""


def adb(*args, check=True):
    return subprocess.run(["adb", *args], check=check, capture_output=True, encoding="utf-8", errors="replace").stdout


def frame():
    """The screen as PNG bytes."""
    return subprocess.run(["adb", "exec-out", "screencap", "-p"], check=True, capture_output=True).stdout


def screenshot(name):
    with open(os.path.join(out, f"{name}.png"), "wb") as f:
        f.write(frame())


def nodes():
    """The screen's nodes, re-dumped until uiautomator answers; the last dump is kept for evidence."""
    global dump_xml
    for _ in range(3):
        adb("shell", "uiautomator", "dump", "/sdcard/ui.xml", check=False)
        xml = adb("exec-out", "cat", "/sdcard/ui.xml", check=False)
        if xml.strip().startswith("<?xml"):
            dump_xml = xml
            return list(ET.fromstring(xml).iter("node"))
        time.sleep(1)
    return []


def matches(n, key):
    rid, text, desc = n.get("resource-id", ""), n.get("text", ""), n.get("content-desc", "")
    if key.startswith("id:"):
        return re.fullmatch(key[3:], rid) is not None
    return key in (text, desc)


def box(n):
    """The node's bounds, or None while it has no area (laid out, not painted)."""
    x1, y1, x2, y2 = map(int, re.findall(r"\d+", n.get("bounds")))
    return (x1, y1, x2, y2) if x2 > x1 and y2 > y1 else None


def find(key, timeout=20):
    """`key`'s node once it is on screen: every lookup waits to its deadline, none assumes a paint."""
    end = time.time() + timeout
    while time.time() < end:
        found = nodes()
        if dismiss_anr(found):
            continue
        for n in found:
            if matches(n, key) and box(n):
                return n
        time.sleep(1)
    return None


def gone(key, timeout=15):
    """True once `key` has left the screen (a snackbar, a dialog)."""
    end = time.time() + timeout
    while time.time() < end:
        if not any(matches(n, key) and box(n) for n in nodes()):
            return True
        time.sleep(1)
    return False


def centre(n):
    """The middle of a uiautomator node."""
    x1, y1, x2, y2 = box(n)
    return (x1 + x2) // 2, (y1 + y2) // 2


def tap(key, timeout=20):
    n = find(key, timeout)
    if n is None:
        fail(f"not found: {key}")
        return False
    x, y = centre(n)
    adb("shell", "input", "tap", str(x), str(y))
    return True


def wait(key, timeout=30):
    if find(key, timeout) is None:
        fail(f"never showed: {key}")
        return False
    return True


def text_of(key, timeout=20):
    n = find(key, timeout)
    return (n.get("text") or n.get("content-desc")) if n is not None else None


def settled(key, tries=10):
    """`key`'s box once it stops moving: a cold start on a slow emulator is still sliding in."""
    n = find(key)
    last = box(n) if n is not None else None
    for _ in range(tries):
        time.sleep(1)
        n = find(key)
        now = box(n) if n is not None else None
        if now == last:
            return now
        last = now
    return last


def quiet(timeout=8):
    """Until the screen stops changing: a page sliding in is not painted yet."""
    end, last = time.time() + timeout, None
    while time.time() < end:
        nodes()
        if dump_xml == last:
            return
        last = dump_xml


def shot(name):
    quiet()
    screenshot(name)
    print("shot", name, flush=True)


def steady(before=b"", timeout=12):
    """Until two screenshots in a row are the same, and not `before`: a fade or a theme swap is a
    pixel change the dump cannot see. Returns the frame that settled."""
    end, last = time.time() + timeout, before
    while time.time() < end:
        time.sleep(0.3)
        now = frame()
        if now and now != before and now == last:
            return now
        last = now
    print("the screen never settled, going on", flush=True)
    return last


def repainted(name, before):
    """A screenshot once the theme swap under way has been drawn; the frame it saved."""
    settled = steady(before)
    with open(os.path.join(out, f"{name}.png"), "wb") as f:
        f.write(settled)
    print("shot", name, flush=True)
    return settled


def sign_out(name, tries=2):
    """Settings' Sign out answered, then the onboarding: the dialog's title in the dump and its fade
    over before its positive button (Android's last action) is tapped. A tap the emulator drops during
    that fade only dismisses the dialog, and an app wedged under it answers nothing: each attempt
    starts from Settings, the second from a restarted app."""
    for attempt in range(1, tries + 1):
        if attempt > 1:
            print("signing out again, from a restarted app", flush=True)
            launch()
        if not (tap("Settings") and tap("id:sign-out")):
            return False
        if not wait("Sign out of nsh-mail?", 15):
            return False
        if attempt == 1:
            shot(name)
        steady()
        tap("id:android:id/button1")
        if wait("id:onboarding", 30):
            return True
        print(f"sign out {attempt} did not show the onboarding", flush=True)
    fail("signing out never showed the onboarding")
    return False


def during(gesture, name, at=0.7):
    """Screenshots `name` while `gesture` (an `adb shell input` command) is under way."""
    p = subprocess.Popen(["adb", "shell", "input", *gesture])
    time.sleep(at)
    screenshot(name)
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


def changed(key, before, timeout=10):
    """True once `key` says something other than `before` (the selection's count, say)."""
    end = time.time() + timeout
    while time.time() < end:
        if text_of(key, 3) not in (None, before):
            return True
        time.sleep(1)
    return False


def evidence(name):
    """The screen and its dump under `name`: what the next flake is diagnosed from."""
    try:
        quiet(3)
        screenshot(name)
        nodes()
        with open(os.path.join(out, f"{name}.xml"), "w") as f:
            f.write(dump_xml)
        print("evidence", name, flush=True)
        # What the app itself said, where a failure is read.
        print("\n".join(adb("logcat", "-d", "-s", "nsh:*", "AndroidRuntime:E", check=False).splitlines()[-25:]), flush=True)
    except Exception as e:  # it must never hide the failure it explains
        print("no evidence for", name, "-", e, flush=True)


def fail(what):
    failures.append(f"[{mode}] {what}")
    print("FAIL", what, flush=True)
    evidence(f"{mode}-fail-{len(failures)}")


def back(timeout=4):
    """Android's back, then wait for what it changed."""
    adb("shell", "input", "keyevent", "4")
    quiet(timeout)


def long_press(x, y, tries=2):
    """A hold until a selection shows: under load the list takes a short hold for a scroll."""
    for hold in ("900", "1400")[:tries]:
        adb("shell", "input", "swipe", str(x), str(y), str(x), str(y), hold)
        if find("id:end-selection", 5) is not None:
            return True
        print("the long-press did not select, holding longer", flush=True)
    fail("the long-press never started a selection")
    return False


def uncaught(kind, value, tb):
    """A failure nobody expected still leaves the screen behind it."""
    evidence(f"{mode}-crash")
    traceback.print_exception(kind, value, tb)


sys.excepthook = uncaught


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
            screenshot(f"{mode}-archived-snackbar")  # the snackbar lives seconds: no waiting for it
            print("shot", f"{mode}-archived-snackbar", flush=True)
            if tap("id:toast-action", 5):
                gone("id:toast-action")  # Undo: the row is back once its snackbar has gone
        during(["swipe", str(mid + quarter), str(y), str(mid), str(y), "1500"], f"{mode}-swipe-delete", 1.0)
        gone("id:toast-action")  # the deletion's Undo leaves before the next gesture

        # Long-press selects; a tap adds the row below; back ends it. The selection has to show before
        # that tap: a long-press taken for a scroll would leave the tap to open the message instead.
        if long_press(x1 + 200, y):
            count = text_of("id:selection-count", 5)
            adb("shell", "input", "tap", str(x1 + 200), str(y + (y2 - y1)))
            if not changed("id:selection-count", count, 8):
                fail(f"the tap below the selection did not add a row: {text_of('id:selection-count', 3)}")
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
            if tap("id:send-later") and find("Tomorrow morning", 5) is not None:
                shot(f"{mode}-schedule-menu")
                scheduled = tap("Tomorrow morning")
                if scheduled:
                    wait("id:toast-action", 5)
                    screenshot(f"{mode}-scheduled-snackbar")
                    print("shot", f"{mode}-scheduled-snackbar", flush=True)
        if not scheduled:
            back()
            back()
    wait("id:messages")

    # Scroll the inbox, switch tabs and back: the inbox keeps its place.
    adb("shell", "input", "swipe", str(w // 2), str(h * 7 // 10), str(w // 2), str(h * 3 // 10), "300")
    shot(f"{mode}-inbox-scrolled")

    if tap("Search"):
        wait("id:search")
        shot(f"{mode}-search")
        tap("id:search")  # the field, before anything is typed
        adb("shell", "input", "text", "roadmap")
        wait("id:result-count", 15)
        back()  # the keyboard
        shot(f"{mode}-search-results")
        # A mail laid out for 600px fits the phone's width.
        if tap("id:search"):
            adb("shell", "input", "keyevent", "123", *["67"] * 8)  # end, then delete "roadmap"
            adb("shell", "input", "text", "receipt")
            wait("Your receipt from Fastmail", 15)
            back()
            # The first tap may only take the focus from the search field.
            for _ in range(2):
                if tap("Your receipt from Fastmail") and find("id:reader", 8) is not None:
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
                if long_press(x1 + 200, (y1 + y2) // 2) and tap("id:picked-delete"):
                    wait("id:empty-compose", 15)
                    shot(f"{mode}-empty-folder")
        if tap("Mailboxes") and tap("id:mailbox-all"):
            wait("id:messages", 15)

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
                if tap("id:signature-save") and find("id:settings", 8) is not None:
                    break
            else:
                fail("the signature page never closed")

    if tap("Mail"):
        wait("id:messages")
        shot(f"{mode}-tab-back-to-mail")
        if tap("id:fab-menu") and tap("id:compose"):
            wait("id:compose-to")
            shot(f"{mode}-compose-signature")
            back()
            back()
            wait("id:messages")

    # The appearance changes while the app runs: a colour change, so this waits on the pixels.
    other = "light" if mode == "dark" else "dark"
    before = frame()
    adb("shell", "cmd", "uimode", "night", "yes" if other == "dark" else "no")
    swapped = repainted(f"{mode}-then-{other}-live", before)
    adb("shell", "cmd", "uimode", "night", "yes" if mode == "dark" else "no")
    steady(swapped)  # this mode's appearance again, before the screen is touched

    # Signing out shows the onboarding.
    if sign_out(f"{mode}-sign-out-dialog"):
        # The first screen of the onboarding, painted: where mail comes from, and its way on.
        wait("id:mode-local", 15)
        wait("id:onboarding-go", 15)
        shot(f"{mode}-onboarding")
        # The cloud form: the server address and the sign-in/sign-up choice are painted before
        # anything is typed into it.
        if tap("id:mode-cloud"):
            wait("id:server", 15)
            wait("id:cloud-email", 15)
            wait("Create account", 15)
            shot(f"{mode}-onboarding-cloud")

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
wait("id:reader")
shot("tablet")
adb("shell", "wm", "size", "reset")
adb("shell", "wm", "density", "reset")

adb("logcat", "-d", "-s", "nsh:*", "AndroidRuntime:E", check=False)
with open(os.path.join(out, "logcat.txt"), "w") as f:
    f.write(adb("logcat", "-d", check=False)[-400000:])

if failures:
    print("\n".join(failures))
    sys.exit(1)
