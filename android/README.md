# Android host (FCM push)

`nsh package --target android` builds APKs without Gradle, so they cannot carry Firebase. This Gradle project runs the same app in an `NshView` and adds Firebase Cloud Messaging, so cloud mode can push new mail while the app is closed. Design: [docs/design.md](../docs/design.md) §5.

`nsh package --target android` 不经 Gradle，打出的 APK 不能加 Firebase。这个 Gradle 工程用 `NshView` 运行同一个应用，再加上 Firebase Cloud Messaging，云端模式在应用关闭时也能推送新邮件。设计见 §5。

## Build / 构建

Needs nsh, JDK 17, the Android SDK (`platforms;android-35`, build tools, an NDK) and the Rust targets `aarch64-linux-android` and `armv7-linux-androideabi`.

```sh
android/prepare.sh                         # nsh.aar + libnsh.so with the app's crate, and the bundle
cd android && ./gradlew assembleDebug      # app/build/outputs/apk/debug/app-debug.apk
```

`prepare.sh` downloads `nsh.aar` from the NSH release matching `nsh --version` (checksum verified), replaces its `libnsh.so` with the one `nsh package` builds for this app (it holds the `nsh-mail/native` module), and writes the bundle with `nsh build --release`. Both outputs are git-ignored.

`prepare.sh` 从与 `nsh --version` 相同的 NSH 版本下载 `nsh.aar`（校验 sha256），把其中的 `libnsh.so` 换成 `nsh package` 为本应用构建的那份（含 `nsh-mail/native` 模块），并用 `nsh build --release` 生成 bundle。产物都不进 git。

## Firebase

Without `app/google-services.json` the APK builds and runs, with push off. To turn it on:

没有 `app/google-services.json` 时 APK 照常构建运行，只是没有推送。开启步骤：

1. Create a Firebase project and add an Android app with package `top.bangwu.nshmail`. 创建 Firebase 项目，添加包名为 `top.bangwu.nshmail` 的 Android 应用。
2. Download its `google-services.json` to `android/app/` (never commit it). For CI, save the file's content as the repository secret `GOOGLE_SERVICES_JSON`. 下载 `google-services.json` 放到 `android/app/`（不要提交）；CI 用仓库 secret `GOOGLE_SERVICES_JSON` 存文件内容。
3. Create a service account key (Project settings → Service accounts) and point the server's `NSHMAIL_FCM_SERVICE_ACCOUNT` at it. 生成服务账号密钥，服务端的 `NSHMAIL_FCM_SERVICE_ACCOUNT` 指向它。

## How it talks to the app / 与应用的交互

- FCM token → host event `push.token` `{kind: "fcm", token}` → `app/push.ts` → `Push.setToken` → cloud mode `POST /v1/devices`.
- FCM data message `{type: "newMail", id, from?, subject?}` → a notification shaped like the app's own (`message:<id>`, nsh's channel and launch extra) → pressing it opens the message through the app's `Notifications` listener (`app/resident.ts`).
- `AndroidManifest.xml` mirrors what `nsh package` writes; keep it in step with `app/nsh.json` (permissions, URL schemes).
