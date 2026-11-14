English | [简体中文](README.zh-CN.md)

# nsh-mail-server

Cloud-mode backend for nsh-mail: accounts, mail sync, REST + WebSocket API, push (FCM, Web Push), and the web client's static files. API and design: [docs/design.md](../../docs/design.md) §4–§7.

## Run

```sh
echo "NSHMAIL_MASTER_KEY=$(openssl rand -base64 32)" > .env
docker compose up -d          # repository root
curl localhost:8080/healthz
```

Images: `ghcr.io/markbang/nsh-mail-server` (API only) and `ghcr.io/markbang/nsh-mail` (also serves the web client at `/`), both `latest` and `sha-<commit>` from `main`. The container health check runs `nsh-mail-server healthcheck` (GET `/healthz`).

Without Docker: `NSHMAIL_MASTER_KEY=… cargo run -p nsh-mail-server`.

Back up `data/` and the master key together; without the key the stored mail credentials cannot be decrypted.

## Configuration

| Variable | Default | Meaning |
|---|---|---|
| `NSHMAIL_MASTER_KEY` | required | Base64 of 32 random bytes; encrypts mail credentials (AES-256-GCM). Never change it after accounts exist. |
| `NSHMAIL_BIND` | `0.0.0.0:8080` | Listen address. |
| `NSHMAIL_DATA_DIR` | `data` (`/data` in Docker) | `server.sqlite` (users, sessions, devices and every user's mail), per user `users/<id>.secrets.json` (encrypted credentials), generated VAPID key. Old per-user `users/<id>.sqlite` files are imported at start and renamed `*.migrated`; a file that fails to import stops the server and stays untouched. |
| `NSHMAIL_PUBLIC_URL` | `http://localhost:8080` | External URL; OAuth redirect URIs and the web return page are built from it. |
| `NSHMAIL_WEB_DIR` | unset | Web client build to serve at `/`, with `app/web/sw.js` copied next to its `index.html` (served with `Service-Worker-Allowed: /`; Web Push needs it). |
| `NSHMAIL_CORS_ORIGINS` | empty (same origin only) | Comma-separated origins allowed cross-origin, for development. |
| `NSHMAIL_ALLOW_SIGNUP` | `true` | `false` closes sign-up. |
| `NSHMAIL_MAX_ATTACHMENTS_MB` | `25` | Most a message's attachments may add up to (1–1024); staged uploads (`uploads/`) expire after a day. |
| `NSHMAIL_GOOGLE_CLIENT_ID` / `_SECRET` | unset | Google OAuth web client; redirect URI `<PUBLIC_URL>/v1/oauth/google/callback`. |
| `NSHMAIL_MS_CLIENT_ID` / `_SECRET` | unset | Microsoft OAuth web app; redirect URI `<PUBLIC_URL>/v1/oauth/microsoft/callback`. |
| `NSHMAIL_FCM_SERVICE_ACCOUNT` | unset (no FCM) | Path to the Firebase service account JSON. |
| `NSHMAIL_VAPID_PRIVATE_KEY` | generated in data dir | Base64url 32-byte P-256 private key for Web Push. |
| `NSHMAIL_VAPID_SUBJECT` | `NSHMAIL_PUBLIC_URL` | `mailto:` or `https:` contact sent to push services (Apple rejects `http:`). |
| `RUST_LOG` | `info` | Log filter. Paths only, never query strings or bodies. |

## API additions

- `GET /v1/me`, `PATCH /v1/me {hideContent}` — notification privacy: when on, pushes carry only account and message ids.
- `GET /v1/detect?email=` — provider presets and Mozilla autoconfig for the add-account form (`null` when unknown).
- `POST /v1/sync` — sync every account now (pull to refresh).
- `GET /v1/events?device=<id>` — while this socket is open, that device gets no push.
- `GET /v1/oauth/{provider}/start?token=…&return=native|web` — ends at `nshmail://oauth-done?account=<id>` or `<PUBLIC_URL>/?oauth-done&account=<id>`; failures carry `error=<message>`.
- Push data: `{type:"newMail", account, id, from?, subject?}`.
- `GET|POST /v1/sources`, `GET|PATCH|DELETE /v1/sources/{id}`, `POST /v1/sources/{id}/token`, `POST /v1/sources/{id}/test` — notification sources (see [docs/design/notification-inbox.md](../../docs/design/notification-inbox.md)). A mail account is a `kind: "mail"` source created and deleted with the account. Creating a webhook/Gotify/ntfy source answers `{source, token, ingestUrl}` once; `/token` rotates it and the old URL stops working. Every mail message is also projected into the feed (`feed_messages`) by triggers in `mail_core`, so list and search behave the same for mail and for ingested notifications.

## Ingestion

Notifications arrive on URLs outside `/v1`; each one is a source, and the token in the URL is shown once:

| Front end | URL | Notes |
|---|---|---|
| Webhook | `POST /hooks/{token}` | JSON or form; `config.preset` (`github`, `gitlab`, `sentry`, `grafana`, `uptime-kuma`) or `config.mapping` (`sender`, `title`, `body`, `bodyHtml`, `link`, `priority`, `tags`, `externalId` as a path, `a || b` chain, `{{template}}` or literal). Answers `202 {"id": …}`. |
| Gotify | `POST /message?token={token}` | Gotify's JSON in, Gotify's JSON out. |
| ntfy | `PUT|POST /{topic}` | The topic is the token; ntfy headers, JSON publish or plain body. `GET /{topic}/json` streams NDJSON, replaying from `?since=<seq>`. |

Limits per source: 256 KiB a request, 60 messages a minute with a burst of 120. A source's `secret` (write-only, `hasSecret` in responses) turns on the `X-Hub-Signature-256` check for its webhook; it is stored AES-256-GCM in `users/notify.secrets.json`, which key rotation covers like the mail secrets. Ingested HTML goes through the same sanitizer as mail, with remote images removed; `externalId` makes a sender's retry a no-op; duplicates still answer `202` with the existing id.

## Notification feed

`server.sqlite` gains `sources` and `feed_messages` at start; the migration is idempotent and verifies that the projection holds exactly one row per mail message (drafts and trash excluded). The projection is derived data:

```sh
docker compose exec app nsh-mail-server rebuild-feed   # prints counts as JSON
```

Use it if the feed is ever wrong; mail rows are only read. `NSHMAIL_DATA_DIR` selects the database.
