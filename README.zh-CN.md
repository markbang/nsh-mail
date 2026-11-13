[English](README.md) | 简体中文

# nsh-mail

把所有邮箱放进一个收件箱的客户端，基于 [Native Shell](https://nsh.dev)，支持 Web、Android、Linux、Windows。本地模式凭据和邮件只在本机；云端模式由服务端同步并推送新邮件。

<picture><source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/markbang/nsh-mail/screenshots/main/web-inbox-dark.png"><img src="https://raw.githubusercontent.com/markbang/nsh-mail/screenshots/main/web-inbox-light.png" alt="收件箱" width="100%"></picture>

| 阅读 | 写信 |
|---|---|
| <picture><source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/markbang/nsh-mail/screenshots/main/web-reader-dark.png"><img src="https://raw.githubusercontent.com/markbang/nsh-mail/screenshots/main/web-reader-light.png" alt="阅读" width="100%"></picture> | <picture><source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/markbang/nsh-mail/screenshots/main/web-compose-dark.png"><img src="https://raw.githubusercontent.com/markbang/nsh-mail/screenshots/main/web-compose-light.png" alt="写信" width="100%"></picture> |

<picture><source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/markbang/nsh-mail/screenshots/main/android-inbox-dark.png"><img src="https://raw.githubusercontent.com/markbang/nsh-mail/screenshots/main/android-inbox-light.png" alt="Android 收件箱" width="270"></picture> <picture><source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/markbang/nsh-mail/screenshots/main/android-reader-dark.png"><img src="https://raw.githubusercontent.com/markbang/nsh-mail/screenshots/main/android-reader-light.png" alt="Android 阅读" width="270"></picture>

<sub>截图由 CI 从 main 自动生成（[tools/screenshots](tools/screenshots)）。</sub>

## 安装

从 [Releases](https://github.com/markbang/nsh-mail/releases/latest) 下载：

- **Android 8+**：`nsh-mail-<版本>-android-arm64.apk`。装过 0.1.x 测试版的，先卸载一次：正式版换了签名密钥（证书 SHA-256 `F3:92:9A:E4:…:6B:4B:42`）。
- **Linux x86_64**：`nsh-mail-<版本>-linux-x86_64.tar.gz`。需要 GTK 4.14+；显示 HTML 邮件需要 WebKitGTK 6.0。
- **Windows x64**：`nsh-mail-<版本>-windows-x64-setup.exe` 或 `.zip`。未签名，首次运行 SmartScreen 会警告。
- **Web 和云端模式**：服务端镜像 `ghcr.io/markbang/nsh-mail` 自带 Web 客户端（[部署](crates/server/README.zh-CN.md)）。

每个版本的 `SHA256SUMS.txt` 列出校验和。

设计：[docs/design.zh-CN.md](docs/design.zh-CN.md)
