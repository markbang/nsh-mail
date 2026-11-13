#!/usr/bin/env python3
"""WCAG contrast of the text/icon token pairs the app draws, in both appearances.

Usage: scripts/contrast.py [-v] [app/theme.json]   (exit 1 when a pair fails; -v lists every pair)
"""
import json, sys
from pathlib import Path

paths = [a for a in sys.argv[1:] if a != "-v"]
theme = json.loads(Path(paths[0] if paths else Path(__file__).parent.parent / "app/theme.json").read_text())["cssVars"]

def rgba(hex_):
    h = hex_.lstrip("#")
    if len(h) == 6:
        h += "ff"
    return [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4, 6)]

def over(top, bottom):
    a = top[3]
    return [top[i] * a + bottom[i] * (1 - a) for i in range(3)] + [1]

def lum(c):
    f = lambda v: v / 12.92 if v <= 0.03928 else ((v + 0.055) / 1.055) ** 2.4
    return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2])

def ratio(a, b):
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)

SURFACES = ["background", "content1", "content2", "sidebar", "card", "popover", "default-100"]
# (ink, minimum): body text needs 4.5, icons and large text 3.
ON_SURFACES = [("foreground", 4.5), ("foreground-500", 4.5), ("foreground-600", 4.5), ("default-500", 4.5),
               ("default-foreground", 4.5), ("primary", 4.5), ("danger", 4.5), ("muted-foreground", 4.5),
               ("unread", 3), ("flag", 3)]
SOLID = ["default", "primary", "secondary", "success", "warning", "danger", "badge"]
# The flat variant: ink on its color at 20% (default at 40%) over the page.
FLAT = {"default": ("default-700", 0.4), "primary": ("primary-600", 0.2), "danger": ("danger-600", 0.2),
        "success": ("success-700", 0.2), "warning": ("warning-700", 0.2)}
FLAT_DARK = {"success": "success", "warning": "warning", "danger": "danger-500"}

# Pairs on their own surface: the phone's snackbar (an inverse surface) and its action.
EXTRA = [("background", "foreground-800", 4.5), ("primary-300", "foreground-800", 4.5)]

failed = 0
def check(mode, label, fg, bg, need):
    global failed
    r = ratio(fg, bg)
    ok = r >= need
    failed += not ok
    if not ok or "-v" in sys.argv:
        print(f"{'ok  ' if ok else 'FAIL'} {mode:5} {label:48} {r:5.2f} (needs {need})")

for mode in ("light", "dark"):
    t = {**theme.get("theme", {}), **theme[mode]}
    c = lambda k: rgba(t[k])
    for s in SURFACES:
        base = c(s)
        for ink, need in ON_SURFACES:
            if ink in t:
                check(mode, f"{ink} on {s}", over(c(ink), base), base, need)
    for k in SOLID:
        if k in t:
            check(mode, f"{k}-foreground on {k}", c(f"{k}-foreground"), c(k), 4.5)
    for ink, bg, need in EXTRA:
        check(mode, f"{ink} on {bg}", c(ink), c(bg), need)
    for k, (ink, alpha) in FLAT.items():
        ink = FLAT_DARK.get(k, ink) if mode == "dark" else ink
        frame = c(k)
        frame[3] = alpha
        bg = over(frame, c("background"))
        check(mode, f"{ink} on {k}/{int(alpha * 100)} (flat)", c(ink), bg, 4.5)

print("contrast:", "all pairs pass" if not failed else f"{failed} pair(s) fail")
sys.exit(1 if failed else 0)
