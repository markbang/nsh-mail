[English](https://github.com/markbang/nsh-mail/releases/tag/vX.Y.Z) | 简体中文

<!-- 复制为 docs/releases/X.Y.Z.zh-CN.md（TEMPLATE.md 复制为 X.Y.Z.md），替换 X.Y.Z，把 app/nsh.json 的 "build" 加一，再推送标签 vX.Y.Z。 -->

## X.Y.Z 包含

- 

## 下载

| 平台 | 文件 | 说明 |
|---|---|---|
| Android 8+ | `nsh-mail-X.Y.Z-arm64-v8a.apk`、`-armeabi-v7a.apk`、`-x86_64.apk`、`-universal.apk` | 正式签名。 |
| Android（Google Play） | `nsh-mail-X.Y.Z.aab` | 应用包，含 arm64-v8a 和 armeabi-v7a。 |
| Linux x86_64 | `nsh-mail-X.Y.Z-linux-x86_64.tar.gz` | 需要 GTK 4.14+；显示 HTML 邮件需要 WebKitGTK 6.0。 |
| Windows x64 | `nsh-mail-X.Y.Z-windows-x64-setup.exe`、`nsh-mail-X.Y.Z-windows-x64.zip` | 未签名：首次运行 SmartScreen 会警告（“更多信息”→“仍要运行”）。 |
| 服务端 | `ghcr.io/markbang/nsh-mail` | 云端模式服务端的 Docker 镜像，内含 Web 客户端。 |

选哪个 APK？大多数手机：arm64-v8a。老的 32 位手机：armeabi-v7a。模拟器和 Chromebook：x86_64。拿不准：universal（体积大）。Google Play 或企业分发：AAB。

`SHA256SUMS.txt` 列出所有文件的校验和。

Android 签名证书 SHA-256：`F3:92:9A:E4:A1:E2:AF:07:03:AA:0C:E7:AC:E2:B5:6E:E9:9F:1E:B1:0B:B3:A4:1F:C8:11:07:DE:E4:6B:4B:42`
