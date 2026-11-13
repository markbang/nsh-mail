#!/usr/bin/env python3
"""WCAG contrast of the M3 color-role pairs the app draws, in both appearances.

Usage: scripts/contrast.py [app/theme.json] [-v]   (exit 1 when a pair fails)
"""
import json, sys
from pathlib import Path

args = [a for a in sys.argv[1:] if not a.startswith("-")]
theme = json.loads(Path(args[0] if args else Path(__file__).parent.parent / "app/theme.json").read_text())["cssVars"]

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

# on-X over X: text, 4.5.
PAIRS = [(f"on-{r}", r) for r in ("primary", "primary-container", "secondary", "secondary-container", "tertiary",
                                  "tertiary-container", "error", "error-container", "surface", "surface-variant",
                                  "background")]
PAIRS += [("on-primary-fixed", "primary-fixed"), ("on-primary-fixed-variant", "primary-fixed"),
          ("on-secondary-fixed", "secondary-fixed"), ("on-secondary-fixed-variant", "secondary-fixed"),
          ("on-tertiary-fixed", "tertiary-fixed"), ("on-tertiary-fixed-variant", "tertiary-fixed"),
          ("inverse-on-surface", "inverse-surface")]
SURFACES = ["surface", "surface-dim", "surface-bright", "surface-container-lowest", "surface-container-low",
            "surface-container", "surface-container-high", "surface-container-highest"]
# Ink drawn on any surface: (ink, minimum). Text needs 4.5; icons, outlines and marks 3.
ON_SURFACES = [("on-surface", 4.5), ("on-surface-variant", 4.5), ("primary", 4.5), ("error", 4.5),
               ("outline", 3), ("unread", 3), ("flag", 3)] + [(f"chart-{i}", 3) for i in range(1, 6)]

failed = 0
def check(mode, label, fg, bg, need):
    global failed
    r = ratio(fg, bg)
    ok = r >= need
    failed += not ok
    if not ok or "-v" in sys.argv:
        print(f"{'ok  ' if ok else 'FAIL'} {mode:5} {label:52} {r:5.2f} (needs {need})")

for mode in ("light", "dark"):
    t = {**theme.get("theme", {}), **theme[mode]}
    c = lambda k: rgba(t[k])
    for ink, bg in PAIRS:
        check(mode, f"{ink} on {bg}", c(ink), c(bg), 4.5)
    check(mode, "inverse-primary on inverse-surface", c("inverse-primary"), c("inverse-surface"), 3)
    for s in SURFACES:
        for ink, need in ON_SURFACES:
            check(mode, f"{ink} on {s}", over(c(ink), c(s)), c(s), need)

print("contrast:", "all pairs pass" if not failed else f"{failed} pair(s) fail")
sys.exit(1 if failed else 0)
