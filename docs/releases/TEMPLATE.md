English | [简体中文](https://github.com/markbang/nsh-mail/releases/download/vX.Y.Z/RELEASE_NOTES.zh-CN.md)

<!-- Copy to docs/releases/X.Y.Z.md (and TEMPLATE.zh-CN.md to X.Y.Z.zh-CN.md), replace X.Y.Z, bump "build" in app/nsh.json, then push the tag vX.Y.Z. -->

## What's in X.Y.Z

- 

## Downloads

| Platform | File | Notes |
|---|---|---|
| Android 8+ | `nsh-mail-X.Y.Z-arm64-v8a.apk`, `-armeabi-v7a.apk`, `-x86_64.apk`, `-universal.apk` | Release-signed. |
| Android (Google Play) | `nsh-mail-X.Y.Z.aab` | App bundle with arm64-v8a and armeabi-v7a. |
| Linux x86_64 | `nsh-mail-X.Y.Z-linux-x86_64.tar.gz` | Needs GTK 4.14+, and WebKitGTK 6.0 for HTML mail. |
| Windows x64 | `nsh-mail-X.Y.Z-windows-x64-setup.exe`, `nsh-mail-X.Y.Z-windows-x64.zip` | Unsigned: SmartScreen warns on first run (More info → Run anyway). |
| Server | `ghcr.io/markbang/nsh-mail` | Docker image of the cloud-mode server with the web client. |

Which APK? Most phones: arm64-v8a. Old 32-bit phones: armeabi-v7a. Emulators and Chromebooks: x86_64. Not sure: universal (larger). Google Play or enterprise distribution: the AAB.

`SHA256SUMS.txt` lists the checksums of every file.

Android signing certificate SHA-256: `F3:92:9A:E4:A1:E2:AF:07:03:AA:0C:E7:AC:E2:B5:6E:E9:9F:1E:B1:0B:B3:A4:1F:C8:11:07:DE:E4:6B:4B:42`
