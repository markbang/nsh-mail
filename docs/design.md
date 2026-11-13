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
| 选文件 | `Picker.pickFiles` 各平台都有（Web 为 `<input type=file>`），给出 `File`，整份读进内存（单个上限 512 MiB） | 附件按钮用它 |
| 粘贴 | `Input` 的 `onPaste` 给出剪贴板里的图片等文件（`File`）和文件管理器复制的 `file:` URL（Web 上读不到） | 正文框监听粘贴 |
| 拖放 | `View` 的 `onFileDrop` 只在桌面（GTK、Win32、AppKit）给出 `file:` URL；Web 没有，而 Web 上 TSX 跑在单独的 realm（同源 iframe）里，收不到页面的拖放事件 | 桌面用 `onFileDrop`，路径交给 Rust 读（TSX 的 `fs/promises` 要 `capabilities.fs` 授权）；Web 由 Rust 经 web-sys 在页面 `window` 上听 `dragover`/`drop`（`PageDrop.listen`） |
| TSX ↔ Rust 传字节 | `Vec<u8>` 对应 `Uint8Array`，调用时复制 | 附件字节经 `Mail.stage` 一次过边界，之后只传 id |

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
- `CloudBackend`（全平台）：经 NSH `fetch` 调服务端 REST，WebSocket 收事件（原生用 tokio-tungstenite，Web 用 NSH 的浏览器 WebSocket 传输），断线按退避重连。原生平台缓存列表和正文留待后续：mail-core 没有写入服务端结果的公开接口。

TSX 不知道当前是哪种模式，界面一套。状态归属：邮件、账号、同步状态在 Rust；当前选中、草稿、筛选在 TSX。

### TSX ↔ Rust 接口（`nsh-mail/native`）

```ts
Mail.init(): Promise<Status>                       // 打开库、恢复会话、开始同步
Mail.status(): Status                              // { mode: "none"|"local"|"cloud", server?, user?, syncing, error? }
Mail.useLocal(): Promise<void>                     // Web 上 reject：ERR_UNSUPPORTED
Mail.signIn(server, email, password, signUp): Promise<void>   // 进入云端模式
Mail.signOut(): Promise<void>
Mail.accounts(): Promise<Account[]>
Mail.detect(email): Promise<ServerSettings | null> // 预设表 + Mozilla autoconfig
Mail.addImap(input: ImapInput): Promise<Account>   // 先连通 IMAP 和 SMTP 再保存
Mail.addOAuth(provider: "google"|"microsoft"): Promise<Account>
Mail.removeAccount(id): Promise<void>
Mail.list(q: { folder: "inbox"|"sent"|"drafts"|"archive"|"trash", account?, before?, limit }): Promise<Summary[]>
Mail.get(id): Promise<Message>                     // 正文已清洗，排版表格已改成块（见 §7）；附件元数据
Mail.openAttachment(id, attachment): Promise<string | null>  // Web：图片/PDF 新标签页打开，其余下载；桌面：系统保存对话框后用默认程序打开；
                                                   // 没有保存对话框时（Android）存进应用目录再打开。返回已保存但没有程序打开的文件路径
Mail.attachmentPreview(id, attachment): Promise<string>      // PNG/JPEG/GIF/WebP（≤ 8 MB）的 data: URI，阅读页缩略图
Mail.setFlags(id, { seen?, flagged? }): Promise<void>
Mail.moveTo(id, "archive" | "trash" | …): Promise<void>   // IMAP MOVE（无 MOVE 时 COPY + \Deleted + EXPUNGE）；没有 Archive/Trash 时创建，Gmail 的归档是 All Mail
Mail.send(draft: Draft): Promise<void>             // 回复时带 inReplyTo / references；attachments 见 §4 附件
Mail.stage(name, type, bytes: Uint8Array, onProgress: (sent) => void): Promise<Upload>   // 暂存附件，得到 id
Mail.stageFile(path, type, onProgress): Promise<Upload>   // 本地文件（桌面拖放、文件管理器复制后粘贴），Rust 读；Web 上 reject
Mail.unstage(id): Promise<void>
Mail.attachmentLimit(): number                     // 一封信附件总大小上限（字节）
Mail.search(text, limit): Promise<Summary[]>
Mail.unread(): Promise<Record<accountId, number>>  // 各账号收件箱的未读数
Mail.refresh(): Promise<void>
Mail.on(listener: (e: MailEvent) => void): () => void   // newMail / changed / syncState / error
Push.setToken(token): Promise<void>                // Android 宿主的 FCM token（§5），云端模式登记到服务端
```

消息 id 是本地库的稳定字符串 id；云端模式下是服务端 id。统一收件箱 = 所有账号 INBOX 按日期倒序合并。

各类型的字段与 `mail_core::types` 一一对应，JSON 形状相同（serde 的 tag 成为 `type`/`kind` 字段，`Message` 平铺 `Summary` 的字段）；`list` 的 `before` 是上一页最后一封的 id；`MailEvent` 以 `type` 区分。这些类型就是 `mail_core::types`：后端直接用它们，经 `app/src/types.rs` 的 `Js<T>` 以 serde JSON 过边界，TS 声明在同一文件里手写（Web 只编译 mail-core 的 `types`，不带引擎）。

## 4. 数据流

### 本地模式（Linux、Windows、Android）

```
TSX ──Mail.*──▶ Rust LocalBackend ──tokio/rustls──▶ IMAP (993 / 143+STARTTLS)
                       │                       └──▶ SMTP (465 / 587+STARTTLS)
                       ▼
              SQLite（data_dir/mail.sqlite，FTS5）◀── 凭据：系统钥匙串
```

1. 添加账号：`detect` 填好服务器 → 连通测试 → 凭据进钥匙串，账号元数据进 SQLite。
2. 首次同步：每个文件夹取最近 500 封。不超过 32 KB 的整封下载（离线可读、全文索引）；更大的只取信头和第一个文本部分做摘要（按 `BODYSTRUCTURE` 定位，不用部分 FETCH），正文打开时按需下载并缓存。
3. 增量：新信取本地已有最大 UID 之上的（`UID FETCH n:*`，RFC 4549 §4.3.1），`UIDNEXT` 只用来判断有没有新信，不当水位线（服务器可能先分配 UID、后让信可见）；支持 `CONDSTORE` 时用 `HIGHESTMODSEQ` 同步标记；UIDVALIDITY 变了就重建该文件夹。
4. 实时：INBOX 上 `IDLE`，收到 `EXISTS`/`EXPUNGE`/`FETCH`（别的客户端改了标记）就同步 INBOX；每 30 秒重发一次 IDLE，因为有的服务器（如 GreenMail）把别处的标记改动攒到 IDLE 结束才发；每 29 分钟全量同步。不支持 IDLE 的服务器每 2 分钟轮询。命令连接每账户一条、复用；复用的连接已被服务器关掉（空闲超时）时，在新连接上重试一次，其他错误不重试。连接或同步出错时记日志（带账户 id），按 5 秒到 5 分钟退避重连并全量同步。新信写库后发 `newMail` 事件，并在应用不在前台时发系统通知。
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
3. 服务端为每个邮箱账号常驻一个 tokio 任务，复用 mail-core 的同步逻辑，数据写进共享的 `data/server.sqlite`，每行带 `user_id`。
4. 新信：写库 → 通过 WebSocket 发给在线客户端 → 给不在线的设备发推送：Android 用 FCM，Web 用 Web Push；桌面端是常驻进程，挂着 WebSocket，收到事件后自己弹系统通知。
5. 读、搜、发都走 REST。原生客户端在本地 SQLite 缓存列表和正文（离线可读）尚未实现，待 mail-core 提供写入接口。

### 附件

草稿的 `attachments` 是 `{kind: "upload", id}`（`Mail.stage` 暂存的文件）或 `{kind: "forward", messageId, index}`（已存邮件的附件，转发默认全带、可逐个去掉）。

- 来源：附件按钮 `Picker.pickFiles`；正文框粘贴（`onPaste` 的 `files`，桌面上还有 `uris`）；拖放（桌面 `onFileDrop`，Web 用 `PageDrop.listen`）。`File` 的字节由 TSX 交给 `Mail.stage`，本地路径交给 `Mail.stageFile`。
- 暂存：本地模式放在 Rust 内存里，发出后丢弃；云端模式 `POST /v1/uploads` 建档（名字、类型、大小），再按 1 MiB 分块 `PATCH /v1/uploads/{id}?offset=` 上传，每块之后报进度（NSH 的 Rust `fetch` 整体收发，没有上传进度）。关掉写信页时 `unstage` 未发出的文件。
- 上限：附件合计默认 25 MB（Gmail、Outlook 的上限），按原始字节算。客户端加入文件时就检查（`Mail.attachmentLimit()`，云端模式取服务端 `/v1/me` 的 `maxAttachmentBytes`）；mail-core 发送前、服务端建档和发送时再检查，超限是 `ERR_TOO_LARGE`（HTTP 413）。
- 编码：lettre 组 `multipart/mixed`，文件名在 `Content-Disposition` 里按 RFC 2231，`Content-Type` 的 `name` 按 RFC 2047（老客户端只认这个）。"已发送"里 `APPEND` 的就是发出的整封信，附件随之保存。
- 服务端：分块存在 `data/uploads/<用户>/<id>`（元数据 `<id>.json`），发送成功即删除，其余 24 小时后由每小时一次的清理删除。

### 服务端 API

```
POST /v1/auth/signup | /v1/auth/login        {email, password} → {token}
POST /v1/auth/logout      GET /v1/me    PATCH /v1/me  {hideContent}（通知不显示内容）
GET  /v1/accounts        POST /v1/accounts (imap)        DELETE /v1/accounts/{id}
GET  /v1/detect?email=   POST /v1/sync（立即同步全部账号）
GET  /v1/oauth/{google|microsoft}/start?token=…           GET /v1/oauth/{provider}/callback
GET  /v1/messages?folder=&account=&before=&limit=         GET /v1/messages/{id}
GET  /v1/messages/{id}/attachments/{index}  附件字节：Content-Type 为其类型，Content-Disposition: attachment（filename* 按 RFC 5987 带 UTF-8 原名），
                                            CSP sandbox + nosniff；本地没有正文时从 IMAP 取并缓存
PATCH /v1/messages/{id}  {seen?, flagged?}
POST /v1/messages/{id}/move  {folder}     移到同一账号的另一个文件夹（归档、删除到废纸篓）
POST /v1/send            GET /v1/search?q=&limit=      GET /v1/unread → {accountId: 未读数}
POST /v1/uploads         {filename, mimeType, size} → Upload {id, filename, mimeType, size}
PATCH /v1/uploads/{id}?offset=   分块原始字节（≤ 4 MiB）→ {received}      DELETE /v1/uploads/{id}
POST /v1/devices         {kind: "fcm"|"webpush", token | subscription, vapidKey?}  DELETE /v1/devices/{id}
GET  /v1/push/vapid-key  GET /v1/events (WebSocket，Authorization 或 ?token=；带 ?device=<id> 时该设备在线期间不发推送)
GET  /                   Web 客户端静态文件         GET /healthz
```

错误响应体是 `{code, message}`（即 mail-core 的 `Error`，`code` 如 `ERR_AUTH`），HTTP 状态码表示类别。OAuth 结束时跳到 `nshmail://oauth-done?account=<id>`（`start` 带 `return=native`）或 `<公开地址>/?oauth-done&account=<id>`，失败时带 `error=<说明>`。服务端配置见 `crates/server/README.md`。

## 5. 推送方案

| 平台 | 应用不在前台时 | 实现 |
|---|---|---|
| Android | FCM data 消息 → `FirebaseMessagingService` 弹通知，点击打开该信 | `android/` Gradle 工程（见下） |
| Web | Web Push → Service Worker `push` 事件 → `showNotification` | NSH 打包不带额外文件，打包后把 `app/web/sw.js` 复制到 `index.html` 旁（CI 的 web job 已做）；Rust 侧经 wasm-bindgen 调 `PushManager.subscribe`（VAPID 公钥来自服务端），在用户允许通知后订阅 |
| Linux / Windows | 常驻进程（托盘）挂 WebSocket，收到 `newMail` 调 `Notifications.show` | NSH 自带，无需额外服务 |

服务端：FCM 用 HTTP v1 API + 服务账号 JWT；Web Push 用 `web-push` crate（aes128gcm 加密，VAPID 签名）。推送失效（404/410、`UNREGISTERED`）即删除该设备。Web Push 订阅绑定订阅时的 VAPID 公钥：服务端随设备记下当时的公钥，启动时删掉公钥不同的订阅；登记时带的 `vapidKey` 不是当前公钥就回 409。Web 客户端每次启动取 `/v1/push/vapid-key`，已有订阅的 `applicationServerKey` 不同就先退订再订阅，然后照常 `POST /v1/devices`。

隐私：Web Push 内容端到端加密；FCM 载荷经过 Google，只带发件人名和主题，设置里可选"通知不显示内容"（只发消息 id，客户端自取）。

Android 宿主（`android/`，用法见其 README）：

- **运行时**：`prepare.sh` 取与 CLI 同版本的 `nsh.aar`（Java 运行时，校验 sha256），其中的 `libnsh.so` 换成 `nsh package --target android` 用本 crate 构建的那份（`.nsh/rust/android-release/lib`，含 `nsh-mail/native`），bundle 用 `nsh build --release` 放进 assets。`MainActivity` 照 nsh 的 `AppActivity` 写：`NshView.loadAssetBundle`、通知与 URL intent、返回键、沉浸式系统栏。
- **token**：宿主把 FCM token 作为 host 事件 `push.token` `{kind: "fcm", token}` 发给应用，`app/push.ts` 交给 `Push.setToken`，云端模式登录后经 `on_token` 拿到并 `POST /v1/devices`。用事件而不是 `host.call`：事件不需要授权，而 `host.call` 要声明 `capabilities.host`，声明任何 capabilities 都会让没列出的能力（网络、通知……）全部被拒。
- **通知**：data 消息按应用自己的通知的样子显示（nsh 的通知渠道、标签 `message:<id>`、启动 extra），点击后 `handleNotificationIntent` 交给应用已有的 `Notifications` 监听（`app/resident.ts`）打开该信，无需新的 TSX。
- 没有 `google-services.json` 时照常构建运行，只是不推送；CI 有 secret `GOOGLE_SERVICES_JSON` 时写入。

## 6. 云端服务技术选型

- **Rust + axum + tokio**：与 mail-core 同语言，同步逻辑一份代码。
- **存储**：一个 `server.sqlite` 存用户、会话、设备，以及所有用户的邮件（与客户端同一套表和 FTS5）。邮件表每行带 `user_id`，索引以它开头；mail-core 只在绑定了用户的 `Store` 上提供查询，漏写用户的查询写不出来。客户端本地库是同一套表，只有一个用户（`LOCAL_USER` = 0）。一个文件好备份、好迁移；代价是单机。v1 单实例足够。旧版每用户一份的 `data/users/<id>.sqlite` 在启动时导入（逐表核对行数和内容，事务内完成），原文件改名为 `*.migrated` 保留；导入或核对失败则拒绝启动、原文件不动。
- **密码**：Argon2id；会话 token 随机 32 字节，库里只存 SHA-256。
- **邮箱凭据**：AES-256-GCM 加密存储在每个用户的 `data/users/<id>.secrets.json`（mail-core 的 `EncryptedFileStore`），主密钥来自环境变量 `NSHMAIL_MASTER_KEY`，不落盘。轮换：新钥放 `NSHMAIL_MASTER_KEY`、旧钥放 `NSHMAIL_MASTER_KEY_OLD`，启动时先读全部文件，有条目两把钥匙都解不开就拒绝启动、不改任何文件；否则把旧钥加密的条目逐文件重写（先留 `.bak-old-key`，原子替换，只用新钥重读核对后删副本），之后可去掉 `NSHMAIL_MASTER_KEY_OLD`。只设一把钥匙时解不开同样拒绝启动。
- **部署**：CI 构建 Docker 镜像推到 ghcr.io；`docker compose` 一键自托管。正式上线的主机和域名需要另行确认（付费、对外）。

## 7. 安全

- **OAuth 优先**：Gmail 和 Outlook 只引导 OAuth（XOAUTH2）。本地模式在客户端做授权码 + PKCE，回调用回环地址 `http://127.0.0.1:<随机端口>`（Google "桌面应用"客户端、Microsoft 公共客户端都支持，Android 也可用）；云端模式由服务端做 Web 流程。Google 范围 `https://mail.google.com/`，Microsoft 范围 `IMAP.AccessAsUser.All SMTP.Send offline_access`；两家都另加 `openid email`，从 ID token 取账号邮箱。客户端 ID 在构建时注入（`NSHMAIL_GOOGLE_CLIENT_ID`、`NSHMAIL_MS_CLIENT_ID`）。
- **普通 IMAP**：只接受应用专用密码（界面说明怎样生成，并给常见服务商的链接）；QQ、163、iCloud、Fastmail、Yahoo 等预设服务器。
- **凭据存储**：Windows 凭据管理器、Linux Secret Service（`keyring` crate）；没有 Secret Service 的 Linux 会话退回应用私有目录下 0600 的加密文件并提示。Android v1 存应用私有目录（系统沙箱隔离）的加密文件，Gradle 宿主就位后改用 Android Keystore。
- **传输**：只允许 TLS（993/465 或 STARTTLS），拒绝明文；rustls + webpki 根证书。
- **邮件内容**：HTML 在 mail-core 里用 ammonia 清洗一次（去掉脚本、事件属性、`javascript:`、表单、iframe/object/embed、`<meta>`/`<link>`/`<base>`；保留 `<style>`、`style`/`class` 和表格排版属性），`cid:` 内嵌图片换成 `data:` URI（每封合计 10 MB 内）。客户端用 NSH `WebView` 显示（Web 上是不带 `allow-scripts` 的 sandbox iframe，原生关闭 JS、`incognito`），文档里的 CSP 默认挡住所有远程资源（图片、CSS `url()`、字体），用户点“显示图片”才放开。`nsh.json` 的 `capabilities.webview.hosts` 为空：WebView 里的链接导航被原生拒绝并以 `onError` 报回，客户端交给系统浏览器打开；Web 上靠 `<base target="_blank">` 开新标签页。HTML 放在浅色卡片里（`color-scheme: light`），深色模式不反色。没有 HTML 部分的邮件、没有 Web 引擎的平台（Linux 缺 WebKitGTK 6.0、Windows 缺 WebView2）显示纯文本。
- **附件**：Web 上只有图片和 PDF 以原类型的 blob 在新标签页打开，其余一律以 `application/octet-stream` 下载，防止 HTML/SVG 附件在本站源下运行；服务端附件接口同样带 `Content-Security-Policy: sandbox`。
- **日志**：不记录凭据、token 和正文。

## 8. CI 与产物

GitHub Actions，每个 PR 跑：

| Job | Runner | 产物 |
|---|---|---|
| core + server | ubuntu-24.04 | `cargo test`、`clippy`；服务端 Docker 镜像（main 上推 ghcr） |
| linux | ubuntu-24.04（GTK 4.14） | `nsh package --target linux` → `.tar.gz` |
| windows | windows-latest（只在 `v*` 标签和手动运行） | `nsh package --target windows` → `.zip` + 安装器；有证书 secret 时 signtool 签名（带时间戳） |
| android | ubuntu-24.04 + SDK/NDK | `nsh package` 的 `.apk`，以及 Gradle 宿主（FCM）的 `.apk`，只含 arm64-v8a；PR 和 main 用调试签名。`v*` 标签和手动运行在有密钥 secret 时用正式签名，发布产物是两份 `.aab`（arm64-v8a + armeabi-v7a，Play 按设备只下发一份 `libnsh.so`），旁边的 `nsh package` `.apk` 仍只含 arm64-v8a，Gradle 宿主按 ABI 各出一份 `.apk` |
| web | ubuntu-24.04 + wasm32 | 静态站点 `.zip` |

## 9. 第一版范围

添加账号（IMAP + OAuth）、统一收件箱、读信、写信和回复、搜索、本地模式完整可用；云端模式能同步并推送；CI 出四个平台的产物。暂不做：多选批量操作、文件夹管理、规则、PGP。
