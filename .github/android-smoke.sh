#!/usr/bin/env bash
# Temporary (removed before merge): launches each host APK and keeps its screens and log.
set -u
pkg=top.bangwu.nshmail
out="$RUNNER_TEMP/smoke"
mkdir -p "$out"
logs() { grep -E " (nsh|nsh-mail|AndroidRuntime) *:" "$1" | cut -c1-240 | tail -30; }

for variant in stock ours; do
  echo "::group::$variant: first start"
  adb install -r -g "$RUNNER_TEMP/$variant.apk"
  adb logcat -c
  adb shell am start -W -n "$pkg/.MainActivity"
  sleep 25
  echo "pid: $(adb shell pidof $pkg)"
  adb exec-out screencap -p > "$out/$variant.png"
  adb logcat -d > "$out/$variant.log"
  logs "$out/$variant.log"
  adb uninstall "$pkg"
  echo "::endgroup::"
done

# A notification press as PushService builds it ("message:<id>" in nsh's launch extra), in local mode (demo mail),
# with the Gradle host and, for comparison, with nsh's own packaged APK.
press() {
  local name=$1 apk=$2 activity=$3
  echo "::group::$name: notification presses"
  adb install -r -g "$apk"
  adb shell "printf '%s' '{\"nshmail.mode\":\"local\"}' | run-as $pkg sh -c 'mkdir -p files/nsh/$pkg && cat > files/nsh/$pkg/storage.json'"
  adb logcat -c
  adb shell am start -W -n "$pkg/$activity"
  sleep 15
  adb exec-out screencap -p > "$out/$name-inbox.png"
  adb shell am start -W -n "$pkg/$activity" --es com.longbridge.nsh.NOTIFICATION message:m1
  sleep 8
  adb exec-out screencap -p > "$out/$name-warm-press.png"
  adb shell am force-stop "$pkg"
  adb shell am start -W -n "$pkg/$activity" --es com.longbridge.nsh.NOTIFICATION message:m2
  sleep 15
  adb exec-out screencap -p > "$out/$name-cold-press.png"
  adb logcat -d > "$out/$name-press.log"
  logs "$out/$name-press.log"
  adb uninstall "$pkg"
  echo "::endgroup::"
}
press gradle "$RUNNER_TEMP/ours.apk" .MainActivity
press nsh "$RUNNER_TEMP/nsh.apk" com.longbridge.nsh.AppActivity
