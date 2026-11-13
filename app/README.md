# nsh-mail app

The client: TSX screens over a Rust crate that exports `nsh-mail/native` (the `Mail` module of
[docs/design.md](../docs/design.md) §3). Design system and motion: [DESIGN.md](DESIGN.md).

```
App.tsx          shell: onboarding, rail + list + reader (wide), tabs (phones, narrow web), keys, snackbar
store.ts         TSX state: mirrors of Rust's status, accounts and messages; mailbox, selection, search
nav.ts           the phone stack and the wide layout's dialog stack
resident.ts      tray, close-to-tray, new-mail notifications
pages/           one NavPage each
components/      what pages share (message list, reader, mailboxes)
ui/              the styled layer over nsh/base
locales/app.yml  en and zh-CN
src/             the crate: Mail, the Backend trait, LocalBackend, CloudBackend, DemoBackend (debug builds)
tests/           local mode end to end against GreenMail
web/sw.js        the Web Push service worker, copied next to the web build's index.html
.e2e/            cloud mode end to end against nsh-mail-server and GreenMail (`nsh test` skips it)
```

`Mail` runs on a `Backend` and speaks `mail_core::types` (as JSON, declared for TSX in `src/types.rs`).
`LocalBackend` (Linux, Windows, Android) is mail-core on the device: `mail.sqlite` in the app's data directory,
passwords in the OS keyring or, without one (and on Android), in an encrypted file beside it. `CloudBackend` (every
platform) talks to an nsh-mail server: REST through NSH's `fetch`, events over a WebSocket that reconnects with backoff
(tokio-tungstenite natively, the browser's through NSH on the web), the session token in the same secret stores or, on
the web, in the page's storage. A provider's sign-in opens the server's `/v1/oauth/{provider}/start` in the browser and
comes back on `nshmail://oauth-done` (on the web, in a window that hands the result over a `BroadcastChannel`). On the
web it subscribes to Web Push once notifications are allowed. It does not cache lists and bodies on the device yet.
`DemoBackend` keeps a few weeks of mail in memory for the UI tests (`nshmail.mode` = `demo` in `Storage`); release
builds do not contain it. Gmail and Outlook sign-in in local mode need `NSHMAIL_GOOGLE_CLIENT_ID` (and
`NSHMAIL_GOOGLE_CLIENT_SECRET`) and `NSHMAIL_MS_CLIENT_ID` at build time; without them, pressing one says so.

HTML mail shows in NSH's `WebView` (`components/MailView.tsx`), so where the app runs it needs a web engine: WebKitGTK
6.0 on Linux (`libwebkitgtk-6.0-4` on Debian and Ubuntu; GNOME desktops have it), the WebView2 Runtime on Windows
(part of Windows 11; Edge installs it on Windows 10) and the system WebView on Android. Without one the reader shows
the mail's text.

```sh
nsh check                   # cargo check, then tsc against the generated declarations
nsh test                    # cargo test, then App.test.tsx headless
                            # GREENMAIL_HOST=127.0.0.1 also runs tests/greenmail.rs against a GreenMail
                            # (java -Dgreenmail.setup.test.all -Dgreenmail.auth.disabled -jar greenmail-standalone.jar)
nsh run                     # this machine; --target web needs wasm32-unknown-unknown, clang and llvm-ar
nsh package --target web    # needs wasm-bindgen-cli of the version Cargo.toml pins
cp web/sw.js .nsh/out/nsh-mail/  # nsh ships no extra files; the server serves it as /sw.js
nsh package --target android --abi arm64-v8a  # preview APK; releases are the AAB (--format aab), see android/README.md
GREENMAIL_JAR=… .e2e/run.sh # cloud mode end to end; or with GreenMail already on 3993 / 3465
```

Run `nsh package` from this directory: release builds take their size profile from `.cargo/config.toml` (z, fat LTO).

The web client is meant to be served by its server (`NSHMAIL_WEB_DIR`), so it signs in to the page's own origin by
default and needs no CORS.
