#!/usr/bin/env python3
"""Drives nsh-mail on a running emulator with adb and screenshots each screen,
in light and dark mode: inbox, reader, compose, search, mailboxes, settings,
and a tab switch that must keep the inbox's scroll position.

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


def find(key, timeout=20):
    end = time.time() + timeout
    while time.time() < end:
        for n in nodes():
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

    tap("id:message-.*")
    wait("id:reader")
    shot(f"{mode}-reader")
    back()
    wait("id:messages")

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
    adb("shell", "cmd", "uimode", "night", "no" if mode == "dark" else "yes")
    shot(f"{mode}-then-{'light' if mode == 'dark' else 'dark'}-live")

adb("logcat", "-d", "-s", "nsh:*", "AndroidRuntime:E", check=False)
with open(os.path.join(out, "logcat.txt"), "w") as f:
    f.write(adb("logcat", "-d", check=False)[-400000:])

if failures:
    print("\n".join(failures))
    sys.exit(1)
