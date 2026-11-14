// Signs up on a fresh nsh-mail-server, adds the three seeded GreenMail accounts
// through the UI, then captures the fixed screen set in light and dark.
// Usage: node shoot.mjs <server origin> <out dir>
import { mkdirSync } from "node:fs";
import { chromium } from "playwright";

const [origin = "http://127.0.0.1:18090", out = "out"] = process.argv.slice(2);
const imaps = process.env.GREENMAIL_IMAPS_PORT ?? "3993";
const smtps = process.env.GREENMAIL_SMTPS_PORT ?? "3465";
const NOW = new Date("2026-11-13T09:30:00+08:00"); // seed.py's NOW
const accounts = [
  ["lin.chen@northwind.dev", "Lin Chen (Northwind)"],
  ["lin@chen-family.example", "Lin Chen"],
  ["chenlin@mail.example", "陈琳"],
];
// The newest message of each account: all three listed means the first sync is done.
const synced = ["Lunch tomorrow?", "Reminder: your trip to Kyoto is coming up", "豆瓣读书周报：本周新书速递"];

mkdirSync(out, { recursive: true });
const t0 = Date.now();
const log = (...a) => console.log(`[${((Date.now() - t0) / 1000).toFixed(1)}s]`, ...a);

const browser = await chromium.launch({
  executablePath: process.env.CHROME_PATH || undefined,
  args: ["--force-color-profile=srgb", "--font-render-hinting=none", "--disable-partial-raster", "--disable-skia-runtime-opts"],
});
const context = await browser.newContext({
  viewport: { width: 1440, height: 900 },
  deviceScaleFactor: 1,
  locale: "en-US",
  timezoneId: "Asia/Shanghai",
  colorScheme: "light",
  permissions: ["notifications"],
});
// Headless Chromium reports a coarse pointer, which NSH reads as the phone idiom.
await context.addInitScript(() => {
  const mm = window.matchMedia.bind(window);
  window.matchMedia = (q) =>
    /pointer:\s*fine|hover:\s*hover/.test(q) ? mm("(min-width: 0px)") : /pointer:\s*coarse|hover:\s*none/.test(q) ? mm("(max-width: 0px)") : mm(q);
});
const page = await context.newPage();
await page.clock.setFixedTime(NOW);
page.on("pageerror", (e) => log("pageerror", String(e).slice(0, 300)));
page.on("response", (r) => {
  const u = new URL(r.url());
  if (u.pathname.startsWith("/v1/") && r.status() >= 400) log("HTTP", r.status(), r.request().method(), u.pathname);
});

const id = (testID) => page.getByTestId(testID);
const field = (testID) => id(testID).locator("input,textarea").or(page.locator(`input[data-testid="${testID}"],textarea[data-testid="${testID}"]`)).first();
async function fill(testID, value) {
  await field(testID).fill(value);
}
async function press(testID) {
  await id(testID).first().click();
}
async function idle() {
  await page.waitForLoadState("networkidle").catch(() => {});
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(800);
}
async function shot(name, theme) {
  await idle();
  await page.screenshot({ path: `${out}/web-${name}-${theme}.png`, animations: "disabled", caret: "hide" });
  log("saved", `web-${name}-${theme}.png`);
}
async function home() {
  await page.goto(origin + "/");
  await id("open-settings").waitFor({ timeout: 60_000 });
  for (const text of synced) await page.getByText(text).first().waitFor({ timeout: 90_000 });
}

await page.goto(origin + "/");
await id("onboarding").waitFor({ timeout: 60_000 });
if (await id("mode-cloud").isVisible()) await press("mode-cloud");
await page.getByText("Create account").click();
if (await id("server").isVisible()) await fill("server", origin);
await fill("cloud-email", "lin@mail.example");
await fill("cloud-password", "correct horse battery staple");
await press("onboarding-go");
await id("open-settings").waitFor({ timeout: 30_000 });
log("signed up");

for (const [email, name] of accounts) {
  await press("open-settings");
  await press("add-account");
  await press("add-imap");
  await fill("imap-email", email);
  await fill("imap-name", name);
  await press("imap-continue");
  await id("imap-password").waitFor({ timeout: 30_000 });
  await fill("imap-host", "127.0.0.1");
  await fill("imap-port", imaps);
  await fill("smtp-host", "127.0.0.1");
  await fill("smtp-port", smtps);
  await fill("imap-password", "pw");
  await press("imap-add");
  await page.getByText(`Added ${email}`).waitFor({ timeout: 30_000 });
  await id("modal").waitFor({ state: "detached" }).catch(() => {});
  log("added", email);
}

// The work account signs its mail: the composer shots show it above the quote.
await press("open-settings");
await press("account-more-0");
await page.getByText("Signature…").click();
await fill("signature-text", "**Lin Chen**\nNorthwind · [northwind.dev](https://northwind.dev)");
await press("signature-save");
await page.getByText("Signature saved").waitFor();
await press("header-back");
log("signature set");

for (const theme of ["light", "dark"]) {
  await page.emulateMedia({ colorScheme: theme });
  await home();
  await shot("inbox", theme);

  await page.getByText("Booking confirmed: Shanghai").first().click();
  await id("reader").waitFor();
  await page.getByText("E-ticket-K7Q2XM.pdf").first().waitFor();
  await shot("reader", theme);

  // A conversation: the latest reply expanded, the message it answers collapsed above.
  await page.getByText("周末回家吃饭吗？").first().click();
  await id("thread-older").waitFor();
  await shot("conversation", theme);
  // Reaching that row scrolled the list: back to its top.
  await home();

  await page.getByText("Lunch tomorrow?").first().click();
  await press("reply");
  await field("compose-body").waitFor();
  await field("compose-body").press("Control+Home");
  await page.keyboard.type("Hi Maya,\n\nSounds good, 12:15 works. I'll bring the dark mode mockups so we can go through contrast before Thursday.\n\nLin");
  await shot("compose", theme);

  await home();
  if (await id("open-search").isVisible()) await press("open-search");
  await field("search").fill("Tokyo");
  await field("search").press("Enter");
  await id("results").waitFor({ timeout: 30_000 }).catch(() => {});
  await shot("search", theme);

  await home();
  await press("open-settings");
  await id("signed-in-as").waitFor();
  await shot("settings", theme);

  await press("account-more-0");
  await page.getByText("Signature…").click();
  await id("signature-preview").waitFor();
  await shot("signature", theme);
  await page.locator('[data-testid="header-back"]:visible').click();

  await press("add-account");
  await id("add-imap").waitFor();
  await shot("add-account", theme);
}
// Undo send and Schedule send, on the wall clock (the server refuses times past): the composer's
// Send menu, Sending… with Undo, the Scheduled list. Nothing goes: both are taken back.
await page.clock.setFixedTime(new Date());
for (const theme of ["light", "dark"]) {
  await page.emulateMedia({ colorScheme: theme });
  await home();
  await press("compose");
  await fill("compose-to", "maya@northwind.dev");
  await fill("compose-subject", "Kickoff notes");
  await fill("compose-body", "Hi Maya,\n\nNotes from today's kickoff are below.\n\nLin");
  await press("send-later");
  await page.getByText("Tomorrow morning").waitFor();
  await shot("send-menu", theme);
  await page.getByText("Tomorrow morning").click();
  await page.getByText(/^Scheduled for /).waitFor();

  await press("compose");
  await fill("compose-to", "maya@northwind.dev");
  await fill("compose-subject", "Slides for Thursday");
  await press("send");
  await page.getByText("Sending…").waitFor();
  await shot("sending", theme);
  await press("toast-action");
  await field("compose-subject").waitFor();
  await press("compose-discard");
  await press("confirm-discard");

  await press("mailbox-scheduled");
  await id("scheduled").waitFor({ timeout: 30_000 });
  await shot("scheduled", theme);
  await press("scheduled-cancel");
  await page.getByText("Scheduled send cancelled").waitFor();
  await press("mailbox-all");
}
await page.clock.setFixedTime(NOW);

// The narrow web layout (navigation bar, one stack) at a phone's 412×915.
await page.setViewportSize({ width: 412, height: 915 });
for (const theme of ["light", "dark"]) {
  await page.emulateMedia({ colorScheme: theme });
  await page.goto(origin + "/");
  for (const text of synced) await page.getByText(text).first().waitFor({ timeout: 90_000 });
  await shot("narrow-inbox", theme);
  await page.getByText("Booking confirmed: Shanghai").first().click();
  await id("reader").waitFor();
  await page.getByText("E-ticket-K7Q2XM.pdf").first().waitFor();
  await shot("narrow-reader", theme);
  // The 600px booking mail fits the phone's width: no sideways scroll.
  let overflow = null;
  for (const f of page.frames()) {
    const o = await f
      .evaluate(() => (document.querySelector('table[width="600"]') ? document.documentElement.scrollWidth - document.documentElement.clientWidth : null))
      .catch(() => null);
    if (o !== null) overflow = o;
  }
  log("narrow reader scrolls sideways by", overflow, "px");
  if (overflow === null || overflow > 0) throw new Error(`the booking mail at 412px: sideways overflow ${overflow}`);
}
// Empty and failure states, at 1440 again: a folder with nothing in it, a search that finds nothing.
await page.setViewportSize({ width: 1440, height: 900 });
for (const theme of ["light", "dark"]) {
  await page.emulateMedia({ colorScheme: theme });
  await home();
  // Nothing is seeded in Drafts or Trash.
  await press("mailbox-drafts");
  await page.getByText("No drafts").waitFor({ timeout: 30_000 });
  await shot("empty-folder", theme);
  await press("mailbox-trash");
  await page.getByText("Trash is empty").waitFor({ timeout: 30_000 });
  await shot("empty-trash", theme);
  await press("mailbox-all");
  await fill("search", "no such words anywhere");
  await page.getByText("Nothing found for \u201cno such words anywhere\u201d").waitFor({ timeout: 30_000 });
  await shot("search-none", theme);
  await press("search-clear");
}

// The server goes away: the sync fails (a banner with a retry) and the reader
// cannot open a message. Both say what to do, instead of looking empty.
const serverPid = Number(process.env.SERVER_PID ?? 0);
if (!serverPid) log("SERVER_PID unset: no failure shots");
else {
  await page.emulateMedia({ colorScheme: "dark" });
  await home();
  process.kill(serverPid, "SIGKILL");
  await press("refresh-all");
  await id("sync-banner").waitFor({ timeout: 90_000 });
  await shot("sync-error", "dark");
  await page.emulateMedia({ colorScheme: "light" });
  await shot("sync-error", "light");
  await page.getByText("Reminder: benefits enrollment closes Nov 20").first().click();
  await id("reader-retry").waitFor({ timeout: 60_000 });
  await shot("reader-error", "light");
  await page.emulateMedia({ colorScheme: "dark" });
  await shot("reader-error", "dark");
}

await browser.close();
log("done");
