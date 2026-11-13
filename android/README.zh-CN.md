[English](README.md) | 简体中文

# Android 宿主（FCM 推送）

`nsh package --target android` 不经 Gradle，打出的 APK 不能加 Firebase。这个 Gradle 工程用 `NshView` 运行同一个应用，再加上 Firebase Cloud Messaging，云端模式在应用关闭时也能推送新邮件。设计见 [docs/design.zh-CN.md](../docs/design.zh-CN.md) §5。

## 构建

需要 nsh、JDK 17、Android SDK（`platforms;android-35`、build tools、NDK）和 Rust 目标 `aarch64-linux-android`（正式发布还要 `armv7-linux-androideabi`）。

```sh
android/prepare.sh                         # nsh.aar + libnsh.so with the app's crate, and the bundle
                                           # NSH_ABIS=x86_64 for an emulator (default arm64-v8a)
cd android && ./gradlew assembleDebug      # app/build/outputs/apk/debug/app-arm64-v8a-debug.apk
```

每个 ABI 都是一整份 `libnsh.so`（约 32 MB），所以每个 APK 只带一个：Gradle 按 `NSH_ABIS` 分 ABI 出包。正式发布用 AAB（`bundleRelease`，CI 设 `NSH_ABIS=arm64-v8a,armeabi-v7a`），Play 只给每台手机下发它自己的那份。

`prepare.sh` 从与 `nsh --version` 相同的 NSH 版本下载 `nsh.aar`（校验 sha256），把其中的 `libnsh.so` 换成 `nsh package` 为本应用构建的那份（含 `nsh-mail/native` 模块），并用 `nsh build --release` 生成 bundle。产物都不进 git。

## Firebase

没有 `app/google-services.json` 时 APK 照常构建运行，只是没有推送。开启步骤：

1. 创建 Firebase 项目，添加包名为 `top.bangwu.nshmail` 的 Android 应用。
2. 下载它的 `google-services.json` 放到 `android/app/`（不要提交）。CI 用仓库 secret `GOOGLE_SERVICES_JSON` 存文件内容。
3. 生成服务账号密钥（项目设置 → 服务账号），把服务端的 `NSHMAIL_FCM_SERVICE_ACCOUNT` 指向它。

## 与应用的交互

- FCM token → host 事件 `push.token` `{kind: "fcm", token}` → `app/push.ts` → `Push.setToken` → 云端模式 `POST /v1/devices`。
- FCM data 消息 `{type: "newMail", id, from?, subject?}` → 按应用自己的通知样式显示（`message:<id>`、nsh 的通知渠道和启动 extra）→ 点击后经应用的 `Notifications` 监听（`app/resident.ts`）打开该信。
- `AndroidManifest.xml` 与 `nsh package` 生成的一致；改动时与 `app/nsh.json` 保持同步（权限、URL scheme）。
