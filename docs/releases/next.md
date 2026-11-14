English | [简体中文](next.zh-CN.md)

## Added

- Conversations: replies are grouped with the message they answer, across your accounts, in one row that shows who wrote and how many messages it holds. A conversation opens on its latest message with the earlier ones collapsed above; archive, delete and mark as read act on all of it. Settings → Reading → Group by conversation turns it off.
- Undo send: after Send, a message waits a few seconds (Settings → Sending → Undo send: off, 5, 10, 20 or 30 s; 10 by default) with "Sending… Undo" at the bottom; Undo brings the composer back as it was. The wait is kept on the device (local mode) or on the server (cloud mode), so closing the app or the tab neither loses the message nor stops it.
- Schedule send: the arrow next to Send (the clock on phones) sends tomorrow morning, on Monday morning, or at a date and time you pick, in your time zone. Scheduled messages are listed under Scheduled, where you can edit or cancel them. In cloud mode the server sends them on time; in local mode they go out while nsh-mail runs (the tray keeps it running on desktops), and anything due while it was closed goes out at the next start, with a notice.
- Compose drafts are kept: closing the composer keeps what you wrote, also across restarts, and Discard asks before deleting it (#40).

## Improved

- Newsletters and other mail laid out for a fixed width scale to fit narrow panes and phones instead of scrolling sideways (#42).
- On phone-width browsers a 600px newsletter now fits without any sideways scroll: the reader drops its own padding around zoomed mail and keeps a slimmer margin.

## Fixed

- Servers with a certificate from a private CA connect once that CA is installed on the device: the system's certificate store is trusted besides the built-in roots (#41).
