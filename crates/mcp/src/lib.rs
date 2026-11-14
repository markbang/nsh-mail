//! `nsh-notify-mcp` — the MCP server for the nsh-mail notification inbox.
//!
//! Tools: `list_unread`, `search`, `get`, `mark_read`, `list_sources`, exactly
//! as [docs/design/notification-inbox.md] §3 defines them. Two transports share
//! one dispatcher:
//!
//! - stdio ([`stdio::serve`]), what Claude Code, Codex and Cursor launch;
//! - streamable HTTP ([`router`]), which the nsh-mail server can mount as-is at
//!   `/mcp`, because the tools only need a [`Notify`] backend.
//!
//! [docs/design/notification-inbox.md]: https://github.com/markbang/nsh-mail/blob/main/docs/design/notification-inbox.md

pub mod api;
pub mod http;
pub mod protocol;
pub mod stdio;

pub use api::{BoxFuture, Error, ListQuery, Message, Notify, NotifyApi, Page, Source};
pub use http::{BackendFactory, client_router, router};
