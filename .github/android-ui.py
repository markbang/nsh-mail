#!/usr/bin/env python3
"""Drives nsh-mail on a running emulator with adb and screenshots each screen,
in light and dark mode: inbox, swipe to archive and Undo, selection, pull to
refresh, reader, compose, search, mailboxes, settings, a tab switch that must
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
    return subprocess.run(["adb", *args], check=check, capture_output=True, text=True).stdout


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


def fail(what):
    failures.append(f"[{mode}] {what}")
    print("FAIL", what, flush=True)
    shot(f"{mode}-fail-{len(failures)}")


def back():
    adb("shell", "input", "keyevent", "4")
    time.sleep(0.8)


def launch():
    adb("shell", "am", "force-stop", PKG)
    adb("shell", "monkey", "-p", PKG, "-c", "android.intent.category.LAUNCHER", "1")


apk = sorted(glob.glob(os.path.join(apk_dir, "*.apk")))[0]
adb("install", "-r", "-g", apk)
adb("shell", "settings", "put", "system", "font_scale", "1.0")
size = adb("shell", "wm", "size").split()[-1]
w, h = map(int, size.split("x"))

for mode in ("light", "dark"):
    adb("shell", "cmd", "uimode", "night", "yes" if mode == "dark" else "no")
    launch()
    wait("id:messages", 60)
    shot(f"{mode}-inbox")

    # Swipe the first row right: Archive, with Undo. Swipes start mid-row: the screen's edges are the system's back gesture.
    row = bounds("id:message-.*")
    if row:
        x1, y1, x2, y2 = row
        y = (y1 + y2) // 2
        mid = (x1 + x2) // 2
        quarter = (x2 - x1) // 4
        during(["swipe", str(mid - quarter), str(y), str(mid), str(y), "1500"], f"{mode}-swipe-archive", 1.0)
        time.sleep(1)
        adb("shell", "input", "swipe", str(mid - quarter), str(y), str(x2 - 60), str(y), "150")
        wait("id:toast-action", 5)
        shot(f"{mode}-archived-snackbar")
        tap("id:toast-action", 5)
        time.sleep(1)
        during(["swipe", str(mid + quarter), str(y), str(mid), str(y), "1500"], f"{mode}-swipe-delete", 1.0)
        time.sleep(1)

        # Long-press selects; a tap adds the next row; back ends it.
        adb("shell", "input", "swipe", str(x1 + 200), str(y), str(x1 + 200), str(y), "900")
        time.sleep(0.5)
        adb("shell", "input", "tap", str(x1 + 200), str(y + (y2 - y1)))
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

    # The FAB menu: New message, Add account.
    if tap("id:fab-menu"):
        wait("id:fab-add-account", 5)
        shot(f"{mode}-fab-menu")
    if tap("id:compose"):
        wait("id:compose-to")
        shot(f"{mode}-compose")
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
            if tap("Your receipt from Fastmail"):
                wait("id:reader")
                shot(f"{mode}-reader-fit")
                back()

    if tap("Mailboxes"):
        wait("id:mailbox-all")
        shot(f"{mode}-mailboxes")

    if tap("Settings"):
        wait("id:settings")
        shot(f"{mode}-settings")

    if tap("Mail"):
        wait("id:messages")
        shot(f"{mode}-tab-back-to-mail")

    # The appearance changes while the app runs.
    other = "light" if mode == "dark" else "dark"
    adb("shell", "cmd", "uimode", "night", "yes" if other == "dark" else "no")
    time.sleep(3)
    shot(f"{mode}-then-{other}-live")
    adb("shell", "cmd", "uimode", "night", "yes" if mode == "dark" else "no")
    time.sleep(3)

    # Signing out shows the onboarding.
    if tap("Settings") and tap("id:sign-out"):
        time.sleep(1)
        shot(f"{mode}-sign-out-dialog")
        nodes_now = [n for n in nodes() if n.get("text", "").lower() == "sign out"]
        if nodes_now:
            x1, y1, x2, y2 = map(int, re.findall(r"\d+", nodes_now[-1].get("bounds")))
            adb("shell", "input", "tap", str((x1 + x2) // 2), str((y1 + y2) // 2))
        wait("id:onboarding")
        shot(f"{mode}-onboarding")

mode = "large"
# Larger text (Android's font size at 130%), then a tablet-sized screen (1280 x 800 dp).
adb("shell", "settings", "put", "system", "font_scale", "1.3")
launch()
wait("id:messages", 60)
shot("large-text-inbox")
tap("id:message-.*")
wait("id:reader")
shot("large-text-reader")
adb("shell", "settings", "put", "system", "font_scale", "1.0")
adb("shell", "wm", "size", "2560x1600")
adb("shell", "wm", "density", "320")
launch()
wait("id:messages", 60)
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
