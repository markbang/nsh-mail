// The main flows against the in-memory demo backend (debug builds), as a user
// takes them. Local mode against real servers: tests/greenmail.rs.
import { expect, open, test } from "nsh/test";

const desktop = { platform: "linux", width: 1280, height: 820, locale: "en-US" } as const;
const demo = { storage: { "nshmail.mode": "demo" } };

test("prefills our server for cloud mode", async () => {
  const app = await open("./App.tsx", desktop);
  await app.press("id:mode-cloud");
  expect(app.value("id:server")).toBe("https://mail-api.bangwu.me");
});

test("adds an IMAP account with the detected servers", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
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
  const app = await open("./App.tsx", { ...desktop, ...demo });
  await app.waitFor("text:Q4 roadmap draft");
  await app.press("text:Q4 roadmap draft");
  await app.waitFor("id:reader >> text:Q4-roadmap.pdf");
  await app.press("id:reply");
  await app.waitFor("id:compose-subject");
  expect(app.value("id:compose-subject")).toBe("Re: Q4 roadmap draft");
  expect(app.value("id:compose-to")).toBe("Jonas Weber <jonas@northwind.dev>");
  expect(app.value("id:compose-body")).toMatch(/^\n\nOn .*\n> Attached is the first draft of the Q4 roadmap\.[^]*\n$/);
  await app.press("id:send");
  await app.waitFor("text:Message sent");
  await app.press("text:Sent");
  await app.waitFor("id:messages >> text:Re: Q4 roadmap draft");
});

test("attaches picked files, keeps a forward's, and refuses more than the limit", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
  await app.waitFor("text:Q4 roadmap draft");
  await app.press("text:Q4 roadmap draft");
  await app.waitFor("id:reader >> text:Q4-roadmap.pdf");
  await app.press("id:forward");
  await app.waitFor("id:compose-attachments >> text:Q4-roadmap.pdf");
  await app.type("id:compose-to", "mei@lumen.dev");

  let huge = false;
  app.onPick((pick) => {
    expect(pick.kind).toBe("files");
    if (huge) return [{ name: "footage.mov", type: "video/quicktime", data: new Uint8Array(26 * 1024 * 1024) }];
    return [
      { name: "2026年第三季度报告.pdf", type: "application/pdf", data: new Uint8Array(300 * 1024) },
      { name: "截图.png", type: "image/png", data: new Uint8Array(2048) },
    ];
  });
  await app.press("id:compose-attach");
  await app.waitFor("id:compose-attachments >> text:300 KB");
  await app.waitFor("id:compose-attachments >> text:2 KB");
  expect(app.count("id:compose-attachment")).toBe(3);
  await app.press("label:Remove 截图.png");
  await app.waitFor("id:compose-attachments >> text:截图.png", { state: "gone" });

  // A file dropped from the file manager, and one pasted.
  const icon = new URL("./assets/tray.svg", import.meta.url);
  await app.dropFiles("id:compose-body", [`file://${icon.pathname}`]);
  await app.waitFor("id:compose-attachments >> text:tray.svg");
  await app.press("label:Remove tray.svg");
  await app.paste("id:compose-body", { files: [icon.pathname] });
  await app.waitFor("id:compose-attachments >> text:tray.svg");
  await app.press("label:Remove tray.svg");

  huge = true;
  await app.press("id:compose-attach");
  await app.waitFor("text:/^Attachments can add up to 25.0 MB/");
  expect(app.count("id:compose-attachment")).toBe(2);

  await app.press("id:send");
  await app.waitFor("text:Message sent");
  await app.press("text:Sent");
  await app.press("id:messages >> text:Fwd: Q4 roadmap draft");
  await app.waitFor("id:reader >> text:Q4-roadmap.pdf");
  await app.waitFor("id:reader >> text:2026年第三季度报告.pdf");
  expect(app.shows("id:reader >> text:截图.png")).toBe(false);
});

test("search finds mail across accounts and folders", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
  await app.waitFor("id:search");
  await app.type("id:search", "roadmap");
  await app.waitFor("id:results >> text:Q4 roadmap draft");
  await app.waitFor("id:results >> text:Re: Q4 roadmap draft");
  await app.waitFor("text:Results for “roadmap”");
  await app.clear("id:search");
  await app.type("id:search", "no such words anywhere");
  await app.waitFor("text:Nothing found for “no such words anywhere”");
});

test("archives a message with Undo, then for real", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
  await app.waitFor("text:Q4 roadmap draft");
  await app.press("text:Q4 roadmap draft");
  await app.waitFor("id:reader >> text:Q4-roadmap.pdf");
  await app.press("id:archive");
  await app.waitFor("text:Archived");
  await app.waitFor("id:messages >> text:Q4 roadmap draft", { state: "gone" });
  await app.press("id:toast-action");
  await app.waitFor("id:messages >> text:Q4 roadmap draft");

  await app.press("text:Q4 roadmap draft");
  await app.waitFor("id:reader >> text:Q4-roadmap.pdf");
  await app.press("id:archive");
  await app.waitFor("text:Archived", { state: "gone", timeout: 15000 });
  await app.press("text:Archive");
  await app.waitFor("id:messages >> text:Q4 roadmap draft");
});

test("phones: the inbox, a message and back", async () => {
  const app = await open("./App.tsx", { platform: "android", width: 390, height: 844, locale: "en-US", ...demo });
  await app.waitFor("text:Photos from the weekend");
  await app.press("text:Photos from the weekend");
  await app.waitFor("id:reader >> text:Daniel Okafor");
  await app.back();
  await app.waitFor("id:messages");
});

test("desktops: closing the window keeps nsh-mail running in the tray", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
  await app.waitFor("text:Q4 roadmap draft");
  await app.window("main").pressClose();
  expect(app.window("main").state()).toMatchObject({ visible: false });
});

test("keys: j opens the next message, r replies to it", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
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
  const app = await open("./App.tsx", { ...desktop, ...demo, clock: {} });
  await app.clock!.runFor(1000);
  await app.waitFor("text:Q4 roadmap draft");
  await app.clock!.runFor(91_000);
  await app.waitFor("text:Quick check on the release notes");
  // Hidden, the next arrival goes to a system notification instead.
  await app.window("main").pressClose();
  await app.clock!.runFor(91_000);
  expect(app.window("main").state()).toMatchObject({ visible: false });
});

test("HTML mail shows in the web view, its remote images blocked until the reader asks", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
  await app.waitFor("text:Design review notes: unified inbox");
  await app.press("text:Design review notes: unified inbox");
  await app.waitFor("id:reader >> id:body-html");
  expect(app.shows("id:body-text")).toBe(false);
  await app.press("role:button[Show images]");
  await app.waitFor("text:Remote images are blocked to protect your privacy.", { state: "gone" });
  await app.press("text:Photos from the weekend");
  await app.waitFor("id:reader >> id:body-text");
  expect(app.shows("id:body-html")).toBe(false);
});

test("scrolling to the end loads older mail", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
  await app.waitFor("text:Q4 roadmap draft");
  expect(app.shows("text:Laptop setup")).toBe(false);
  for (let i = 0; i < 6 && !app.shows("text:Laptop setup"); i++) await app.scroll("id:messages", 2000);
  await app.waitFor("text:Laptop setup");
});
