# nsh-mail 设计

把多个邮箱账号放进一个收件箱的客户端，用 [Native Shell](https://nsh.dev)（NSH 0.5）写成，目标平台 Web、Android、Linux、Windows。两种模式：

- **本地模式**：凭据和邮件只在本机，客户端直连 IMAP/SMTP。
- **云端模式**：nsh-mail 服务端代为同步，向客户端推送新邮件。

## 1. 平台事实（来自 NSH 文档）

| 能力 | NSH 提供 | 结论 |
|---|---|---|
| 原始 TCP | Rust crate 在原生平台可用 `tokio::net`（`background(..)`）；TSX `net` 模块仅原生平台，浏览器里报 `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` | IMAP/SMTP 放在 Rust，原生平台直连 |
| 浏览器网络 | 只有 `fetch`（受 CORS）和 WebSocket | Web 无法直连 IMAP/SMTP |
| 存储 | `Storage` 是 5 MB 键值；`data_dir()` 给出应用私有目录（Web 上为 `None`） | 邮件库用 SQLite 放在 `data_dir()`；`Storage` 只放设置 |
| 系统通知 | `Notifications`：Android（POST_NOTIFICATIONS）、Windows toast、Linux `org.freedesktop.Notifications`、Web `Notification` | 前台/常驻时直接用 |
| 桌面常驻 | `nsh/desktop` 的 `Tray` + 主窗口 `closeOnPress: false` | 关窗后留在托盘继续同步 |
| Android 打包 | `nsh package` 不经 Gradle，打包的应用不能加原生代码 | FCM 需要自有 Gradle 工程嵌入 NSH（见 §5） |
| 打包机器 | Linux 包只能在 Linux 上打，Windows 包只能在 Windows 上打；Android、Web 任意 | CI 用对应 runner |

## 2. Web 上的本地模式：不提供

浏览器不能开 TCP 连接。可选的"用户自己的中继"（WebSocket 转 TCP，TLS 在浏览器内的 wasm 里终止）技术上能做到中继看不到明文，但：

1. 凭据要存进浏览器存储（localStorage / OPFS），任何同源脚本或扩展都能读到，比系统钥匙串弱一档；
2. 开放的 TCP 中继容易被滥用为跳板，需要再做一套鉴权；
3. 标签页关掉就没有同步和通知，体验仍然依赖一台常驻机器。

既然用户总要运行一台常驻机器，那台机器直接跑 **nsh-mail 服务端** 更好：同一个二进制，自托管即可，Web 客户端连它就是"用户自己的中继"，并且得到同步、搜索和推送。所以 **Web 只有云端模式**，服务端地址可填官方或自托管实例；登录页明确说明这一点。

## 3. 架构

```
crates/mail-core     Rust 库，不依赖 NSH：IMAP 同步、SMTP 发送、MIME 解析、SQLite+FTS5 存储、
                     OAuth（PKCE）、服务商预设、凭据库。客户端和服务端共用。
crates/server        nsh-mail-server：axum + tokio。账号、同步调度、REST/WebSocket API、推送（FCM、Web Push）、
                     顺带托管 Web 客户端静态文件（同源，免 CORS）。
app/                 NSH 混合应用（TSX 界面 + Rust crate）。crate 导出 `nsh-mail/native`，TSX 只调它。
android/             Gradle 宿主工程：NshView + Firebase Messaging（§5）。
```

客户端内部只有一个 `Mail` 接口（Rust 导出给 TSX），背后两个实现：

- `LocalBackend`（仅原生平台，`cfg(not(target_arch = "wasm32"))`）：mail-core 直连邮箱。
- `CloudBackend`（全平台）：经 NSH `fetch` 调服务端 REST，WebSocket 收事件；原生平台把结果缓存进同一份 SQLite。

TSX 不知道当前是哪种模式，界面一套。状态归属：邮件、账号、同步状态在 Rust；当前选中、草稿、筛选在 TSX。

### TSX ↔ Rust 接口（`nsh-mail/native`）

```ts
Mail.init(): Promise<Status>                       // 打开库、恢复会话、开始同步
Mail.status(): Status                              // { mode: "none"|"local"|"cloud", server?, syncing, error? }
Mail.useLocal(): Promise<void>                     // Web 上 reject：ERR_UNSUPPORTED
Mail.signIn(server, email, password, signUp): Promise<void>   // 进入云端模式
Mail.signOut(): Promise<void>
Mail.accounts(): Promise<Account[]>
Mail.detect(email): Promise<ServerSettings | null> // 预设表 + Mozilla autoconfig
Mail.addImap(input: ImapInput): Promise<Account>   // 先连通 IMAP 和 SMTP 再保存
Mail.addOAuth(provider: "google"|"microsoft"): Promise<Account>
Mail.removeAccount(id): Promise<void>
Mail.list(q: { folder: "inbox"|"sent"|"drafts"|"archive"|"trash", account?, before?, limit }): Promise<Summary[]>
Mail.get(id): Promise<Message>                     // 正文已清洗；附件元数据
Mail.setFlags(id, { seen?, flagged? }): Promise<void>
Mail.send(draft: Draft): Promise<void>             // 回复时带 inReplyTo / references
Mail.search(text, limit): Promise<Summary[]>
Mail.refresh(): Promise<void>
Mail.on(listener: (e: MailEvent) => void): () => void   // newMail / changed / syncState / error
```

消息 id 是本地库的稳定字符串 id；云端模式下是服务端 id。统一收件箱 = 所有账号 INBOX 按日期倒序合并。

## 4. 数据流

### 本地模式（Linux、Windows、Android）

```
TSX ──Mail.*──▶ Rust LocalBackend ──tokio/rustls──▶ IMAP (993 / 143+STARTTLS)
                       │                       └──▶ SMTP (465 / 587+STARTTLS)
                       ▼
              SQLite（data_dir/mail.sqlite，FTS5）◀── 凭据：系统钥匙串
```

1. 添加账号：`detect` 填好服务器 → 连通测试 → 凭据进钥匙串，账号元数据进 SQLite。
2. 首次同步：每个文件夹取最近 500 封的信封和纯文本摘要，正文打开时按需下载并缓存。
3. 增量：按 `UIDVALIDITY`/`UIDNEXT` 取新信，支持 `CONDSTORE` 时用 `HIGHESTMODSEQ` 同步标记；UIDVALIDITY 变了就重建该文件夹。
4. 实时：INBOX 上 `IDLE`（29 分钟续约），不支持 IDLE 的服务器每 2 分钟轮询。新信写库后发 `newMail` 事件，并在应用不在前台时发系统通知。
5. 发送：lettre 经 SMTP 发出，成功后 `APPEND` 到"已发送"（Gmail 自动保存的除外）。
6. 搜索：本地 FTS5（主题、发件人、收件人、正文）；本地没有的旧信，后续版本再补 IMAP `SEARCH`。

桌面端主窗口关闭后留在托盘，IDLE 继续，系统通知照常。Android 本地模式只在应用打开时同步（没有后台服务），需要后台推送请用云端模式，设置页写明。

### 云端模式（全部平台）

```
客户端 ──HTTPS REST──▶ nsh-mail-server ──mail-core──▶ 各邮箱 IMAP/SMTP
   ▲  ◀──WebSocket 事件──┘   │
   │                         ├──FCM HTTP v1──▶ Android
   └─ 系统通知 ◀──────────────┴──Web Push (VAPID)──▶ 浏览器 Service Worker
```

1. 用户在服务端注册/登录（邮箱 + 密码），拿到会话 token；客户端存 token（原生平台进钥匙串，Web 进 localStorage）。
2. 添加账号：普通 IMAP 把应用专用密码交给服务端；OAuth 走服务端的 Web 流程（打开 `/v1/oauth/{provider}/start`，回调存 token，再跳回 `nshmail://oauth-done` 或 Web 页面）。
3. 服务端为每个邮箱账号常驻一个 tokio 任务，复用 mail-core 的同步逻辑，数据写进该用户自己的 SQLite（`data/users/<id>.sqlite`）。
4. 新信：写库 → 通过 WebSocket 发给在线客户端 → 给不在线的设备发推送：Android 用 FCM，Web 用 Web Push；桌面端是常驻进程，挂着 WebSocket，收到事件后自己弹系统通知。
5. 读、搜、发都走 REST；原生客户端把列表和正文缓存在本地 SQLite，离线可读。

### 服务端 API

```
POST /v1/auth/signup | /v1/auth/login        {email, password} → {token}
POST /v1/auth/logout
GET  /v1/accounts        POST /v1/accounts (imap)        DELETE /v1/accounts/{id}
GET  /v1/oauth/{google|microsoft}/start?token=…           GET /v1/oauth/{provider}/callback
GET  /v1/messages?folder=&account=&before=&limit=         GET /v1/messages/{id}
PATCH /v1/messages/{id}  {seen?, flagged?}
POST /v1/send            GET /v1/search?q=&limit=
POST /v1/devices         {kind: "fcm"|"webpush", token | subscription}     DELETE /v1/devices/{id}
GET  /v1/push/vapid-key  GET /v1/events (WebSocket，Authorization 或 ?token=)
GET  /                   Web 客户端静态文件
```

## 5. 推送方案

| 平台 | 应用不在前台时 | 实现 |
|---|---|---|
| Android | FCM data 消息 → `FirebaseMessagingService` 弹通知，点击带消息 id 打开 | `android/` Gradle 工程：依赖 nsh.aar 的 Java 运行时，`libnsh.so` 换成 nsh 用本 crate 构建的那份，加 `firebase-messaging`；FCM token 经 host 通道（`host.call("push.token")`）交给应用，再 `POST /v1/devices` |
| Web | Web Push → Service Worker `push` 事件 → `showNotification` | `nsh package --target web` 产物里加入 `sw.js`；Rust 侧经 wasm-bindgen 调 `PushManager.subscribe`（VAPID 公钥来自服务端） |
| Linux / Windows | 常驻进程（托盘）挂 WebSocket，收到 `newMail` 调 `Notifications.show` | NSH 自带，无需额外服务 |

服务端：FCM 用 HTTP v1 API + 服务账号 JWT；Web Push 用 `web-push` crate（aes128gcm 加密，VAPID 签名）。推送失效（404/410、`UNREGISTERED`）即删除该设备。

隐私：Web Push 内容端到端加密；FCM 载荷经过 Google，只带发件人名和主题，设置里可选"通知不显示内容"（只发消息 id，客户端自取）。

如果 Gradle 宿主路线验证不通，Android 退回 `nsh package` 直出 APK，云端模式在前台时用 WebSocket，后台推送等 NSH 支持后再加；这一点在 PR 里给出证据再定。

## 6. 云端服务技术选型

- **Rust + axum + tokio**：与 mail-core 同语言，同步逻辑一份代码。
- **存储**：每个用户一份 SQLite（与客户端同一套表和 FTS5），外加一个 `server.sqlite` 存用户、会话、设备。好处是复用和隔离；代价是单机。v1 单实例足够，以后横向扩展按用户分片。
- **密码**：Argon2id；会话 token 随机 32 字节，库里只存 SHA-256。
- **邮箱凭据**：AES-256-GCM 加密存储，主密钥来自环境变量 `NSHMAIL_MASTER_KEY`，不落库。
- **部署**：CI 构建 Docker 镜像推到 ghcr.io；`docker compose` 一键自托管。正式上线的主机和域名需要另行确认（付费、对外）。

## 7. 安全

- **OAuth 优先**：Gmail 和 Outlook 只引导 OAuth（XOAUTH2）。本地模式在客户端做授权码 + PKCE，回调用回环地址 `http://127.0.0.1:<随机端口>`（Google "桌面应用"客户端、Microsoft 公共客户端都支持，Android 也可用）；云端模式由服务端做 Web 流程。Google 范围 `https://mail.google.com/`，Microsoft 范围 `IMAP.AccessAsUser.All SMTP.Send offline_access`。客户端 ID 在构建时注入（`NSHMAIL_GOOGLE_CLIENT_ID`、`NSHMAIL_MS_CLIENT_ID`）。
- **普通 IMAP**：只接受应用专用密码（界面说明怎样生成，并给常见服务商的链接）；QQ、163、iCloud、Fastmail、Yahoo 等预设服务器。
- **凭据存储**：Windows 凭据管理器、Linux Secret Service（`keyring` crate）；没有 Secret Service 的 Linux 会话退回应用私有目录下 0600 的加密文件并提示。Android v1 存应用私有目录（系统沙箱隔离）的加密文件，Gradle 宿主就位后改用 Android Keystore。
- **传输**：只允许 TLS（993/465 或 STARTTLS），拒绝明文；rustls + webpki 根证书。
- **邮件内容**：HTML 先用 ammonia 清洗，再用 NSH `TextView` 以原生文本渲染（不跑脚本）；远程图片默认不加载，防跟踪。
- **日志**：不记录凭据、token 和正文。

## 8. CI 与产物

GitHub Actions，每个 PR 跑：

| Job | Runner | 产物 |
|---|---|---|
| core + server | ubuntu-24.04 | `cargo test`、`clippy`；服务端 Docker 镜像（main 上推 ghcr） |
| linux | ubuntu-24.04（GTK 4.14） | `nsh package --target linux` → `.tar.gz` |
| windows | windows-latest | `nsh package --target windows` → `.zip` + 安装器 |
| android | ubuntu-24.04 + SDK/NDK | `.apk`（调试签名） |
| web | ubuntu-24.04 + wasm32 | 静态站点 `.zip` |

## 9. 第一版范围

添加账号（IMAP + OAuth）、统一收件箱、读信、写信和回复、搜索、本地模式完整可用；云端模式能同步并推送；CI 出四个平台的产物。暂不做：附件上传、多选批量操作、文件夹管理、规则、PGP。
