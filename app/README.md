# nsh-mail app

The client: TSX screens over a Rust crate that exports `nsh-mail/native` (the `Mail` module of
[docs/design.md](../docs/design.md) §3). Design system and motion: [DESIGN.md](DESIGN.md).

```
App.tsx          shell: onboarding, three panes (wide) or a stack (phones), keys, toast
store.ts         TSX state: mirrors of Rust's status, accounts and messages; mailbox, selection, search
nav.ts           the phone stack and the wide layout's dialog stack
resident.ts      tray, close-to-tray, new-mail notifications
pages/           one NavPage each
components/      what pages share (message list, reader, mailboxes)
ui/              the styled layer over nsh/base
locales/app.yml  en and zh-CN
src/             the crate: Mail, the Backend trait, DemoBackend
```

`Mail` runs on a `Backend`. Until `LocalBackend` (mail-core) and `CloudBackend` land, both modes use `DemoBackend`:
three accounts and a few weeks of mail in memory; sending, flags, search and adding accounts work, and a message
arrives every 90 seconds and on each refresh. The crate's types mirror `mail_core::types` field for field.

```sh
nsh check                   # cargo check, then tsc against the generated declarations
nsh test                    # cargo test, then App.test.tsx headless
nsh run                     # this machine; --target web needs wasm32-unknown-unknown, clang and llvm-ar
nsh package --target web
```
