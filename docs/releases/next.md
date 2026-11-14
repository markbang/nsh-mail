English | [简体中文](next.zh-CN.md)

## Added

- An MCP server for agents (`nsh-notify-mcp`): list unread notifications, search across every source, read one in full, mark it read and list sources, over stdio or HTTP with an API key. Config blocks for Claude Code, Codex and Cursor are in `crates/mcp/README.md`.
- Notification inbox, foundations: mail now appears in one unified feed next to every other source, and a server API manages those sources (create, rename, pause, delete), each with an ingest URL whose token is shown once and can be rotated. Webhook, Gotify and ntfy ingestion follows in the next change; the app's feed comes after that.
- Signatures, one per account (Settings → the account's menu → Signature), with **bold**, *italic* and links. Added to new messages and to replies and forwards (each can be turned off), above the quoted message as Gmail does, and sent as plain text and HTML. Changing From in the composer swaps it, unless you edited it. Kept on the device in local mode, on the server in cloud mode.

## Improved

- Keyboard and screen readers. Every control is a Tab stop with Material's focus ring and presses with Return or Space, and `?` — or Settings → Help — lists the shortcuts, each key as your platform names it. Screen readers hear the snackbar as an alert for failures and a status for confirmations, the mailbox, search, sidebar and toolbar names, and rows as unread, sender, count, subject and date.
- Large mailboxes: the first sync now arrives in pages, so the inbox opens while the rest is still coming, and its progress shows in the mailbox rail. If it stops midway, the next sync continues from where it stopped instead of starting over. A new "load older mail" path pages further back as you scroll to the end of a folder, and that older mail is searchable once loaded.
- The list stays smooth in a large mailbox. Scrolling and loading older mail recycle the rows instead of rendering the whole mailbox, and a sync now costs the same however far down you are: new mail appears in the first page and the rows below keep their place (before, a sync while scrolled deep reloaded and redrew everything, one 350 ms stall on the web with 10,000 messages loaded — now 50 ms). Paging past 500 conversations no longer cuts the list back to the first page. A conversation of 200 messages opens on its first and last few; **More** reveals 25 at a time, and **Show less** folds them back.
- Empty, loading and failure states across the app. An empty folder says which one it is and offers New message where that helps; a search shows a hint before you type, a loading indicator while it runs, and the reason with Try again if it fails. A mailbox that won't load, a message that won't open and a failed sync all say what to do next, in the list, in the reader and in a banner with a retry. An IMAP sign-in the server refuses now explains app passwords, with a link for the provider.

## Fixed

- Settings' About line showed a version number that never changed; it now shows the one the build reports.
