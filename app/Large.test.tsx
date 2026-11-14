// The list with a large mailbox: 2,000 conversations and 10,000 messages (`demo-large`), as a long
// session meets it: scrolling far, paging deeper, and opening a conversation of 200 messages.
import { expect, open, test } from "nsh/test";

const desktop = { platform: "linux", width: 1280, height: 820, locale: "en-US" } as const;
const large = { storage: { "nshmail.mode": "demo-large", "nshmail.undoSend": "0" } };

/** The texts on screen: a row that is not rendered contributes none, so this bounds the work done. */
const texts = (app: { texts: () => string[] }) => app.texts().length;

/** Scrolls until a conversation numbered `index` is listed, a screen at a time, letting pages load. */
async function scrollTo(app: Awaited<ReturnType<typeof open>>, index: number, screens = 12) {
  const at = `id:messages >> text:/ #${index}$/`;
  for (let i = 0; i < screens && !app.shows(at); i++) {
    await app.scroll("id:messages", 1200);
    await app.wait(100);
  }
  return app.shows(at);
}

test("a 2,000-conversation inbox scrolls and pages with bounded rows", async () => {
  const app = await open("./App.tsx", { ...desktop, ...large });
  await app.waitFor("id:messages >> text:/ #0$/");
  expect(texts(app)).toBeLessThan(300);
  // Far past the first pages: rows that were never loaded arrive, the ones above are recycled.
  expect(await scrollTo(app, 100)).toBe(true);
  expect(app.shows("id:messages >> text:/ #0$/")).toBe(false);
  expect(texts(app)).toBeLessThan(300);
  // And further, over more pages, with the same bound.
  expect(await scrollTo(app, 200, 16)).toBe(true);
  expect(texts(app)).toBeLessThan(300);
});

test("a conversation of 200 messages opens on a few rows and reveals more a page at a time", async () => {
  const app = await open("./App.tsx", { ...desktop, ...large });
  await app.waitFor("id:messages >> text:/ #3$/");
  await app.press("id:messages >> text:/ #3$/");
  await app.waitFor("id:thread-more");
  expect(app.texts().length).toBeLessThan(300);
  expect(app.shows("text:/196 more/")).toBe(true);
  await app.press("id:thread-more");
  await app.waitFor("text:/171 more/");
  expect(app.shows("id:thread-less")).toBe(false);
  expect(app.texts().length).toBeLessThan(300);
});
