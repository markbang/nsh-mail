English | [简体中文](design.zh-CN.md)

# nsh-mail design

A client that puts several mail accounts into one inbox, written with [Native Shell](https://nsh.dev) (NSH 0.5), targeting Web, Android, Linux and Windows. Two modes:

- **Local mode**: credentials and mail stay on the device; the client connects to IMAP/SMTP directly.
- **Cloud mode**: the nsh-mail server syncs on the client's behalf and pushes new mail to clients.

## 1. Platform facts (from the NSH docs)

| Capability | What NSH provides | Conclusion |
|---|---|---|
| Raw TCP | Rust crates can use `tokio::net` on native platforms (`background(..)`); the TSX `net` module is native-only and fails in the browser with `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` | IMAP/SMTP live in Rust and connect directly on native platforms |
| Browser networking | Only `fetch` (subject to CORS) and WebSocket | The web cannot reach IMAP/SMTP directly |
| Storage | `Storage` is a 5 MB key-value store; `data_dir()` gives the app's private directory (`None` on the web) | The mail store is SQLite in `data_dir()`; `Storage` holds only settings |
| System notifications | `Notifications`: Android (POST_NOTIFICATIONS), Windows toast, Linux `org.freedesktop.Notifications`, Web `Notification` | Used directly while in the foreground / resident |
| Staying resident on desktop | `Tray` from `nsh/desktop` + main window `closeOnPress: false` | After the window closes the app stays in the tray and keeps syncing |
| Android packaging | `nsh package` does not use Gradle; packaged apps cannot add native code | FCM needs our own Gradle project embedding NSH (see §5) |
| Packaging machines | Linux packages can only be built on Linux, Windows packages only on Windows; Android and Web anywhere | CI uses the matching runner |
| Picking files | `Picker.pickFiles` exists on every platform (`<input type=file>` on the web) and yields a `File`, read whole into memory (512 MiB limit per file) | Used by the attach button |
| Paste | `Input`'s `onPaste` yields files on the clipboard such as images (`File`) and `file:` URLs copied from the file manager (not readable on the web) | The body field listens for paste |
| Drag and drop | `View`'s `onFileDrop` yields `file:` URLs only on desktop (GTK, Win32, AppKit); the web has none, and on the web TSX runs in a separate realm (a same-origin iframe) that never sees the page's drop events | Desktop uses `onFileDrop` and hands the paths to Rust to read (TSX's `fs/promises` needs the `capabilities.fs` grant); on the web Rust listens for `dragover`/`drop` on the page's `window` via web-sys (`PageDrop.listen`) |
| Passing bytes TSX ↔ Rust | `Vec<u8>` maps to `Uint8Array`, copied on each call | Attachment bytes cross the boundary once through `Mail.stage`; after that only the id is passed |

## 2. Local mode on the web: not offered

Browsers cannot open TCP connections. An optional "user-run relay" (WebSocket to TCP, with TLS terminated in wasm inside the browser) could technically keep plaintext hidden from the relay, but:

1. Credentials would have to live in browser storage (localStorage / OPFS), readable by any same-origin script or extension, a step weaker than the system keychain;
2. An open TCP relay is easily abused as a hop and would need its own authentication;
3. Once the tab closes there is no sync and no notification, so the experience still depends on an always-on machine.

Since the user has to run an always-on machine anyway, that machine is better off running the **nsh-mail server** directly: the same binary, self-hosted, and the web client connecting to it is the "user-run relay", with sync, search and push on top. So **the web has cloud mode only**; the server address can be the official instance or a self-hosted one, and the sign-in page says so plainly.

## 3. Architecture

```
crates/mail-core     Rust library, no NSH dependency: IMAP sync, SMTP sending, MIME parsing, SQLite+FTS5 storage,
                     OAuth (PKCE), provider presets, credential store. Shared by client and server.
crates/server        nsh-mail-server: axum + tokio. Accounts, sync scheduling, REST/WebSocket API, push (FCM, Web Push),
                     and it also serves the web client's static files (same origin, no CORS).
app/                 NSH mixed app (TSX UI + Rust crate). The crate exports `nsh-mail/native`; TSX only calls that.
android/             Gradle host project: NshView + Firebase Messaging (§5).
```

Inside the client there is a single `Mail` interface (exported from Rust to TSX) with two implementations behind it:

- `LocalBackend` (native platforms only, `cfg(not(target_arch = "wasm32"))`): mail-core talks to the mailboxes directly.
- `CloudBackend` (all platforms): calls the server's REST API through NSH `fetch` and receives events over WebSocket (tokio-tungstenite on native, NSH's browser WebSocket transport on the web), reconnecting with backoff when the connection drops. Caching lists and bodies on native platforms is left for later: mail-core has no public interface for writing server results.

TSX does not know which mode is active; there is one UI. State ownership: mail, accounts and sync state live in Rust; the current selection, drafts and filters live in TSX.

### TSX ↔ Rust interface (`nsh-mail/native`)

```ts
Mail.init(): Promise<Status>                       // open the store, restore the session, start syncing
Mail.status(): Status                              // { mode: "none"|"local"|"cloud", server?, user?, syncing, error? }
Mail.useLocal(): Promise<void>                     // rejects on the web: ERR_UNSUPPORTED
Mail.signIn(server, email, password, signUp): Promise<void>   // enter cloud mode
Mail.signOut(): Promise<void>
Mail.accounts(): Promise<Account[]>
Mail.detect(email): Promise<ServerSettings | null> // preset table + Mozilla autoconfig
Mail.addImap(input: ImapInput): Promise<Account>   // connects to IMAP and SMTP before saving
Mail.addOAuth(provider: "google"|"microsoft"): Promise<Account>
Mail.removeAccount(id): Promise<void>
Mail.list(q: { folder: "inbox"|"sent"|"drafts"|"archive"|"trash", account?, before?, limit }): Promise<Summary[]>
Mail.get(id): Promise<Message>                     // body already sanitized, layout tables turned into blocks (see §7); attachment metadata
Mail.openAttachment(id, attachment): Promise<string | null>  // web: images/PDF open in a new tab, everything else downloads; desktop: system save dialog, then open with the default app;
                                                   // without a save dialog (Android) saved to the app directory, then opened. Returns the path of a saved file no app could open
Mail.attachmentPreview(id, attachment): Promise<string>      // data: URI of a PNG/JPEG/GIF/WebP (≤ 8 MB), thumbnail in the reader
Mail.setFlags(id, { seen?, flagged? }): Promise<void>
Mail.moveTo(id, "archive" | "trash" | …): Promise<void>   // IMAP MOVE (COPY + \Deleted + EXPUNGE without MOVE); creates Archive/Trash when missing, Gmail archives to All Mail
Mail.send(draft: Draft): Promise<void>             // replies carry inReplyTo / references; attachments see §4 Attachments
Mail.stage(name, type, bytes: Uint8Array, onProgress: (sent) => void): Promise<Upload>   // stage an attachment, get an id
Mail.stageFile(path, type, onProgress): Promise<Upload>   // local file (desktop drop, paste after copying in the file manager), read by Rust; rejects on the web
Mail.unstage(id): Promise<void>
Mail.attachmentLimit(): number                     // most a message's attachments may add up to (bytes)
Mail.search(text, limit): Promise<Summary[]>
Mail.unread(): Promise<Record<accountId, number>>  // unread count of each account's inbox
Mail.refresh(): Promise<void>
Mail.on(listener: (e: MailEvent) => void): () => void   // newMail / changed / syncState / error
Push.setToken(token): Promise<void>                // FCM token from the Android host (§5), registered with the server in cloud mode
```

A message id is the local store's stable string id; in cloud mode it is the server's id. Unified inbox = every account's INBOX merged, newest first.

Every type's fields match `mail_core::types` one to one, with the same JSON shape (serde tags become `type`/`kind` fields, `Message` flattens `Summary`'s fields); `list`'s `before` is the id of the last message of the previous page; `MailEvent` is discriminated by `type`. These types are `mail_core::types`: the backends use them directly and they cross the boundary as serde JSON through `Js<T>` in `app/src/types.rs`, whose TS declarations are hand-written in the same file (the web compiles only mail-core's `types`, without the engine).

## 4. Data flow

### Local mode (Linux, Windows, Android)

```
TSX ──Mail.*──▶ Rust LocalBackend ──tokio/rustls──▶ IMAP (993 / 143+STARTTLS)
                       │                       └──▶ SMTP (465 / 587+STARTTLS)
                       ▼
              SQLite (data_dir/mail.sqlite, FTS5) ◀── credentials: system keychain
```

1. Adding an account: `detect` fills in the servers → connection test → credentials go to the keychain, account metadata to SQLite.
2. First sync: fetch the latest 500 messages of each folder. Messages up to 32 KB are downloaded whole (readable offline, full-text indexed); for larger ones only the headers and the first text part are fetched for the snippet (located through `BODYSTRUCTURE`, not partial FETCH), and the body is downloaded and cached on demand when opened.
3. Incremental: new mail is whatever lies above the highest UID stored locally (`UID FETCH n:*`, RFC 4549 §4.3.1); `UIDNEXT` is only used to tell whether there is new mail, never as a high-water mark (a server may assign a UID before the message becomes visible); with `CONDSTORE`, flags are synced through `HIGHESTMODSEQ`; when UIDVALIDITY changes the folder is rebuilt.
4. Real time: `IDLE` on INBOX; an `EXISTS`/`EXPUNGE`/`FETCH` (flags changed by another client) triggers an INBOX sync; IDLE is reissued every 30 seconds because some servers (such as GreenMail) hold flag changes made elsewhere until IDLE ends; a full sync runs every 29 minutes. Servers without IDLE are polled every 2 minutes. One command connection per account, reused; when the reused connection has been closed by the server (idle timeout), the command is retried once on a new connection, and other errors are not retried. Connection or sync errors are logged (with the account id), followed by a reconnect with backoff from 5 seconds to 5 minutes and a full sync. After new mail is written to the store a `newMail` event fires, plus a system notification when the app is not in the foreground.
5. Sending: lettre sends over SMTP, then on success `APPEND`s to "Sent" (except Gmail, which saves it on its own).
6. Search: local FTS5 (subject, from, to, body); old mail not stored locally will get IMAP `SEARCH` in a later version.

On desktop, closing the main window leaves the app in the tray, IDLE continues and system notifications work as usual. Android local mode syncs only while the app is open (no background service); background push needs cloud mode, as the settings page says.

### Cloud mode (all platforms)

```
client ──HTTPS REST──▶ nsh-mail-server ──mail-core──▶ each mailbox's IMAP/SMTP
   ▲  ◀──WebSocket events──┘   │
   │                           ├──FCM HTTP v1──▶ Android
   └─ system notification ◀────┴──Web Push (VAPID)──▶ browser Service Worker
```

1. The user signs up / signs in on the server (email + password) and gets a session token; the client stores the token (keychain on native platforms, localStorage on the web).
2. Adding an account: plain IMAP hands the app password to the server; OAuth goes through the server's web flow (open `/v1/oauth/{provider}/start`, the callback stores the token, then redirects back to `nshmail://oauth-done` or the web page).
3. The server keeps one resident tokio task per mail account, reusing mail-core's sync logic, and writes data into the shared `data/server.sqlite`, every row carrying a `user_id`.
4. New mail: written to the store → sent to online clients over WebSocket → pushed to offline devices: FCM on Android, Web Push on the web; the desktop app is a resident process holding a WebSocket and shows its own system notification when an event arrives.
5. Reading, searching and sending all go through REST. Caching lists and bodies in the native client's local SQLite (readable offline) is not implemented yet, pending a write interface in mail-core.

### Attachments

A draft's `attachments` are `{kind: "upload", id}` (a file staged with `Mail.stage`) or `{kind: "forward", messageId, index}` (an attachment of a stored message; forwarding includes all of them by default and each can be removed).

- Sources: the attach button `Picker.pickFiles`; pasting into the body field (`onPaste`'s `files`, plus `uris` on desktop); drag and drop (`onFileDrop` on desktop, `PageDrop.listen` on the web). TSX hands a `File`'s bytes to `Mail.stage` and a local path to `Mail.stageFile`.
- Staging: local mode keeps them in Rust memory and drops them after sending; cloud mode creates a record with `POST /v1/uploads` (name, type, size), then uploads in 1 MiB chunks with `PATCH /v1/uploads/{id}?offset=`, reporting progress after each chunk (NSH's Rust `fetch` sends and receives whole bodies, with no upload progress). Staged files stay with the draft (below) and are `unstage`d when it is discarded.
- Drafts: what is typed belongs to a draft (`app/drafts.ts`), one per context (a new message, or a reply, reply-all or forward of one message). Closing the composer (Escape, the backdrop, back) keeps an edited draft, and reopening the same context restores it; drafts are saved locally with NSH `Storage` in both modes, so they survive a restart, except staged uploads (Rust memory or server uploads), which only last the session. Only Discard deletes one, after a confirmation; sending also ends it; signing out clears all. Drafts are not synced to the IMAP Drafts folder yet.
- Limit: attachments add up to 25 MB by default (Gmail's and Outlook's limit), counted in raw bytes. The client checks as files are added (`Mail.attachmentLimit()`; cloud mode takes `maxAttachmentBytes` from the server's `/v1/me`); mail-core checks again before sending, and the server when creating the record and when sending; going over is `ERR_TOO_LARGE` (HTTP 413).
- Encoding: lettre builds `multipart/mixed`, with the file name in `Content-Disposition` per RFC 2231 and the `name` of `Content-Type` per RFC 2047 (the only one old clients understand). What gets `APPEND`ed to "Sent" is the whole message as sent, attachments included.
- Server: chunks are stored in `data/uploads/<user>/<id>` (metadata in `<id>.json`), deleted once sending succeeds; the rest are deleted after 24 hours by an hourly cleanup.

### Server API

```
POST /v1/auth/signup | /v1/auth/login        {email, password} → {token}
POST /v1/auth/logout      GET /v1/me    PATCH /v1/me  {hideContent} (notifications without content)
GET  /v1/accounts        POST /v1/accounts (imap)        DELETE /v1/accounts/{id}
GET  /v1/detect?email=   POST /v1/sync (sync every account now)
GET  /v1/oauth/{google|microsoft}/start?token=…           GET /v1/oauth/{provider}/callback
GET  /v1/messages?folder=&account=&before=&limit=         GET /v1/messages/{id}
GET  /v1/messages/{id}/attachments/{index}  attachment bytes: Content-Type is its type, Content-Disposition: attachment (filename* per RFC 5987 with the UTF-8 original name),
                                            CSP sandbox + nosniff; fetched from IMAP and cached when the body is not stored locally
PATCH /v1/messages/{id}  {seen?, flagged?}
POST /v1/messages/{id}/move  {folder}     move to another folder of the same account (archive, delete to Trash)
POST /v1/send            GET /v1/search?q=&limit=      GET /v1/unread → {accountId: unread count}
POST /v1/uploads         {filename, mimeType, size} → Upload {id, filename, mimeType, size}
PATCH /v1/uploads/{id}?offset=   raw chunk bytes (≤ 4 MiB) → {received}      DELETE /v1/uploads/{id}
POST /v1/devices         {kind: "fcm"|"webpush", token | subscription, vapidKey?}  DELETE /v1/devices/{id}
GET  /v1/push/vapid-key  GET /v1/events (WebSocket, Authorization or ?token=; with ?device=<id> that device gets no push while connected)
GET  /                   web client static files         GET /healthz
```

Error response bodies are `{code, message}` (mail-core's `Error`, `code` such as `ERR_AUTH`); the HTTP status gives the category. OAuth ends by redirecting to `nshmail://oauth-done?account=<id>` (when `start` had `return=native`) or `<public URL>/?oauth-done&account=<id>`, with `error=<message>` on failure. Server configuration: `crates/server/README.md`.

## 5. Push

| Platform | When the app is not in the foreground | Implementation |
|---|---|---|
| Android | FCM data message → `FirebaseMessagingService` shows a notification; tapping it opens the message | `android/` Gradle project (see below) |
| Web | Web Push → Service Worker `push` event → `showNotification` | NSH packaging carries no extra files, so after packaging `app/web/sw.js` is copied next to `index.html` (CI's web job does this); the Rust side calls `PushManager.subscribe` through wasm-bindgen (VAPID public key from the server), subscribing once the user allows notifications |
| Linux / Windows | Resident process (tray) holds a WebSocket and calls `Notifications.show` on `newMail` | Built into NSH, no extra service |

Server: FCM uses the HTTP v1 API + a service account JWT; Web Push uses the `web-push` crate (aes128gcm encryption, VAPID signing). A dead push target (404/410, `UNREGISTERED`) deletes that device. A Web Push subscription is bound to the VAPID public key it was made with: the server records that key with the device and at start deletes subscriptions whose key differs; a registration whose `vapidKey` is not the current key gets 409. The web client fetches `/v1/push/vapid-key` on every start; if an existing subscription's `applicationServerKey` differs it unsubscribes and subscribes again, then `POST /v1/devices` as usual.

Privacy: Web Push content is end-to-end encrypted; FCM payloads pass through Google and carry only the sender name and subject, and settings offer "notifications without content" (only the message id is sent; the client fetches the rest).

Android host (`android/`, usage in its README):

- **Runtime**: `prepare.sh` fetches `nsh.aar` (the Java runtime) at the CLI's version, verifying its sha256, replaces its `libnsh.so` with the one `nsh package --target android` builds from this crate (`.nsh/rust/android-release/lib`, containing `nsh-mail/native`), and puts the bundle built by `nsh build --release` into assets. `MainActivity` follows nsh's `AppActivity`: `NshView.loadAssetBundle`, notification and URL intents, the back key, immersive system bars.
- **Token**: the host sends the FCM token to the app as the host event `push.token` `{kind: "fcm", token}`; `app/push.ts` passes it to `Push.setToken`, and once signed in to cloud mode `on_token` picks it up and `POST /v1/devices`. An event rather than `host.call`: events need no grant, while `host.call` needs `capabilities.host` declared, and declaring any capabilities denies every capability not listed (network, notifications, …).
- **Notifications**: data messages are shown like the app's own notifications (nsh's notification channel, tag `message:<id>`, launch extra); on tap `handleNotificationIntent` hands it to the app's existing `Notifications` listener (`app/resident.ts`), which opens the message, with no new TSX.
- Without `google-services.json` it builds and runs as usual, just without push; CI writes the file when the secret `GOOGLE_SERVICES_JSON` exists.

## 6. Cloud service technology choices

- **Rust + axum + tokio**: same language as mail-core, one copy of the sync logic.
- **Storage**: one `server.sqlite` holds users, sessions, devices and every user's mail (the same tables and FTS5 as the client). Every row of the mail tables carries a `user_id` and the indexes start with it; mail-core offers queries only on a `Store` bound to a user, so a query that forgets the user cannot be written. The client's local store uses the same tables with a single user (`LOCAL_USER` = 0). One file is easy to back up and migrate; the cost is a single machine. One instance is enough for v1. The old per-user `data/users/<id>.sqlite` files are imported at start (row counts and contents checked table by table, inside a transaction) and the originals are kept, renamed `*.migrated`; if import or the check fails the server refuses to start and leaves the originals untouched.
- **Passwords**: Argon2id; session tokens are 32 random bytes, and the store keeps only their SHA-256.
- **Mail credentials**: encrypted with AES-256-GCM in each user's `data/users/<id>.secrets.json` (mail-core's `EncryptedFileStore`), with the master key from the environment variable `NSHMAIL_MASTER_KEY`, never written to disk. Rotation: put the new key in `NSHMAIL_MASTER_KEY` and the old one in `NSHMAIL_MASTER_KEY_OLD`; at start every file is read first, and if any entry opens with neither key the server refuses to start and changes no file; otherwise entries encrypted with the old key are rewritten file by file (a `.bak-old-key` copy is kept first, the file is replaced atomically, and the copy is deleted only after re-reading with the new key alone checks out), after which `NSHMAIL_MASTER_KEY_OLD` can be removed. With only one key set, entries it cannot open likewise stop the server from starting.
- **Deployment**: CI builds the Docker image and pushes it to ghcr.io; `docker compose` self-hosts in one step. The host and domain for the public service need separate confirmation (paid, public).

## 7. Security

- **OAuth first**: Gmail and Outlook are steered to OAuth (XOAUTH2) only. Local mode does authorization code + PKCE in the client with a loopback redirect `http://127.0.0.1:<random port>` (supported by Google's "Desktop app" clients and Microsoft's public clients, usable on Android too); cloud mode does the web flow on the server. Google scope `https://mail.google.com/`, Microsoft scopes `IMAP.AccessAsUser.All SMTP.Send offline_access`; both add `openid email` and take the account's address from the ID token. Client IDs are injected at build time (`NSHMAIL_GOOGLE_CLIENT_ID`, `NSHMAIL_MS_CLIENT_ID`).
- **Plain IMAP**: app passwords only (the UI explains how to create one and links to common providers); preset servers for QQ, 163, iCloud, Fastmail, Yahoo and others.
- **Credential storage**: Windows Credential Manager, Linux Secret Service (`keyring` crate); Linux sessions without Secret Service fall back to an encrypted 0600 file in the app's private directory, with a notice. Android v1 stores an encrypted file in the app's private directory (isolated by the system sandbox), moving to the Android Keystore once the Gradle host is in place.
- **Transport**: TLS only (993/465 or STARTTLS), plaintext refused; rustls + webpki roots.
- **Mail content**: HTML is sanitized once in mail-core with ammonia (removing scripts, event attributes, `javascript:`, forms, iframe/object/embed, `<meta>`/`<link>`/`<base>`; keeping `<style>`, `style`/`class` and table layout attributes), and `cid:` inline images become `data:` URIs (up to 10 MB in total per message). The client shows it in an NSH `WebView` (on the web a sandboxed iframe without `allow-scripts`; on native with JS off and `incognito`), and the document's CSP blocks every remote resource by default (images, CSS `url()`, fonts) until the user taps "Show images". `capabilities.webview.hosts` in `nsh.json` is empty: link navigation inside the WebView is refused natively and reported through `onError`, and the client opens it in the system browser; on the web `<base target="_blank">` opens a new tab. HTML sits on a light card (`color-scheme: light`) and is not inverted in dark mode. Mail without an HTML part, and platforms without a web engine (Linux lacking WebKitGTK 6.0, Windows lacking WebView2), show plain text.
- **Attachments**: on the web only images and PDFs open in a new tab as a blob of their own type; everything else downloads as `application/octet-stream`, so HTML/SVG attachments never run under the site's origin; the server's attachment endpoint likewise sends `Content-Security-Policy: sandbox`.
- **Logs**: never record credentials, tokens or bodies.

## 8. CI and artifacts

GitHub Actions, on every PR:

| Job | Runner | Artifacts |
|---|---|---|
| core + server | ubuntu-24.04 | `cargo test`, `clippy`; server Docker image (pushed to ghcr on main) |
| linux | ubuntu-24.04 (GTK 4.14) | `nsh package --target linux` → `.tar.gz` |
| windows | windows-latest (only on `v*` tags and manual runs) | `nsh package --target windows` → `.zip` + installer; signed with signtool (timestamped) when the certificate secret exists |
| android | ubuntu-24.04 + SDK/NDK | `nsh package`'s `.apk` and the Gradle host's (FCM) `.apk`, arm64-v8a only, all signed with the release key (forks' PRs, which get no secrets: debug key). `v*` tags and manual runs add two `.aab`s (arm64-v8a + armeabi-v7a; Play delivers only one `libnsh.so` per device), next to the `nsh package` `.apk`, still arm64-v8a only, and one Gradle host `.apk` per ABI |
| web | ubuntu-24.04 + wasm32 | static site `.zip` |
| release | ubuntu-24.04 (only on `v*` tags) | GitHub Release: Android `.apk` + `.aab`, Linux `.tar.gz`, Windows `.zip` + installer, `SHA256SUMS.txt`; notes from `docs/releases/<version>.md` and `.zh-CN.md` |

## 9. First version scope

Adding accounts (IMAP + OAuth), unified inbox, reading, composing and replying, search, local mode fully working; cloud mode syncs and pushes; CI produces artifacts for all four platforms. Not yet: multi-select bulk actions, folder management, rules, PGP.
