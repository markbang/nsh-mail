#!/usr/bin/env bash
# Temporary (removed before merge): launches each host APK and keeps its screen and log.
set -u
pkg=top.bangwu.nshmail
out="$RUNNER_TEMP/smoke"
mkdir -p "$out"
for variant in stock ours; do
  echo "::group::$variant"
  adb install -r -g "$RUNNER_TEMP/$variant.apk"
  adb logcat -c
  adb shell am start -W -n "$pkg/.MainActivity"
  sleep 25
  echo "pid: $(adb shell pidof $pkg)"
  adb exec-out screencap -p > "$out/$variant.png"
  # A notification press as PushService builds it: the app's Notifications listener gets "message:<id>".
  adb shell am start -W -n "$pkg/.MainActivity" --es com.longbridge.nsh.NOTIFICATION message:smoke
  sleep 5
  echo "pid after press: $(adb shell pidof $pkg)"
  adb logcat -d > "$out/$variant.log"
  grep -E " (nsh|nsh-mail|AndroidRuntime) *:" "$out/$variant.log" | tail -40
  adb uninstall "$pkg"
  echo "::endgroup::"
done
