English | [简体中文](README.zh-CN.md)

# nsh-notify-mcp

A [Model Context Protocol](https://modelcontextprotocol.io) server for the nsh-mail **notification inbox**: mail, webhooks (GitHub, GitLab, Sentry, Grafana, Uptime Kuma), Gotify, ntfy and chat bots all land in one feed, and an agent reads it with a single API key that starts with `nb_`.

It is a thin, honest client: five tools, two transports (stdio and streamable HTTP), and the inbox server's own error messages passed through unchanged.

## Tools

| Tool | Arguments | What it does |
|---|---|---|
| `list_unread` | `{ source?, limit? }` | Unread notifications, newest first; bodies shortened, `get` has the full text |
| `search` | `{ q, source?, limit? }` | Full-text search across every source, archived included |
| `get` | `{ id }` | One notification in full |
| `mark_read` | `{ id }` | Marks it read; needs an API key with the **write** scope |
| `list_sources` | `{}` | The sources this key can read — id, kind, name |

`limit` is capped at 50 and defaults to 20. When more match, the result carries a `next` cursor and says so.

## Build

```sh
cargo build -p nsh-notify-mcp --release     # target/release/nsh-notify-mcp
# or
cargo install --path crates/mcp
```

## Run: stdio (what MCP clients launch)

```sh
nsh-notify-mcp --server https://mail-api.bangwu.me --key nb_xxxxxxxx
```

| Option | Environment | Meaning |
|---|---|---|
| `--server URL` | `NSH_NOTIFY_SERVER` | Base URL of your nsh-mail server |
| `--key KEY` | `NSH_NOTIFY_KEY` | API key (`nb_…`), created in the app under Settings → API keys |
| `--timeout SECS` | | Give up on a REST call after SECS (1–300, default 15) |

stdout is the protocol and carries nothing else; logs go to stderr. Exit code 2 means bad arguments, 1 a failed transport.

## Run: HTTP

```sh
nsh-notify-mcp --http 127.0.0.1:8099 --server https://mail-api.bangwu.me --key nb_xxxxxxxx
```

`POST /mcp` takes one JSON-RPC message (or a batch) and answers `application/json`. It is stateless — no session id, no server-initiated stream — so `GET /mcp` answers 405. With `--key` set, requests must carry that key as `Authorization: Bearer …` or `X-API-Key`; without it, each request brings its own key.

```sh
curl -s http://127.0.0.1:8099/mcp \
  -H 'Authorization: Bearer nb_xxxxxxxx' \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

Put TLS in front of it (a reverse proxy) before it leaves localhost.

## Client configuration

### Claude Code

```sh
claude mcp add nsh-notify -- nsh-notify-mcp --server https://mail-api.bangwu.me --key nb_xxxxxxxx
# or keep the key out of the command line:
claude mcp add nsh-notify -e NSH_NOTIFY_KEY=nb_xxxxxxxx -- nsh-notify-mcp --server https://mail-api.bangwu.me
```

`.mcp.json`, in a project directory (shared) or at the top of your home directory:

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

`~/.codex/config.toml`:

```toml
[mcp_servers.nsh-notify]
command = "nsh-notify-mcp"
args = ["--server", "https://mail-api.bangwu.me", "--key", "nb_xxxxxxxx"]
```

### Cursor

`~/.cursor/mcp.json` (or `.cursor/mcp.json` in a project):

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

Once your server exposes `/mcp` itself, Cursor can skip the binary:

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

## API keys

Settings → API keys in the app: give the key a name, choose the scope (`read`, and `write` if the agent should mark things read), and limit it to some sources if you like. The key is shown once; store it in the client config above. Revoking it in the app stops the agent at its next call.

## Using it from the server

The crate is a library first (`notify_mcp`). The nsh-mail server can serve MCP from its own process, at the same URL and behind the same API-key check as the REST API:

```rust
let make: notify_mcp::BackendFactory =
    std::sync::Arc::new(move |_key: Option<&str>| backend.clone()); // your Arc<dyn Notify>
let app = app.merge(notify_mcp::router(make));
```

`notify_mcp::router` owns `POST /mcp` and does no authentication of its own — the mounting server has already checked the Bearer key. Two ready-made pieces:

- `notify_mcp::client_router(server, key, timeout)` — standalone HTTP transport over REST, what `--http` uses;
- the [`Notify`](src/api.rs) trait — implement it over the server's own store and the tools work without an HTTP hop.

## Tests

```sh
cargo test -p nsh-notify-mcp
```

The tests start stub inbox servers on random ports: each tool against them, cursor paging, 401 and 403 (scope) errors, a timeout, a real stdio session through the built binary, and the HTTP transport (including the mounted, no-auth-of-its-own router). No real server, key or network is needed.
