# nsh-mail → 通知收件箱

[English](notification-inbox.md) | 简体中文

产品方向：nsh-mail 不再只是邮件客户端，而是**通知收件箱**——邮件、webhook、IM 机器人、Gotify/ntfy 脚本发来的一切都落到同一个地方，agent 用一个 API key 就能全部读走。在现有的 App、服务端和 `mail-core` 上演进，不推倒重来。

## 1. 数据模型

在同一个 `server.sqlite`（本地模式是 `mail.sqlite`）里新增两张表，都带 `user_id`，索引以 `user_id` 打头（#24 的规则）。

### `sources`（来源）

| 字段 | 说明 |
|---|---|
| `id` | 文本 id（`src_…`） |
| `user_id` | 归属 |
| `kind` | `mail`、`webhook`、`gotify`、`ntfy`、`telegram`、`discord`、`slack`、`feishu`、`dingtalk`、`wecom` |
| `name`、`icon`、`color` | App 里怎么显示 |
| `config` | JSON，按类型不同（IMAP 账号 id、字段映射、预设）；敏感部分用主密钥加密 |
| `ingest_token_hash` | `webhook`/`gotify`/`ntfy` 用：URL token 的 SHA-256，token 只在创建时显示一次 |
| `min_priority` | 低于它的消息照样入库但不推送（默认 0） |
| `enabled` | 暂停接收而不删除 |
| `created_at` | |

邮件账号就是一个 `kind = mail` 的 source，`config.accountId` 指向现有的邮件账号（id 为 `src_m_<账号 id>`，重建时落在同一行）。迁移时给已有账号补上 source；邮件代码路径不变。

### `feed_messages`（feed）

（初稿叫 `messages`，但同一个库里邮件表已经用了这个名字。）

| 字段 | 说明 |
|---|---|
| `id` | 文本 id；邮件消息的 id 就是邮件表里的 id，两表一一对应 |
| `user_id`、`source_id` | |
| `external_id` | 发送方的 id，同一 source 内唯一（重试去重） |
| `sender`、`title` | |
| `body_text`、`body_html`、`body_markdown` | 至少一个；HTML 和邮件一样清洗（脚本、事件属性去掉，远程图片默认拦截） |
| `link` | 用系统浏览器打开 |
| `received_at` | 服务端接收时间，毫秒时间戳（邮件的 `date` 本来就是毫秒） |
| `read_at`、`archived_at` | 未读 / 仍在 feed 里时为 null |
| `priority` | 0–10，与 Gotify/ntfy 同尺度 |
| `tags` | JSON 数组 |
| `payload` | 原始请求体，最多 64 KiB，便于排查 |
| `thread_id` | 仅邮件 |

索引：`(user_id, received_at DESC, id)`、`(user_id, source_id, received_at DESC)`、未读部分索引、去重用的唯一索引 `(source_id, external_id) WHERE external_id IS NOT NULL`，以及非邮件正文的第二张 FTS 表（`feed_messages_fts`）；邮件正文仍在邮件索引里。

`id` 对接收来的消息是 `msg_…`，对邮件就是邮件 id；`seq`（rowid）按用户单调递增，SSE 从它续传。

**邮件是投影，不是搬家。** 邮件保留自己的表（正文、标记、线程、水位）。`accounts`、`folders`、`messages` 上的触发器在同一个事务里写对应的 `feed_messages` 行——所以邮件代码路径不变——草稿箱和垃圾箱不投影（它们不是通知）——统一 feed 用的投影副本，这样 feed 的分页、筛选和 SSE 都是一条查询，邮件代码完全不动。投影可重建（`nsh-mail-server rebuild-feed`），所以出 bug 也不会丢邮件。代价是每封邮件多一行（约 120 字节加文本），换来统一的 feed。

## 2. 入口

### Webhook（`kind = webhook`）

`POST /hooks/{token}`，一个 source 一个 URL，token 32 字节随机数，库里只有哈希。

- 请求体：JSON 或表单；`?format=` 可强制指定。表单会变成一个扁平对象，所以同一套路径都能用。
- 映射：source 的 `config.mapping` 里每个字段一条表达式——`sender`、`title`、`body`、`bodyHtml`、`link`、`priority`、`tags`、`externalId`。表达式可以是路径（`$.issue.title`、`.a.b[0].c`、`$['a']`）、回退链（`a || b`，取第一个有值且非空的）、模板（`{{repository.full_name}}: {{action}}`）或字面量（`3`）。路径只做普通取值，没有过滤器和通配符。`priority` 取 0–10，也可以给级别（`error`、`warning`、`info`、`debug`、`critical`、`alerting`、`ok`）。
- 没配置字段时的默认值：`title` 取 `title`/`subject`/`message`/`text`/`name` 里第一个；`sender` 取 `sender`/`from`/`author`/`app`/`service`/`user`；`link` 取 `url`/`link`/`html_url`/`permalink`；`externalId` 取 `id`/`event_id`/`guid`/`uuid`/`fingerprint`；`priority` 取 `priority` 或级别；`body` 是 JSON 其余部分的 pretty 文本。
- 预设（内置可改，`GET /v1/sources/presets`）：GitHub（issue、PR、Actions、release）、GitLab（pipeline、MR）、Sentry、Grafana、Uptime Kuma。source 用 `config.preset` 选一个，`config.mapping` 覆盖单个字段。
- 可选签名校验：source 配了 secret 时，`X-Hub-Signature-256`（GitHub 风格 HMAC-SHA256，对原始请求体）必须匹配，否则 `401`。
- 返回 `202 {"id": "…"}`（`externalId` 重复时同样返回、不入库）；`401` token 或签名错、`403` source 已暂停、`413` 超过 256 KiB、`429` 超过每 source 限流（60/分，突发 120）。

### Gotify 兼容（`kind = gotify`）

`POST /message?token={token}`，请求体用 Gotify 的 JSON（`title`、`message`、`priority`、`extras`），返回 Gotify 的结构（`id`、`appid`、`message`、`title`、`priority`、`date`）；`id` 是 feed 里按用户的 `seq`，`date` 是 RFC 3339。`extras["client::display"].contentType = "text/html"` 时按清洗后的 HTML 存储。现有 `curl` 脚本和 Gotify 客户端改个地址就能用。

### ntfy 兼容（`kind = ntfy`）

`PUT|POST /{topic}`，topic 就是 source 的 token，用 ntfy 的请求头（`Title`、`Priority`、`Tags`、`Click`、`X-Title`、`X-Message`）、JSON 发布格式或直接正文，返回 ntfy 的消息 JSON。ntfy 的 1–5 优先级在 feed 里是 2–10。`GET /{topic}/json` 按 ntfy 的方式输出 NDJSON：先用 `?since=<seq>` 补发，然后跟随该 source 的实时消息，每 30 秒一个空行。`ntfy publish` 改个地址即可。

### IM 来源（`telegram`、`discord`、`slack`、`feishu`、`dingtalk`、`wecom`）

只用官方 bot API，每种来源各自带凭据：

| 类型 | 收到什么 | 方式 |
|---|---|---|
| `telegram` | 发给 bot 的消息、bot 所在群的消息 | Bot API `getUpdates` 长轮询（不需要公网 webhook） |
| `discord` | bot 能读的频道消息、私信 | Gateway（WebSocket）`MESSAGE_CREATE` |
| `slack` | 拉 bot 进群的频道消息、私信 | Events API + Socket Mode |
| `feishu` | 发给 bot 的消息、群消息 | 事件订阅（长连接） |
| `dingtalk` | 群里 @bot 的消息 | Stream 模式 |
| `wecom` | 发给应用的消息、群消息 | 回调（应用的 token/aes key） |

个人账号全量收取（读你自己的账号，而不是 bot 的）不在这一期；以后走 Matrix 桥。每种 IM 来源用自己的 payload，经同一套映射引擎。

## 3. 给 agent 的出口

### API key

| 字段 | 说明 |
|---|---|
| `id`、`user_id`、`name` | |
| `hash` | 32 字节 token 的 SHA-256，token 只显示一次 |
| `prefix` | 前 8 个字符，列表里辨认用 |
| `scopes` | `read`、`write`（标已读 / 归档） |
| `source_ids` | 为空表示全部 source，否则只这些 |
| `expires_at`、`last_used_at`、`revoked_at` | |

`Authorization: Bearer nb_…`（或 `X-API-Key`）。key 只用于新接口，App 自己的会话鉴权不变。

### REST

```
GET  /v1/messages?unread=1&source=src_…&since=<rfc3339|id>&q=…&limit=50&cursor=…
GET  /v1/messages/{id}
POST /v1/messages/{id}/read        POST /v1/messages/{id}/archive
GET  /v1/sources                   GET /v1/sources/{id}
POST /v1/sources                   PATCH|DELETE /v1/sources/{id}
POST /v1/sources/{id}/token        POST /v1/sources/{id}/test
GET  /v1/sources/presets
```

source 的 secret（webhook 的 HMAC 密钥）只写不读：新建或修改时传 `secret`（空串表示清除），返回里只有 `hasSecret`，主密钥 AES-256-GCM 加密存在 `users/notify.secrets.json`，现有密钥轮换已覆盖。`POST /v1/sources` 只返回一次 `{"source": …, "token": "…", "ingestUrl": "…"}`；`POST /v1/sources/{id}/token` 轮换后旧地址立刻失效；`POST /v1/sources/{id}/test` 通过与接收相同的路径写入一条测试消息。

cursor 不透明（`received_at` + `id` 的 base64，keyset 分页）。`q` 走 FTS 索引，覆盖邮件和非邮件正文。所有查询都按 key 的 source 范围过滤。

返回结构：`GET /v1/messages` 返回 `{messages: [Message], next_cursor?}`（最后一页没有游标）；`GET /v1/messages/{id}` 返回一个 `Message`；`read` 和 `archive` 返回更新后的 `Message` 或 204；`GET /v1/sources` 返回 `{sources: [Source]}`。`Message` 为 `{id, source_id, sender, title, body_text, link, received_at, priority, tags, read_at, archived_at, thread_id?}`，`Source` 为 `{id, kind, name, enabled, min_priority}`。客户端会忽略不认识的字段，所以两者都能扩展而不影响已装好的 agent。

### 实时

`GET /v1/stream`（SSE）：事件 `message`（JSON 与 `/v1/messages/{id}` 相同）、`read`、`archived`，每 15 秒心跳注释。`Last-Event-ID` 从最后收到的 id 续传（id 按用户单调递增），`?since=` 用于冷启动；`?source=` 收窄；key 的范围始终生效。另提供 WebSocket 变体（`GET /v1/stream?format=ws`）。

### MCP

独立的 `crates/mcp`（`nsh-notify-mcp`），工具：

| 工具 | 参数 |
|---|---|
| `list_unread` | `{ source?, limit? }` |
| `search` | `{ q, source?, limit? }` |
| `get` | `{ id }` |
| `mark_read` | `{ id }`（write 权限） |
| `list_sources` | `{}` |

两种传输：stdio（`nsh-notify-mcp --server https://… --key nb_…`）和服务端 HTTP（`/mcp`，同一个 Bearer key）。README 给出 Claude Code、Codex、Cursor 的接入配置。

这个 crate 既是可执行文件也是库：`notify_mcp::router(factory)` 返回可挂载的 `POST /mcp`，自身不鉴权（由挂载它的服务器校验 key），`Notify` trait 让服务器直接用自己的存储回答工具调用，不必绕一圈 HTTP。独立可执行文件是同一个分发器，只是 `Notify` 走 REST。

### OpenAPI

由路由定义自动生成（`utoipa`），`/openapi.json` 提供，`/docs` 渲染；CI 把它提交到 `docs/openapi.json`，agent 不连服务端也能读。

## 4. 给人的界面

- 邮件收件箱旁边新增**全部通知**视图：所有来源一条 feed，带来源图标和颜色、优先级和标签；可按来源筛选；跨全部内容搜索。
- 设置里的**来源**：添加 webhook / Gotify / ntfy / IM 来源，复制接收 URL（或 token），发送测试消息，暂停，删除。
- 设置里的 **API key**：创建（名称、权限、可访问来源、过期），只显示一次，可看最近使用，可撤销。
- **通知**：按来源和优先级设置提醒 / 静默 / 关闭，叠加在现有的按账号邮件通知设置之上。
- 邮件保留现有界面；只有邮件的用户默认仍是邮件收件箱。

## 5. 迁移与线上安全

- 只新增表，不改现有字段含义。邮件投影按用户一个事务写入并核对条数（同 #24 的做法），`users/*.sqlite` 导入先跑。
- 每次涉及这些的部署前都备份数据卷（沿用现有流程：停应用 → 手动备份 schedule → 在副本上试跑 → 部署）。
- 现有客户端不受影响：`/v1/accounts`、`/v1/messages`、`/v1/threads` 不变，新接口是新增。
- 入口有限流和大小上限；`payload` 有上限；HTML 清洗；token 和 API key 只存哈希。

## 6. 命名

产品不再只是邮件，候选名（2026-11-14 查了 npm、GitHub 同名仓库、域名 RDAP）：

| 名称 | npm | GitHub 同名仓库 | 可注册域名 |
|---|---|---|---|
| **AgentInbox** | 空闲 | 23 | `agentinbox.io` |
| PingSink | 空闲 | 0 | `pingsink.io` |
| NotifyInbox | 空闲 | 0 | `notifyinbox.dev` |

推荐 **AgentInbox**：既说明谁在读（agent），也说明它是什么（收件箱），`agentinbox.io` 和 npm 名都空着。改仓库名和 App 名称需要用户确认；在此之前代码、包名（`top.bangwu.nshmail`）、镜像名和 URL 都不动。API 路径里不含产品名，改名时不会破坏任何东西。

## 7. 里程碑

1. **M1——可用的通知收件箱**：模型 + 邮件投影 + 迁移；带预设的 webhook；Gotify 和 ntfy 兼容；API key；`GET /v1/messages`、已读/归档、sources；SSE；MCP（stdio + HTTP）；OpenAPI；App 的「全部通知」视图和来源 / API key 设置。作为 v0.0.4 发布并宣传。
2. **M2——IM 来源**：Telegram、Discord、Slack、飞书、钉钉、企业微信，各一个 PR（需要的应用注册和 key 会单独提醒）。
3. **M3——规则**：过滤器（来源/标签/标题正则 → 优先级、标签、静音）、摘要、Matrix 桥。
