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
  echo "::endgroup::"
done

echo "::group::ours, local mode: a press opens its message"
adb shell am force-stop "$pkg"
adb shell "printf '%s' '{\"nshmail.mode\":\"local\"}' | run-as $pkg sh -c 'mkdir -p files/nsh/$pkg && cat > files/nsh/$pkg/storage.json'"
adb shell am start -W -n "$pkg/.MainActivity"
sleep 15
adb exec-out screencap -p > "$out/inbox.png"
adb logcat -c
adb shell am start -W -n "$pkg/.MainActivity" --es com.longbridge.nsh.NOTIFICATION message:m1
sleep 8
adb exec-out screencap -p > "$out/pressed.png"
adb logcat -d > "$out/pressed.log"
grep -E " (nsh|nsh-mail|AndroidRuntime) *:" "$out/pressed.log" | tail -20
echo "::endgroup::"
