#!/usr/bin/env python3
"""Measures scrolling on the emulator: frame times from `gfxinfo` while the inbox of the large demo
mailbox (`--cfg nshmail_demo_large`: 10,000 messages, 2,000 conversations) is flung down.

Usage: android-perf.py <dir with the APK> [swipes]

Prints one line per run with the frames drawn, their p50/p90/p99 and the share of janky ones (over
16.7 ms), plus the widest frame, which is where a sync or a page stall shows.
"""
import os
import re
import statistics
import subprocess
import sys
import time

PKG = "top.bangwu.nshmail"
apk_dir, swipes = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 24
apk = os.path.join(apk_dir, "nsh-mail.apk")


def adb(*args, check=True):
    return subprocess.run(["adb", *args], check=check, capture_output=True, encoding="utf-8", errors="replace").stdout


def prop(name):
    return adb("shell", "getprop", name, check=False).strip()


def booted(timeout=240):
    adb("wait-for-device")
    end = time.time() + timeout
    while time.time() < end:
        if prop("sys.boot_completed") == "1" and prop("sys.user.0.ce_available") == "true":
            break
        time.sleep(2)
    # The system compiles and indexes right after boot: wait (bounded) for the load to drop.
    end = time.time() + 120
    cpus = int(prop("nproc") or 2)
    while time.time() < end:
        try:
            load = float(adb("shell", "cat", "/proc/loadavg", check=False).split()[0])
        except (IndexError, ValueError):
            break
        if load < cpus:
            break
        time.sleep(5)


def focused():
    for line in adb("shell", "dumpsys", "window", check=False).splitlines():
        if "mCurrentFocus" in line:
            return line.strip()
    return ""


def frames():
    """The frame intervals gfxinfo recorded since the last reset, in ms (one per frame drawn)."""
    out = adb("shell", "dumpsys", "gfxinfo", PKG, "framestats", check=False)
    head = out.split("---PROFILEDATA---")[1] if "---PROFILEDATA---" in out else ""
    rows = [line.split(",") for line in head.splitlines() if line.count(",") > 10]
    if not rows:
        return []
    header, data = rows[0], rows[1:]
    columns = {name: at for at, name in enumerate(header)}
    done, started = columns.get("FrameCompleted"), columns.get("IntendedVsync")
    if done is None or started is None:
        return []
    times = []
    for row in data:
        try:
            times.append((int(row[done]) - int(row[started])) / 1e6)
        except (IndexError, ValueError):
            continue
    return [t for t in times if t > 0]


def summary(label, times):
    if not times:
        print(f"{label}: no frames recorded", flush=True)
        return
    times = sorted(times)
    at = lambda q: times[min(len(times) - 1, int(q * len(times)))]
    jank = sum(1 for t in times if t > 16.7) / len(times) * 100
    print(
        f"{label}: {len(times)} frames, p50 {statistics.median(times):.1f} ms, "
        f"p90 {at(0.9):.1f}, p99 {at(0.99):.1f}, max {times[-1]:.1f}, over 16.7 ms {jank:.0f}%",
        flush=True,
    )


def listed(timeout=240):
    """Until the inbox's rows are on screen: a cold start on a slow emulator takes a while."""
    import xml.etree.ElementTree as ET

    end = time.time() + timeout
    while time.time() < end:
        adb("shell", "uiautomator", "dump", "/sdcard/ui.xml", check=False)
        xml = adb("exec-out", "cat", "/sdcard/ui.xml", check=False)
        if xml.strip().startswith("<?xml"):
            ids = [n.get("resource-id", "") for n in ET.fromstring(xml).iter("node")]
            if any(re.fullmatch(r"message-.*", rid) for rid in ids):
                return True
        time.sleep(2)
    return False


def diagnose():
    """Why no rows: what the app logged, and what is on screen."""
    for line in adb("logcat", "-d", "-t", "400", check=False).splitlines():
        if re.search(r"FATAL|AndroidRuntime|Exception|nsh|NSH|panic", line):
            print("logcat:", line[:300], flush=True)
    adb("shell", "uiautomator", "dump", "/sdcard/ui.xml", check=False)
    print(adb("exec-out", "cat", "/sdcard/ui.xml", check=False)[:4000], flush=True)


booted()
adb("install", "-r", apk)
adb("shell", "am", "force-stop", PKG)
adb("shell", "monkey", "-p", PKG, "-c", "android.intent.category.LAUNCHER", "1")
end = time.time() + 120
while PKG not in focused() and time.time() < end:
    time.sleep(1)
if not listed():
    print("the inbox did not show its rows: nothing measured", flush=True)
    diagnose()
    sys.exit(1)
time.sleep(3)

width, height = 1080, 2400
size = re.findall(r"\d+", adb("shell", "wm", "size", check=False))
if len(size) >= 2:
    width, height = int(size[0]), int(size[1])
x, top, bottom = width // 2, int(height * 0.30), int(height * 0.80)

adb("shell", "dumpsys", "gfxinfo", PKG, "reset", check=False)
time.sleep(1)
for _ in range(swipes):
    adb("shell", "input", "swipe", str(x), str(bottom), str(x), str(top), "250")
    time.sleep(0.15)
time.sleep(2)
summary(f"scroll, {swipes} flings", frames())
