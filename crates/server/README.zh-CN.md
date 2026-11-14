[English](README.md) | 简体中文

# nsh-mail-server

nsh-mail 云端模式的服务端：账号、邮件同步、REST + WebSocket API、推送（FCM、Web Push），并托管 Web 客户端静态文件。接口与设计见 [docs/design.zh-CN.md](../../docs/design.zh-CN.md) §4–§7。

## 运行

```sh
echo "NSHMAIL_MASTER_KEY=$(openssl rand -base64 32)" > .env
docker compose up -d          # repository root
curl localhost:8080/healthz
```

镜像：`ghcr.io/markbang/nsh-mail-server`（仅接口）和 `ghcr.io/markbang/nsh-mail`（同时在 `/` 托管 Web 客户端），都从 `main` 构建，标签为 `latest` 和 `sha-<commit>`。容器健康检查运行 `nsh-mail-server healthcheck`（GET `/healthz`）。

不用 Docker：`NSHMAIL_MASTER_KEY=… cargo run -p nsh-mail-server`。

`data/` 与主密钥要一起备份；丢了主密钥，已存的邮箱凭据无法解密。

## 配置

| 变量 | 默认值 | 说明 |
|---|---|---|
| `NSHMAIL_MASTER_KEY` | 必填 | 32 字节随机数的 base64，加密邮箱凭据（AES-256-GCM）。有账号后不能更换。 |
| `NSHMAIL_BIND` | `0.0.0.0:8080` | 监听地址。 |
| `NSHMAIL_DATA_DIR` | `data`（Docker 中为 `/data`） | `server.sqlite`（用户、会话、设备和所有用户的邮件）、每个用户的 `users/<id>.secrets.json`（加密的凭据）、自动生成的 VAPID 密钥。旧版每用户的 `users/<id>.sqlite` 启动时导入并改名为 `*.migrated`；导入失败的文件会让服务端拒绝启动，文件保持不动。 |
| `NSHMAIL_PUBLIC_URL` | `http://localhost:8080` | 对外地址；OAuth 回调地址和 Web 返回页由它生成。 |
| `NSHMAIL_WEB_DIR` | 未设置 | 在 `/` 托管的 Web 客户端构建目录，需把 `app/web/sw.js` 复制到它的 `index.html` 旁边（带 `Service-Worker-Allowed: /` 响应头；Web Push 需要）。 |
| `NSHMAIL_CORS_ORIGINS` | 空（仅同源） | 允许跨域的来源，逗号分隔，开发用。 |
| `NSHMAIL_ALLOW_SIGNUP` | `true` | 设为 `false` 关闭注册。 |
| `NSHMAIL_MAX_ATTACHMENTS_MB` | `25` | 一封信附件合计大小上限（1–1024）；暂存的上传（`uploads/`）一天后过期。 |
| `NSHMAIL_GOOGLE_CLIENT_ID` / `_SECRET` | 未设置 | Google OAuth Web 客户端；回调地址 `<PUBLIC_URL>/v1/oauth/google/callback`。 |
| `NSHMAIL_MS_CLIENT_ID` / `_SECRET` | 未设置 | Microsoft OAuth Web 应用；回调地址 `<PUBLIC_URL>/v1/oauth/microsoft/callback`。 |
| `NSHMAIL_FCM_SERVICE_ACCOUNT` | 未设置（不发 FCM） | Firebase 服务账号 JSON 的路径。 |
| `NSHMAIL_VAPID_PRIVATE_KEY` | 在数据目录中自动生成 | Web Push 用的 Base64url 编码 32 字节 P-256 私钥。 |
| `NSHMAIL_VAPID_SUBJECT` | `NSHMAIL_PUBLIC_URL` | 发给推送服务的 `mailto:` 或 `https:` 联系方式（Apple 拒绝 `http:`）。 |
| `RUST_LOG` | `info` | 日志过滤。只记路径，从不记查询串或请求体。 |

## 补充接口

- `GET /v1/me`、`PATCH /v1/me {hideContent}` — 通知隐私：开启后推送只带账号和邮件 id。
- `GET /v1/detect?email=` — 添加账号表单用的服务商预设和 Mozilla autoconfig（未知时为 `null`）。
- `POST /v1/sync` — 立即同步全部账号（下拉刷新）。
- `GET /v1/events?device=<id>` — 这个连接打开期间，该设备不再收推送。
- `GET /v1/oauth/{provider}/start?token=…&return=native|web` — 结束时跳到 `nshmail://oauth-done?account=<id>` 或 `<PUBLIC_URL>/?oauth-done&account=<id>`；失败时带 `error=<说明>`。
- 推送数据：`{type:"newMail", account, id, from?, subject?}`。
- `GET|POST /v1/sources`、`GET|PATCH|DELETE /v1/sources/{id}`、`POST /v1/sources/{id}/token`、`POST /v1/sources/{id}/test` — 通知来源（见 [docs/design/notification-inbox.zh-CN.md](../../docs/design/notification-inbox.zh-CN.md)）。邮件账号就是一个 `kind: "mail"` 的 source，随账号创建和删除。新建 webhook/Gotify/ntfy 来源时返回一次 `{source, token, ingestUrl}`；`/token` 用于轮换，旧地址立刻失效。每封邮件也会被 `mail_core` 里的触发器投影进 feed（`feed_messages`），所以邮件和接收来的通知在列表与搜索里表现一致。

## 通知 feed

启动时 `server.sqlite` 会新增 `sources` 和 `feed_messages`；迁移可重复执行，并校验投影里正好每封邮件一行（草稿箱和垃圾箱除外）。投影是派生数据：

```sh
docker compose exec app nsh-mail-server rebuild-feed   # 以 JSON 打印行数
```

feed 出错时用它重建，邮件表只读。`NSHMAIL_DATA_DIR` 指定数据库位置。
