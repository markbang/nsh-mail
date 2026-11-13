# nsh-mail-server

Cloud-mode backend for nsh-mail: accounts, mail sync, REST + WebSocket API, push (FCM, Web Push), and the web client's static files. API and design: [docs/design.md](../../docs/design.md) §4–§7.

nsh-mail 云端模式的服务端：账号、邮件同步、REST + WebSocket API、推送（FCM、Web Push），并托管 Web 客户端静态文件。接口与设计见 [docs/design.md](../../docs/design.md) §4–§7。

## Run / 运行

```sh
echo "NSHMAIL_MASTER_KEY=$(openssl rand -base64 32)" > .env
docker compose up -d          # repository root / 仓库根目录
curl localhost:8080/healthz
```

Without Docker / 不用 Docker: `NSHMAIL_MASTER_KEY=… cargo run -p nsh-mail-server`.

Back up `data/` and the master key together; without the key the stored mail credentials cannot be decrypted.
`data/` 与主密钥要一起备份；丢了主密钥，已存的邮箱凭据无法解密。

## Configuration / 配置

| Variable | Default | Meaning / 说明 |
|---|---|---|
| `NSHMAIL_MASTER_KEY` | required / 必填 | Base64 of 32 random bytes; encrypts mail credentials (AES-256-GCM). Never change it after accounts exist. 32 字节随机数的 base64，加密邮箱凭据；有账号后不能更换。 |
| `NSHMAIL_BIND` | `0.0.0.0:8080` | Listen address. 监听地址。 |
| `NSHMAIL_DATA_DIR` | `data` (`/data` in Docker) | `server.sqlite`, per user `users/<id>.sqlite` (mail) and `users/<id>.secrets.json` (encrypted credentials), generated VAPID key. 数据目录。 |
| `NSHMAIL_PUBLIC_URL` | `http://localhost:8080` | External URL; OAuth redirect URIs and the web return page are built from it. 对外地址，OAuth 回调和 Web 返回页由它生成。 |
| `NSHMAIL_WEB_DIR` | unset / 不托管 | Web client build to serve at `/`, with `app/web/sw.js` copied next to its `index.html` (served with `Service-Worker-Allowed: /`; Web Push needs it). Web 客户端目录，需把 `app/web/sw.js` 复制到 `index.html` 旁边（Web Push 需要）。 |
| `NSHMAIL_CORS_ORIGINS` | empty (same origin only) | Comma-separated origins allowed cross-origin, for development. 开发用跨域白名单，逗号分隔。 |
| `NSHMAIL_ALLOW_SIGNUP` | `true` | `false` closes sign-up. 关闭注册。 |
| `NSHMAIL_MAX_ATTACHMENTS_MB` | `25` | Most a message's attachments may add up to (1–1024); staged uploads (`uploads/`) expire after a day. 每封信附件总大小上限；暂存的上传一天后清理。 |
| `NSHMAIL_GOOGLE_CLIENT_ID` / `_SECRET` | unset | Google OAuth web client; redirect URI `<PUBLIC_URL>/v1/oauth/google/callback`. |
| `NSHMAIL_MS_CLIENT_ID` / `_SECRET` | unset | Microsoft OAuth web app; redirect URI `<PUBLIC_URL>/v1/oauth/microsoft/callback`. |
| `NSHMAIL_FCM_SERVICE_ACCOUNT` | unset / 不发 FCM | Path to the Firebase service account JSON. Firebase 服务账号 JSON 路径。 |
| `NSHMAIL_VAPID_PRIVATE_KEY` | generated in data dir / 自动生成 | Base64url 32-byte P-256 private key for Web Push. Web Push 私钥。 |
| `NSHMAIL_VAPID_SUBJECT` | `NSHMAIL_PUBLIC_URL` | `mailto:` or `https:` contact sent to push services (Apple rejects `http:`). 推送服务联系方式。 |
| `RUST_LOG` | `info` | Log filter. Paths only, never query strings or bodies. 日志只记路径。 |

## API additions / 补充接口

- `GET /v1/me`, `PATCH /v1/me {hideContent}` — notification privacy: when on, pushes carry only account and message ids. 通知隐私：开启后推送只带 id。
- `GET /v1/detect?email=` — provider presets and Mozilla autoconfig for the add-account form (`null` when unknown). 添加账号时的服务器探测。
- `POST /v1/sync` — sync every account now (pull to refresh). 立即同步全部账号。
- `GET /v1/events?device=<id>` — while this socket is open, that device gets no push. 连接期间该设备不再收推送。
- `GET /v1/oauth/{provider}/start?token=…&return=native|web` — ends at `nshmail://oauth-done?account=<id>` or `<PUBLIC_URL>/?oauth-done&account=<id>`; failures carry `error=<message>`. 失败时带 `error`。
- Push data / 推送数据: `{type:"newMail", account, id, from?, subject?}`.
