# Performance

`scroll.mjs` measures the web client in headless Chrome: the frame intervals of a scripted scroll, of
a page landing, of a sync over a long list, and of opening a conversation of 200 messages. Run it
against two builds to compare them.

The demo mailboxes are the fixture: `nshmail.mode` = `demo` (a few weeks of mail) or `demo-large`
(10,000 messages in 2,000 conversations, one of them 200 messages long). A release build has them
only with `--cfg nshmail_demo`:

```sh
cd app
RUSTFLAGS="--cfg nshmail_demo" nsh package App.tsx --target web -o /tmp/web
node tools/perf/scroll.mjs /tmp/web/nsh-mail --large --label before --out /tmp/before.json
```

`puppeteer-core` is the only dependency (`npm i puppeteer-core`, or point `PUPPETEER_CORE` at its
module); Chrome comes from `CHROME_PATH`, else `/usr/local/bin/google-chrome`.

On Android the same mailbox is measured by `.github/android-perf.py` in the Android UI job, from
`gfxinfo framestats` while the inbox is flung down: frames drawn, their p50/p90/p99 and the share
over 16.7 ms.
