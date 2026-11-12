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
src/             the crate: Mail, the Backend trait, LocalBackend, DemoBackend (debug builds)
tests/           local mode end to end against GreenMail
```

`Mail` runs on a `Backend` and speaks `mail_core::types` (as JSON, declared for TSX in `src/types.rs`).
`LocalBackend` (Linux, Windows, Android) is mail-core on the device: `mail.sqlite` in the app's data directory,
passwords in the OS keyring or, without one (and on Android), in an encrypted file beside it. `DemoBackend` keeps a few
weeks of mail in memory for the UI tests (`nshmail.mode` = `demo` in `Storage`) and stands in for cloud mode until
`CloudBackend` lands; release builds do not contain it. Gmail and Outlook sign-in need `NSHMAIL_GOOGLE_CLIENT_ID`
(and `NSHMAIL_GOOGLE_CLIENT_SECRET`) and `NSHMAIL_MS_CLIENT_ID` at build time; without them, pressing one says so.

```sh
nsh check                   # cargo check, then tsc against the generated declarations
nsh test                    # cargo test, then App.test.tsx headless
                            # GREENMAIL_HOST=127.0.0.1 also runs tests/greenmail.rs against a GreenMail
                            # (java -Dgreenmail.setup.test.all -Dgreenmail.auth.disabled -jar greenmail-standalone.jar)
nsh run                     # this machine; --target web needs wasm32-unknown-unknown, clang and llvm-ar
nsh package --target web
```
