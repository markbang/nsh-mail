#!/usr/bin/env bash
# Puts in place what the Gradle build embeds (both git-ignored):
#   app/libs/nsh-mail.aar        nsh.aar of the CLI's NSH release, its libnsh.so replaced by the one
#                                `nsh package` builds with nsh-mail's crate (the `nsh-mail/native` module)
#   app/src/main/assets/nsh/     the app's bundle
# Needs nsh, the Android SDK and NDK, and the Rust Android targets, as `nsh package --target android` does.
# Arguments go to that `nsh package`, such as `--keystore release.jks --key-alias upload` for a release-signed APK.
set -euo pipefail

android=$(cd "$(dirname "$0")" && pwd)
app="$android/../app"
abis=${NSH_ABIS:-arm64-v8a,armeabi-v7a}
version="v$(nsh --version | awk '{print $2}')"

# The Java runtime must match libnsh.so's: take it from the same release as the CLI.
embed="$android/build/nsh-embed-$version"
if [ ! -f "$embed/nsh.aar" ]; then
  mkdir -p "$embed"
  url="https://github.com/longbridge/nsh/releases/download/$version/nsh-embed-android.zip"
  curl -fsSL -o "$embed/embed.zip" "$url"
  expected=$(curl -fsSL "$url.sha256" | awk '{print $1}')
  echo "$expected  $embed/embed.zip" | sha256sum -c --quiet - || {
    echo "nsh-embed-android.zip of $version does not match its checksum" >&2
    exit 1
  }
  unzip -j -o -q "$embed/embed.zip" '*/nsh.aar' -d "$embed"
fi

(cd "$app" && nsh package App.tsx --target android --abi "$abis" "$@")

work="$android/build/nsh-mail-aar"
rm -rf "$work" && mkdir -p "$work"
(cd "$work" && unzip -q "$embed/nsh.aar" && rm -rf jni)
IFS=, read -ra list <<< "$abis"
for abi in "${list[@]}"; do
  mkdir -p "$work/jni/$abi"
  cp "$app/.nsh/rust/android-release/lib/$abi/libnsh.so" "$work/jni/$abi/"
done
mkdir -p "$android/app/libs"
rm -f "$android/app/libs/nsh-mail.aar"
(cd "$work" && zip -q -r "$android/app/libs/nsh-mail.aar" .)

rm -rf "$android/app/src/main/assets/nsh"
nsh build "$app/App.tsx" --release --out "$android/app/src/main/assets/nsh"
