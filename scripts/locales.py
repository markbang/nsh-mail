#!/usr/bin/env python3
"""Checks the app's copy: one value per language, in both languages, and no stale key.

Every key in app/locales/app.yml has an English and a Chinese (`zh-CN`) value; the
Chinese differs from the English except for names and placeholders; every key the
code asks for exists, and every key the file defines is asked for by the code (a
`t("…")` or a `t(`prefix.${…}`)` family).

Usage: scripts/locales.py [-v]   (exit 1 when a check fails)
"""
import re, sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
app = root / "app"
LOCALES = {"en", "zh-CN"}
# Names, formats and placeholders that read the same in both languages.
SAME_ALLOWED = {"provider.", "imap.tls", "imap.starttls", "compose.uploading", "settings.about"}

def parse(path):
    keys, current = {}, None
    for number, line in enumerate(path.read_text().splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        name = re.fullmatch(r"([A-Za-z0-9_.-]+):\s*", line)
        if name:
            current = name.group(1)
            keys[current] = {}
            continue
        value = re.fullmatch(r"\s+([A-Za-z-]+):\s*(.*)", line)
        if value and current:
            text = value.group(2).strip()
            if len(text) >= 2 and text[0] == text[-1] and text[0] in "\"'":
                text = text[1:-1]
            keys[current][value.group(1)] = text
            continue
        if current is not None:
            raise SystemExit(f"{path.name}:{number}: not a key or a language: {line!r}")
    return keys

def uses():
    """The keys and key families the app's code asks for."""
    static, families = set(), set()
    for path in list(app.rglob("*.ts")) + list(app.rglob("*.tsx")):
        if "node_modules" in path.parts or ".nsh" in path.parts:
            continue
        source = path.read_text()
        static.update(re.findall(r"""\bt\(\s*["']([^"'\n$]+)["']""", source))
        for template in re.findall(r"\bt\(\s*`([^`]*)`", source):
            prefix = template.split("${")[0]
            if "${" in template:
                families.add(prefix)
    return static, families

def main():
    keys = parse(app / "locales/app.yml")
    static, families = uses()
    failed = []
    for key, values in keys.items():
        missing = LOCALES - set(values)
        if missing:
            failed.append(f"{key}: no {' or '.join(sorted(missing))} value")
        elif not all(values[lang] for lang in LOCALES):
            failed.append(f"{key}: an empty value")
        elif values["en"] == values["zh-CN"] and not key.startswith(tuple(SAME_ALLOWED)):
            failed.append(f"{key}: the Chinese is the English: {values['en']!r}")
    for key in sorted(static):
        if key not in keys:
            failed.append(f"{key}: used by the code, missing here")
    for key in keys:
        if key in static or any(key.startswith(prefix) for prefix in families):
            continue
        failed.append(f"{key}: defined, but nothing asks for it")
    if "-v" in sys.argv:
        for key in sorted(keys):
            print(f"{key}: {keys[key].get('en')!r} / {keys[key].get('zh-CN')!r}")
    print("locales:", "all checks pass" if not failed else f"{len(failed)} problem(s)")
    for line in failed:
        print("  " + line)
    sys.exit(1 if failed else 0)

main()
