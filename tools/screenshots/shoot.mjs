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
  ["mark.wu@lumenlabs.dev", "Mark Wu (Lumen Labs)"],
  ["mark@wu-family.net", "Mark Wu"],
  ["2837465510@qq.com", "吴马克"],
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
await fill("cloud-email", "mark@bangwu.top");
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

for (const theme of ["light", "dark"]) {
  await page.emulateMedia({ colorScheme: theme });
  await home();
  await shot("inbox", theme);

  await page.getByText("Booking confirmed: Shanghai").first().click();
  await id("reader").waitFor();
  await page.getByText("E-ticket-K7Q2XM.pdf").first().waitFor();
  await shot("reader", theme);

  await page.getByText("Lunch tomorrow?").first().click();
  await press("reply");
  await field("compose-body").waitFor();
  await field("compose-body").press("Control+Home");
  await page.keyboard.type("Hi Sarah,\n\nSounds good, 12:15 works. I'll bring the dark mode mockups so we can go through contrast before Thursday.\n\nMark");
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

  await press("add-account");
  await id("add-imap").waitFor();
  await shot("add-account", theme);
}
await browser.close();
log("done");
