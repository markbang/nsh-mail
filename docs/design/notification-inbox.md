# nsh-mail → notification inbox

Product direction: nsh-mail stops being only a mail client and becomes a **notification inbox**: everything that wants to reach you — mail, webhooks, IM bots, Gotify/ntfy scripts — lands in one place, and an agent can read all of it with one API key. This builds on the existing app, server and `mail-core`; nothing is rewritten.

Design documents are English here; the Chinese version is [notification-inbox.zh-CN.md](notification-inbox.zh-CN.md).

## 1. Model

Two new tables in the same `server.sqlite` (local mode: `mail.sqlite`), both carrying `user_id` with indexes leading with it (the rule from #24).

### `sources`

| Column | Notes |
|---|---|
| `id` | text id (`src_…`) |
| `user_id` | owner |
| `kind` | `mail`, `webhook`, `gotify`, `ntfy`, `telegram`, `discord`, `slack`, `feishu`, `dingtalk`, `wecom` |
| `name`, `icon`, `color` | how the app shows it |
| `config` | JSON, per kind (IMAP account id, mapping rules, presets); secret parts encrypted with the master key |
| `ingest_token_hash` | for `webhook`, `gotify`, `ntfy`: SHA-256 of the URL token; the token itself is shown once |
| `min_priority` | messages below it are stored but not pushed (default 0) |
| `enabled` | pause ingestion without deleting |
| `created_at` | |

A mail account is a source of kind `mail` whose `config.accountId` points at the existing mail account (its id is `src_m_<account id>`, so a rebuild finds the same row). Existing mail accounts are given sources by the migration; the mail code path is unchanged.

### `feed_messages` (the feed)

(Called `messages` in the first draft; the mail tables already use that name in the same database.)

| Column | Notes |
|---|---|
| `id` | text id; for mail it is the mail message id, so the two tables stay 1:1 |
| `user_id`, `source_id` | |
| `external_id` | sender's id, unique per source when present (dedup on retries) |
| `sender`, `title` | |
| `body_text`, `body_html`, `body_markdown` | at least one; HTML sanitized exactly like mail (scripts, event handlers, remote images blocked by default) |
| `link` | opens in the system browser |
| `received_at` | when the service accepted it, milliseconds since the epoch (mail's `date` is already ms) |
| `read_at`, `archived_at` | null while unread / in the feed |
| `priority` | 0–10, Gotify/ntfy compatible scale |
| `tags` | JSON array |
| `payload` | raw request body, capped at 64 KiB, for debugging |
| `thread_id` | mail only |

Indexes: `(user_id, received_at DESC, id)`, `(user_id, source_id, received_at DESC)`, `(user_id) WHERE read_at IS NULL`, a unique `(source_id, external_id) WHERE external_id IS NOT NULL` for dedup, and a second FTS table (`feed_messages_fts`) for non-mail text; mail text stays in the mail index.

`id` is `msg_…` for ingested messages and the mail message id for mail; `seq` (the row's rowid) is monotonic per user and is what SSE resumes from.

**Mail is projected, not moved.** Mail keeps its own tables (bodies, flags, thread keys, watermarks). Triggers on `accounts`, `folders` and `messages` write the matching `feed_messages` row in the same transaction, so the mail code path is unchanged; drafts and trash are not projected (they are not notifications) — a projected copy used by the unified feed, so the feed's paging, filters and SSE are one query and mail code stays untouched. A mail message that sits in Archive is projected with `archived_at` set, because archiving is how a notification is handled; that keeps the app's archive and an agent's archive the same thing. The projection is rebuildable (`nsh-mail-server rebuild-feed`), so a bug cannot lose mail. Tradeoff: one extra row per mail message (≈120 bytes plus text), accepted for a uniform feed.

## 2. Ingestion

### Webhook (`kind = webhook`)

`POST /hooks/{token}` — one URL per source, token 32 random bytes, only its hash stored.

- Body: JSON (`application/json`) or form-encoded; `?format=` overrides detection. Form bodies become a flat object, so the same paths work.
- Mapping: one expression per field — `sender`, `title`, `body`, `bodyHtml`, `link`, `priority`, `tags`, `externalId` — set in the source's `config.mapping` by the app. An expression is a path (`$.issue.title`, `.a.b[0].c`, `$['a']`), a fallback chain (`a || b`, first that resolves and is not empty), a template (`{{repository.full_name}}: {{action}}`), or a literal (`3`). Paths are plain lookups: no filters or wildcards. `priority` takes 0–10, or a level (`error`, `warning`, `info`, `debug`, `critical`, `alerting`, `ok`).
- Defaults when a field is not mapped: `title` = first of `title`/`subject`/`message`/`text`/`name`; `sender` = `sender`/`from`/`author`/`app`/`service`/`user`; `link` = `url`/`link`/`html_url`/`permalink`; `externalId` = `id`/`event_id`/`guid`/`uuid`/`fingerprint`; `priority` = `priority` or a level; `body` = the rest of the JSON as pretty text.
- Presets (shipped, editable, `GET /v1/sources/presets`): GitHub (issues, PRs, Actions, releases), GitLab (pipelines, MRs), Sentry (issue alerts), Grafana (alerting), Uptime Kuma. A source names one with `config.preset`; `config.mapping` overrides individual fields.
- Optional signature check: when a source has a secret, `X-Hub-Signature-256` (GitHub style HMAC-SHA256 of the raw body) must match; otherwise the request is refused with `401`.
- Response `202 {"id": "…"}` (also for a repeated `externalId`, which stores nothing); `401` bad token or signature, `403` paused source, `413` body over 256 KiB, `429` over the per-source rate limit (60/min, burst 120).

### Gotify compatible (`kind = gotify`)

`POST /message?token={token}` with Gotify's JSON body (`title`, `message`, `priority`, `extras`) answering Gotify's shape (`id`, `appid`, `message`, `title`, `priority`, `date`); `id` is the feed's per-user `seq` and `date` is RFC 3339. `extras["client::display"].contentType = "text/html"` stores the message as sanitized HTML. Existing `curl` scripts and Gotify's own clients work by changing the URL.

### ntfy compatible (`kind = ntfy`)

`PUT|POST /{topic}` where the topic *is* the source token, with ntfy headers (`Title`, `Priority`, `Tags`, `Click`, `X-Title`, `X-Message`), the JSON publish form, or the body as the message; it answers ntfy's message JSON. ntfy priorities 1–5 become 2–10 in the feed. `GET /{topic}/json` streams newline-delimited JSON as ntfy does: it replays from `?since=<seq>` and then follows the source live, with an empty line every 30 s. Changing `ntfy publish` calls to this host is enough.

### IM sources (`telegram`, `discord`, `slack`, `feishu`, `dingtalk`, `wecom`)

Official bot APIs only; each is a source with its own credentials:

| Kind | What we receive | How |
|---|---|---|
| `telegram` | messages sent to the bot or in groups it is in | Bot API long polling `getUpdates` (no public webhook needed) |
| `discord` | messages in channels the bot can read, DMs | Gateway (WebSocket) with `MESSAGE_CREATE` |
| `slack` | messages in channels the app is invited to, DMs | Events API over Socket Mode |
| `feishu` | messages to the bot, group messages | Event subscription (long connection) |
| `dingtalk` | group messages where the bot is mentioned | Stream mode |
| `wecom` | messages to the app, group messages | Callback with the app's token/aes key |

Personal-account mirroring (reading *your* accounts rather than a bot's) is out of scope; a Matrix bridge is the future path. Each IM source maps its payload through the same mapping engine as webhooks.

## 3. Output for agents

### API keys

| Column | Notes |
|---|---|
| `id`, `user_id`, `name` | |
| `hash` | SHA-256 of a 32-byte token; the token is shown once |
| `prefix` | first 8 characters, for recognising it in a list |
| `scopes` | `read`, `write` (mark read / archive) |
| `source_ids` | empty = every source; otherwise only these |
| `expires_at`, `last_used_at`, `revoked_at` | |

`Authorization: Bearer nb_…` (or `X-API-Key`). Keys authenticate only the feed endpoints, the stream and `/mcp`; the app's own session auth is unchanged, and the same routes accept either, so an agent and the app can never disagree about what the feed holds.

A key is `nb_` and 32 random bytes, `prefix` is its first 8 characters. Every key can read; `scopes` holds `write` for keys that may mark notifications read or archive them. `last_used_at` is stamped at most once a minute, so a polling agent does not turn every request into a write. `POST /v1/keys` answers the token once and it is never stored in the clear; revocation takes effect on the next request.

### REST

```
GET  /v1/feed?unread=1&source=src_…&since=<rfc3339|ms|id>&q=…&limit=50&cursor=…
GET  /v1/feed/{id}
POST /v1/feed/{id}/read            POST /v1/feed/{id}/archive
POST /v1/feed/read                 POST /v1/feed/archive            (batches, for the app)
GET  /v1/keys                      POST /v1/keys
DELETE /v1/keys/{id}
GET  /v1/sources                   GET /v1/sources/{id}
POST /v1/sources                   PATCH|DELETE /v1/sources/{id}
POST /v1/sources/{id}/token        POST /v1/sources/{id}/test
GET  /v1/sources/presets
```

The feed is `/v1/feed`, not `/v1/messages`: that path is the existing mail list of one folder and §5 keeps it unchanged, while the feed spans every source. `/v1/keys` and the write side of `/v1/sources` take only the app's session; everything an agent needs takes either.

A source's secret (the webhook HMAC key) is write-only: sent as `secret` on create or patch (an empty string clears it), reported only as `hasSecret`. It is stored AES-256-GCM under the master key in `users/notify.secrets.json`, which the key rotation already covers. `POST /v1/sources` answers `{"source": …, "token": "…", "ingestUrl": "…"}` once; `POST /v1/sources/{id}/token` rotates the token and answers the same, so the old URL stops working immediately. `POST /v1/sources/{id}/test` stores a test message through the ingestion path. A key gets its `Source` without `config`, which holds the mapping and may name a bot's credentials.

Cursors are opaque (base64 of `received_at` + `id`, keyset paging on `(received_at DESC, id DESC)`). `before=<id>` continues from a message the caller already has; `since=` takes an RFC 3339 time, milliseconds since the epoch, or a message id, and means "received after". `q` uses both full-text indexes, so one search covers mail and everything else. `archived=0` (the default view), `1` (only archived) or `any`. Every query is filtered by the key's sources.

Shapes: `GET /v1/feed` answers `{messages: [Message], nextCursor?, more, unread, counts}` (`nextCursor` is absent on the last page; `counts` is unread per source id over everything the caller can see); `GET /v1/feed/{id}` answers a `Message`; `read` answers the updated `Message`; `archive` answers 204, since a mail notification is moved by the mail engine and has no row left to answer with. `POST /v1/feed/read` takes `{ids, read?}` and `POST /v1/feed/archive` takes `{ids}` (at most 200), the app's list operations. A `Message` is `{id, seq, sourceId, externalId?, sender, title, bodyText, bodyHtml, bodyMarkdown, link, receivedAt, readAt, archivedAt, priority, tags, threadId?}` and a `Source` `{id, kind, name, enabled, minPriority, icon?, color?, createdAt, hasToken, hasSecret}`. Fields are camelCase like the rest of `/v1` (the app reads the same JSON); `receivedAt`/`readAt`/`archivedAt` are milliseconds since the epoch. Clients ignore unknown fields, so both can grow without breaking installed agents, and the MCP client also accepts the snake_case spelling.

Archiving is one path for the app and for agents: a mail notification is moved to Archive by the mail engine (the row then re-projects there **archived**, see §1), and anything else gets `archived_at` on its feed row. Either way it leaves the default feed view and a matching `archived` event goes out.

### Streaming

`GET /v1/stream` (SSE): events `message` (the same JSON as `/v1/feed/{id}`), `read` (`{id, readAt}`) and `archived` (`{id, archivedAt}`), heartbeat comment every 15 s. `message` events carry the feed's `seq` as their event id (monotonic per user) and `Last-Event-ID` resumes after the last one; a cold start uses `?since=<seq>`. `read` and `archived` are live-only, so a client that reconnects reloads the feed instead of replaying them. `?source=` narrows the stream; the key's scope always applies, and events of other users' feeds never leave the server. A WebSocket variant (`GET /v1/stream?format=ws`) sends the same events as JSON with a `type` field.

### MCP

A separate crate `crates/mcp` (`nsh-notify-mcp`), tools:

| Tool | Arguments |
|---|---|
| `list_unread` | `{ source?, limit? }` |
| `search` | `{ q, source?, limit? }` |
| `get` | `{ id }` |
| `mark_read` | `{ id }` (write scope) |
| `list_sources` | `{}` |

Two transports: stdio (`nsh-notify-mcp --server https://… --key nb_…`) and HTTP (streamable HTTP at `/mcp` on the server, same Bearer key). README shows the exact config block for Claude Code, Codex and Cursor.

The crate is a library as much as a binary: `notify_mcp::router(factory)` returns a mountable `POST /mcp` that does no auth of its own (the mounting server's key check applies), and the `Notify` trait lets the server answer the tools from its own store without an HTTP hop. The standalone binary is the same dispatcher with a REST-backed `Notify`. The server mounts that router at `/mcp` behind its key check and answers from its store with the key's scope applied, so an agent that talks to `/mcp` needs no second address.

### OpenAPI

Generated from the route definitions (`utoipa`), served at `/openapi.json` and rendered at `/docs` (Swagger UI, its assets vendored, so a server with no internet still documents itself). `nsh-mail-server openapi` prints the document and the same bytes are committed to `docs/openapi.json`, so an agent can read the API without a server; a test fails when the file and the routes drift apart.

## 4. For people: the app

- A new **All notifications** view beside the mail inbox: one feed of every source, with the source's icon and colour, priority and tags; filter by source; search across everything.
- **Sources** in Settings: add a webhook / Gotify / ntfy / IM source, copy its ingest URL (or token), send a test message, pause it, delete it.
- **API keys** in Settings: create (name, scope, sources, expiry), copy once, see last use, revoke.
- **Notifications**: per source and priority — notify / silent / off, on top of the existing per-account mail settings.
- Mail keeps its current screens; the mail inbox stays the default view for people who only have mail.

## 5. Migration and production safety

- New tables only; no column of an existing table changes meaning. Mail rows are projected in one transaction per user with counts checked (like #24), and `users/*.sqlite` imports run first.
- Backup the volume before every deploy that touches this (existing procedure: stop, schedule `nsh-mail data backup (manual)`, dry-run on the copy, deploy).
- Existing clients keep working: `/v1/accounts`, `/v1/messages`, `/v1/threads` unchanged. The new endpoints are additive, and `/v1/messages` keeps its meaning (mail in one folder) — the agent feed is `/v1/feed` (§3).
- The feed migration is versioned (`feed_meta`); version 2 marks mail that is already in Archive as archived in the feed, so a mailbox migrated later does not look like a pile of new notifications.
- Rate limits and size caps on ingress; `payload` capped; HTML sanitized; tokens and API keys only stored hashed.

## 6. Naming

The product stops being just mail. Candidates (checked 2026-11-14: npm registry, GitHub repository names, RDAP for the domains):

| Name | npm | GitHub repos named so | Domains free |
|---|---|---|---|
| **AgentInbox** | free | 23 | `agentinbox.io` |
| PingSink | free | 0 | `pingsink.io` |
| NotifyInbox | free | 0 | `notifyinbox.dev` |

Recommendation: **AgentInbox** — it says who reads it (agents) and what it is (an inbox), and the `.io` domain and npm name are free. Renaming the repository and the app needs the user's confirmation; until then the code, package id (`top.bangwu.nshmail`), image name and URLs stay as they are. No product name appears in API paths, so nothing breaks at rename time.

## 7. Milestones

1. **M1 — "notification inbox" usable**: model + mail projection + migration; webhook with presets; Gotify and ntfy compatibility; API keys; `GET /v1/messages`, read/archive, sources; SSE; MCP (stdio + HTTP); OpenAPI; the app's All view and the Sources / API keys settings. Release as v0.0.4 and promote it.
2. **M2 — IM sources**: Telegram, Discord, Slack, Feishu, DingTalk, WeCom, one PR each (ping attention from the PR worker for API keys/apps each one needs).
3. **M3 — rules**: filters (source/tag/title regex → priority, tags, mute), digest summaries, Matrix bridge.
