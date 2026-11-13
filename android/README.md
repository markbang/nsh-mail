English | [简体中文](README.zh-CN.md)

# Android host (FCM push)

`nsh package --target android` builds APKs without Gradle, so they cannot carry Firebase. This Gradle project runs the same app in an `NshView` and adds Firebase Cloud Messaging, so cloud mode can push new mail while the app is closed. Design: [docs/design.md](../docs/design.md) §5.

## Build

Needs nsh, JDK 17, the Android SDK (`platforms;android-35`, build tools, an NDK) and the Rust target `aarch64-linux-android` (`armv7-linux-androideabi` too for a release).

```sh
android/prepare.sh                         # nsh.aar + libnsh.so with the app's crate, and the bundle
                                           # NSH_ABIS=x86_64 for an emulator (default arm64-v8a)
cd android && ./gradlew assembleDebug      # app/build/outputs/apk/debug/app-arm64-v8a-debug.apk
```

Each ABI adds a whole `libnsh.so` (about 32 MB), so APKs carry one: Gradle splits them per ABI in `NSH_ABIS`. Releases ship the AAB (`bundleRelease`, CI sets `NSH_ABIS=arm64-v8a,armeabi-v7a`); Play gives each phone only its own library.

`prepare.sh` downloads `nsh.aar` from the NSH release matching `nsh --version` (checksum verified), replaces its `libnsh.so` with the one `nsh package` builds for this app (it holds the `nsh-mail/native` module), and writes the bundle with `nsh build --release`. Both outputs are git-ignored.

## Firebase

Without `app/google-services.json` the APK builds and runs, with push off. To turn it on:

1. Create a Firebase project and add an Android app with package `top.bangwu.nshmail`.
2. Download its `google-services.json` to `android/app/` (never commit it). For CI, save the file's content as the repository secret `GOOGLE_SERVICES_JSON`.
3. Create a service account key (Project settings → Service accounts) and point the server's `NSHMAIL_FCM_SERVICE_ACCOUNT` at it.

## How it talks to the app

- FCM token → host event `push.token` `{kind: "fcm", token}` → `app/push.ts` → `Push.setToken` → cloud mode `POST /v1/devices`.
- FCM data message `{type: "newMail", id, from?, subject?}` → a notification shaped like the app's own (`message:<id>`, nsh's channel and launch extra) → pressing it opens the message through the app's `Notifications` listener (`app/resident.ts`).
- `AndroidManifest.xml` mirrors what `nsh package` writes; keep it in step with `app/nsh.json` (permissions, URL schemes).
