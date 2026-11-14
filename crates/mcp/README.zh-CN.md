[English](README.md) | 简体中文

# nsh-notify-mcp

nsh-mail **通知收件箱**的 [Model Context Protocol](https://modelcontextprotocol.io) 服务器：邮件、Webhook（GitHub、GitLab、Sentry、Grafana、Uptime Kuma）、Gotify、ntfy 和聊天机器人全都汇聚到一个信息流里，agent 用一个以 `nb_` 开头的 API key 就能读取。

它只是一个薄而诚实的客户端：五个工具、两种传输（stdio 和 streamable HTTP），服务端返回的错误原样透出。

## 工具

| 工具 | 参数 | 作用 |
|---|---|---|
| `list_unread` | `{ source?, limit? }` | 未读通知，最新的在前；列表里正文会被缩短，完整正文用 `get` |
| `search` | `{ q, source?, limit? }` | 在所有来源里全文搜索，包含已归档的 |
| `get` | `{ id }` | 读取一条通知的完整内容 |
| `mark_read` | `{ id }` | 标记已读；需要带 **write** 权限的 API key |
| `list_sources` | `{}` | 这个 key 能读的来源：id、类型、名称 |

`limit` 最多 50，默认 20。匹配项更多时，结果里会带 `next` 游标并说明这一点。

## 构建

```sh
cargo build -p nsh-notify-mcp --release     # target/release/nsh-notify-mcp
# 或
cargo install --path crates/mcp
```

## 运行：stdio（MCP 客户端启动的就是这个）

```sh
nsh-notify-mcp --server https://mail-api.bangwu.me --key nb_xxxxxxxx
```

| 选项 | 环境变量 | 含义 |
|---|---|---|
| `--server URL` | `NSH_NOTIFY_SERVER` | 你的 nsh-mail 服务器地址 |
| `--key KEY` | `NSH_NOTIFY_KEY` | API key（`nb_…`），在 App 的 设置 → API keys 里创建 |
| `--timeout SECS` | | 单次 REST 请求的超时秒数（1–300，默认 15） |

stdout 只承载协议内容，日志一律走 stderr。退出码 2 表示参数有误，1 表示传输层失败。

## 运行：HTTP

```sh
nsh-notify-mcp --http 127.0.0.1:8099 --server https://mail-api.bangwu.me --key nb_xxxxxxxx
```

`POST /mcp` 接收一条 JSON-RPC 消息（或一批），返回 `application/json`。它是无状态的——没有会话 id，也没有服务端主动推送的流——所以 `GET /mcp` 返回 405。指定了 `--key` 时，请求必须带上同一个 key（`Authorization: Bearer …` 或 `X-API-Key`）；不指定时，每个请求自备 key。

```sh
curl -s http://127.0.0.1:8099/mcp \
  -H 'Authorization: Bearer nb_xxxxxxxx' \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

离开本机之前，请在前面加一层带 TLS 的反向代理。

## 客户端配置

### Claude Code

```sh
claude mcp add nsh-notify -- nsh-notify-mcp --server https://mail-api.bangwu.me --key nb_xxxxxxxx
# 或者不把 key 写在命令行里：
claude mcp add nsh-notify -e NSH_NOTIFY_KEY=nb_xxxxxxxx -- nsh-notify-mcp --server https://mail-api.bangwu.me
```

`.mcp.json`，放在项目目录（团队共享）或家目录顶层：

```json
{
  "mcpServers": {
    "nsh-notify": {
      "command": "nsh-notify-mcp",
      "args": ["--server", "https://mail-api.bangwu.me"],
      "env": { "NSH_NOTIFY_KEY": "nb_xxxxxxxx" }
    }
  }
}
```

### Codex

`~/.codex/config.toml`：

```toml
[mcp_servers.nsh-notify]
command = "nsh-notify-mcp"
args = ["--server", "https://mail-api.bangwu.me", "--key", "nb_xxxxxxxx"]
```

### Cursor

`~/.cursor/mcp.json`（或项目里的 `.cursor/mcp.json`）：

```json
{
  "mcpServers": {
    "nsh-notify": {
      "command": "nsh-notify-mcp",
      "args": ["--server", "https://mail-api.bangwu.me"],
      "env": { "NSH_NOTIFY_KEY": "nb_xxxxxxxx" }
    }
  }
}
```

等服务器自己提供 `/mcp` 之后，Cursor 可以不用这个可执行文件：

```json
{
  "mcpServers": {
    "nsh-notify": {
      "url": "https://mail-api.bangwu.me/mcp",
      "headers": { "Authorization": "Bearer nb_xxxxxxxx" }
    }
  }
}
```

## API key

在 App 的 设置 → API keys 里创建：起个名字，选权限（`read`，需要让 agent 标记已读时再加 `write`），也可以限定只能读某几个来源。key 只显示一次，请存进上面的客户端配置。在 App 里吊销后，agent 的下一请求就会被拒绝。

## 在服务器里复用

这个 crate 首先是库（`notify_mcp`）。nsh-mail 服务器可以在自己的进程里提供 MCP，与 REST API 同址，并共用同一套 API key 校验：

```rust
let make: notify_mcp::BackendFactory =
    std::sync::Arc::new(move |_key: Option<&str>| backend.clone()); // 你的 Arc<dyn Notify>
let app = app.merge(notify_mcp::router(make));
```

`notify_mcp::router` 只负责 `POST /mcp`，自身不做鉴权——挂载它的服务器已经校验过 Bearer key。现成的两块：

- `notify_mcp::client_router(server, key, timeout)`——独立的 HTTP 传输，走 REST，`--http` 用的就是它；
- [`Notify`](src/api.rs) trait——在服务器自己的存储上实现它，工具就不再需要绕一圈 HTTP。

## 测试

```sh
cargo test -p nsh-notify-mcp
```

测试会在随机端口上启动桩服务器：逐个工具、游标分页、401 与 403（权限）错误、超时、用编译出的可执行文件跑一次真实的 stdio 会话，以及 HTTP 传输（包括挂载模式、自身不鉴权的 router）。不需要真实的服务器、key 或网络。
