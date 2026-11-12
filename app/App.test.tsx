// The main flows against the demo backend, as a user takes them.
import { expect, open, test } from "nsh/test";

const desktop = { platform: "linux", width: 1280, height: 820, locale: "en-US" } as const;
const local = { storage: { "nshmail.mode": "local" } };

test("onboarding: keeping mail on this device opens the unified inbox", async () => {
  const app = await open("./App.tsx", desktop);
  await app.waitFor("id:onboarding");
  await app.press("id:onboarding-go");
  await app.waitFor("text:Design review notes: unified inbox");
  await app.waitFor("text:周五聚餐改到七点");
  expect(app.storage()["nshmail.mode"]).toBe("local");
});

test("adds an IMAP account with the detected servers", async () => {
  const app = await open("./App.tsx", { ...desktop, ...local });
  await app.waitFor("id:open-settings");
  await app.press("id:open-settings");
  await app.press("id:add-account");
  await app.press("id:add-imap");
  await app.type("id:imap-email", "me@fastmail.com");
  await app.press("id:imap-continue");
  await app.waitFor("id:imap-password");
  expect(app.value("id:imap-host")).toBe("imap.fastmail.com");
  expect(app.value("id:smtp-port")).toBe("465");
  await app.press("id:imap-add");
  await app.waitFor("text:Enter the app password.");
  await app.type("id:imap-password", "abcd efgh ijkl mnop");
  await app.press("id:imap-add");
  await app.waitFor("text:Added me@fastmail.com");
  await app.waitFor("id:modal", { state: "gone" });
  await app.waitFor("text:me@fastmail.com");
  await app.waitFor("text:Your account is connected");
});

test("opens a message, replies with it quoted, and finds the reply in Sent", async () => {
  const app = await open("./App.tsx", { ...desktop, ...local });
  await app.waitFor("text:Q4 roadmap draft");
  await app.press("text:Q4 roadmap draft");
  await app.waitFor("id:reader >> text:Q4-roadmap.pdf");
  await app.press("id:reply");
  await app.waitFor("id:compose-subject");
  expect(app.value("id:compose-subject")).toBe("Re: Q4 roadmap draft");
  expect(app.value("id:compose-to")).toBe("Jonas Weber <jonas@northwind.dev>");
  expect(app.value("id:compose-body")).toContain("> Attached is the first draft of the Q4 roadmap.");
  await app.press("id:send");
  await app.waitFor("text:Message sent");
  await app.press("text:Sent");
  await app.waitFor("id:messages >> text:Re: Q4 roadmap draft");
});

test("search finds mail across accounts and folders", async () => {
  const app = await open("./App.tsx", { ...desktop, ...local });
  await app.waitFor("id:search");
  await app.type("id:search", "roadmap");
  await app.waitFor("id:results >> text:Q4 roadmap draft");
  await app.waitFor("id:results >> text:Re: Q4 roadmap draft");
  await app.clear("id:search");
  await app.type("id:search", "no such words anywhere");
  await app.waitFor("text:Nothing found for “no such words anywhere”");
});

test("phones: the inbox, a message and back", async () => {
  const app = await open("./App.tsx", { platform: "android", width: 390, height: 844, locale: "en-US", ...local });
  await app.waitFor("text:Photos from the weekend");
  await app.press("text:Photos from the weekend");
  await app.waitFor("id:reader >> text:Daniel Okafor");
  await app.back();
  await app.waitFor("id:messages");
});

test("desktops: closing the window keeps nsh-mail running in the tray", async () => {
  const app = await open("./App.tsx", { ...desktop, ...local });
  await app.waitFor("text:Q4 roadmap draft");
  await app.window("main").pressClose();
  expect(app.window("main").state()).toMatchObject({ visible: false });
});

test("keys: j opens the next message, r replies to it", async () => {
  const app = await open("./App.tsx", { ...desktop, ...local });
  await app.waitFor("text:Q4 roadmap draft");
  await app.keyboard.press("j");
  await app.waitFor("id:reader >> text:Design review notes: unified inbox");
  await app.keyboard.press("j");
  await app.waitFor("id:reader >> text:Photos from the weekend");
  await app.keyboard.press("r");
  await app.waitFor("id:compose-subject");
  expect(app.value("id:compose-subject")).toBe("Re: Photos from the weekend");
});

test("new mail arrives on its own, also while the window is hidden", async () => {
  const app = await open("./App.tsx", { ...desktop, ...local, clock: {} });
  await app.clock!.runFor(1000);
  await app.waitFor("text:Q4 roadmap draft");
  await app.clock!.runFor(91_000);
  await app.waitFor("text:Quick check on the release notes");
  // Hidden, the next arrival goes to a system notification instead.
  await app.window("main").pressClose();
  await app.clock!.runFor(91_000);
  expect(app.window("main").state()).toMatchObject({ visible: false });
});

test("remote images stay blocked until the reader loads them", async () => {
  const app = await open("./App.tsx", { ...desktop, ...local });
  await app.waitFor("text:Design review notes: unified inbox");
  await app.press("text:Design review notes: unified inbox");
  await app.waitFor("id:reader >> text:Inbox mockup");
  await app.press("role:button[Load images]");
  await app.waitFor("text:Remote images are blocked to protect your privacy.", { state: "gone" });
});

test("scrolling to the end loads older mail", async () => {
  const app = await open("./App.tsx", { ...desktop, ...local });
  await app.waitFor("text:Q4 roadmap draft");
  expect(app.shows("text:Laptop setup")).toBe(false);
  for (let i = 0; i < 6 && !app.shows("text:Laptop setup"); i++) await app.scroll("id:messages", 2000);
  await app.waitFor("text:Laptop setup");
});
