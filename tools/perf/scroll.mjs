// Measures the web client on the demo mailboxes, in headless Chrome: the frame intervals of a
// scripted scroll, of a page landing, of a sync over a long list, and of opening a long
// conversation. Run it against two builds (before, after) to compare.
//
// Usage: node tools/perf/scroll.mjs <site dir> [--large] [--out <file.json>] [--label <name>]
//
// <site dir> is a `nsh package App.tsx --target web` output (`RUSTFLAGS=--cfg nshmail_demo` for
// the demo mailboxes); `--large` opens the 10,000-message mailbox (`nshmail.mode` = `demo-large`).
// Needs `puppeteer-core` (`PUPPETEER_CORE` pointing at its module, or installed next to this file)
// and Chrome (`CHROME_PATH`).
import { createServer } from "node:http";
import { readFile, writeFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";

const args = process.argv.slice(2);
const site = args.find((a) => !a.startsWith("--"));
const flag = (name) => args.includes(`--${name}`);
const value = (name) => args[args.indexOf(`--${name}`) + 1];
const out = value("out") ?? "perf.json";
const label = value("label") ?? (flag("large") ? "large" : "small");

const puppeteerModule = process.env.PUPPETEER_CORE ?? "puppeteer-core";
const puppeteer = await import(puppeteerModule).then((m) => m.default ?? m).catch(() => {
  throw new Error("puppeteer-core is missing: npm i puppeteer-core, or set PUPPETEER_CORE");
});

const types = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".json": "application/json",
  ".webmanifest": "application/manifest+json",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".css": "text/css; charset=utf-8",
};
const server = createServer(async (req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname));
  const file = join(site, path === "/" ? "index.html" : path);
  try {
    const body = await readFile(file);
    res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" });
    res.end(body);
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
const origin = `http://127.0.0.1:${server.address().port}/`;

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME_PATH || "/usr/local/bin/google-chrome",
  headless: true,
  args: ["--no-sandbox", "--disable-dev-shm-usage", "--force-color-profile=srgb", "--font-render-hinting=none"],
});
const page = await browser.newPage();
await page.setViewport({ width: 1440, height: 900 });
await page.evaluateOnNewDocument((mode) => localStorage.setItem("nsh:top.bangwu.nshmail:nshmail.mode", mode), flag("large") ? "demo-large" : "demo");
// A headless browser reports a coarse pointer, which NSH reads as a phone; the desktop list wants a mouse.
await page.evaluateOnNewDocument(() => {
  const mm = window.matchMedia.bind(window);
  window.matchMedia = (q) =>
    /pointer:\s*fine|hover:\s*hover/.test(q) ? mm("(min-width: 0px)") : /pointer:\s*coarse|hover:\s*none/.test(q) ? mm("(max-width: 0px)") : mm(q);
});
const errors = [];
page.on("pageerror", (e) => errors.push(String(e).slice(0, 300)));

const wait = (ms) => page.evaluate((ms) => new Promise((ok) => setTimeout(ok, ms)), ms);
const round = (n) => Math.round(n * 10) / 10;
const slowest = (frames, over = 50) => {
  const at = frames.gaps.map((d, i) => [d, frames.gapAt[i]]).filter(([d]) => d > over);
  return at.length ? Math.max(...at.map(([d]) => round(d))) : 0;
};
const stats = (list) => {
  if (!list.length) return { frames: 0 };
  const sorted = [...list].sort((a, b) => a - b);
  const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))];
  return {
    frames: list.length,
    p50: round(at(0.5)),
    p95: round(at(0.95)),
    p99: round(at(0.99)),
    max: round(sorted[sorted.length - 1]),
    over33: list.filter((d) => d > 33).length,
    over50: list.filter((d) => d > 50).length,
    over100: list.filter((d) => d > 100).length,
  };
};
/** Starts the page's frame clock; `framesSince(t)` reads the intervals after `t` (ms on its clock). */
const frames = () =>
  page.evaluate(() => {
    window.__frames ??= [];
    const tick = (t) => {
      window.__frames.push(t);
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });
const now = () => page.evaluate(() => performance.now());
const framesSince = (from) =>
  page.evaluate((from) => {
    const seen = (window.__frames ?? []).filter((t) => t >= from);
    const gaps = seen.map((t, i) => (i ? t - seen[i - 1] : 0)).slice(1);
    return { gaps, gapAt: seen.slice(1) };
  }, from);
const rows = () => page.$$eval('[data-testid^="message-"]', (els) => els.length);
/** The deepest conversation listed on screen, by the `#<n>` its subject ends with. */
const deepest = () =>
  page.$$eval('[data-testid^="message-"]', (els) =>
    els.reduce((deep, el) => {
      const at = /#(\d+)\b/.exec(el.getAttribute("aria-label") ?? "");
      return at ? Math.max(deep, Number(at[1])) : deep;
    }, -1),
  );
const threadRows = () => page.$$eval('[data-testid^="thread-message-"]', (els) => els.length);

/** The view with `selector`, else the last one labelled `label` (a build without the testID). */
async function find(selector, label) {
  const el = await page.$(selector);
  if (el) return el;
  const byLabel = await page.$$(`[aria-label="${label}"]`);
  return byLabel.at(-1) ?? null;
}
async function press(handle) {
  const box = await handle.boundingBox();
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}

const t0 = Date.now();
const report = { label, errors };
const step = async (name, run) => {
  try {
    report[name] = await run();
  } catch (e) {
    report[name] = { failed: String(e).slice(0, 200) };
  }
};

await page.goto(origin, { waitUntil: "load" });
await page.waitForSelector('[data-testid="messages"]', { timeout: 120000 });
await page.waitForSelector('[data-testid^="message-"]', { timeout: 120000 });
report.firstRowsMs = Date.now() - t0;
await wait(1200);
await frames();
report.texts = await page.evaluate(() => document.body.innerText.length);
const list = await page.$('[data-testid="messages"]');
const box = await list.boundingBox();
await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
const hoverList = () => page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
const spin = async (steps, delta) => {
  for (let i = 0; i < steps; i++) {
    await page.mouse.wheel({ deltaY: delta });
    await wait(16);
  }
};

// A conversation of 200 messages: how long the reader takes to show, and to reveal more of it.
await step("reader", async () => {
  const fromPage = await page.evaluate(() => {
    const row = [...document.querySelectorAll('[data-testid^="message-"]')].find((el) => /200 messages/.test(el.getAttribute("aria-label") ?? ""));
    row?.scrollIntoView();
    return !!row;
  });
  if (!fromPage) throw new Error("no 200-message conversation on screen");
  const started = Date.now();
  const row = await page.evaluateHandle(
    () => [...document.querySelectorAll('[data-testid^="message-"]')].find((el) => /200 messages/.test(el.getAttribute("aria-label") ?? "")),
  );
  const from = await now();
  await press(row);
  await page.waitForSelector('[data-testid="reader"]', { timeout: 30000 });
  const openMs = Date.now() - started;
  const shown = await wait(1000).then(threadRows);
  const more = await page.$('[data-testid="thread-more"]');
  let reveal = null;
  if (more) {
    const revealFrom = await now();
    await press(more);
    await wait(800);
    reveal = { ...stats((await framesSince(revealFrom)).gaps), rows: await threadRows() };
    await page.screenshot({ path: out.replace(/\.json$/, "") + `-${label}-reveal.png` });
  }
  await page.screenshot({ path: out.replace(/\.json$/, "") + `-${label}-reader.png` });
  const gaps = await framesSince(from);
  return { openMs, ...stats(gaps.gaps), collapsedRows: shown, reveal };
});

// Scrolling: 12 gestures of 20 wheel steps, the way a user flicks down a long list.
await step("scroll", async () => {
  await hoverList();
  const from = await now();
  const started = await deepest();
  let widest = 0;
  for (let i = 0; i < 10; i++) {
    await spin(10, 400);
    widest = Math.max(widest, await rows());
    await wait(150);
  }
  return { ...stats((await framesSince(from)).gaps), rowsInDom: widest, deepestFrom: started, deepest: await deepest() };
});
await wait(1200);

// Paging: to the end of what is loaded, a page at a time.
await step("paging", async () => {
  await hoverList();
  const from = await now();
  for (let i = 0; i < 8; i++) {
    await spin(10, 1200);
    await wait(400);
  }
  return { ...stats((await framesSince(from)).gaps), rowsInDom: await rows(), deepest: await deepest() };
});
await wait(1200);

await page.screenshot({ path: out.replace(/\.json$/, "") + `-${label}-deep.png` });

// A sync over the loaded list: the Refresh button, and how the frame cadence holds while it lands.
await step("sync", async () => {
  await hoverList();
  const loaded = await rows();
  const before = await deepest();
  const from = await now();
  const refresh = await find('[data-testid="refresh"]', "Check for mail");
  if (!refresh) throw new Error("no Refresh button");
  await press(refresh);
  await wait(3000);
  const frames = await framesSince(from);
  return { ...stats(frames.gaps), slowest: slowest(frames), rowsInDom: loaded, rowsAfter: await rows(), deepestBefore: before, deepest: await deepest() };
});
await wait(1200);

report.durationMs = Date.now() - t0;
report.errorCount = errors.length;
await writeFile(out, JSON.stringify(report, null, 2) + "\n");
console.log(JSON.stringify(report, null, 2));
await browser.close();
server.close();
