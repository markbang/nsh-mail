// The notification inbox's flows against the in-memory demo backend (debug
// builds): the feed and its filters, a webhook source's URL shown once, API
// keys created and revoked, and the per-source notification rules.
import { expect, open, test } from "nsh/test";

const desktop = { platform: "linux", width: 1280, height: 820, locale: "en-US" } as const;
const demo = { storage: { "nshmail.mode": "demo", "nshmail.undoSend": "0" } };

test("the feed holds every source; a chip filters it; opening reads, archiving takes away", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
  await app.waitFor("id:rail-notifications");
  await app.press("id:rail-notifications");
  // Mail is in it, and so are the webhooks and bots.
  await app.waitFor("id:notify-list >> text:CI failed on main");
  await app.waitFor("id:notify-list >> text:Q4 roadmap draft");

  await app.press("id:notify-filter-src-mail-a-work");
  await app.waitFor("id:notify-list >> text:CI failed on main", { state: "gone" });
  await app.waitFor("id:notify-list >> text:Q4 roadmap draft");
  await app.press("id:notify-filter-all");
  await app.waitFor("id:notify-list >> text:CI failed on main");

  // Opening one shows it beside the list, and marks it read.
  await app.press("id:notify-list >> text:CI failed on main");
  await app.waitFor("id:notify-title");
  expect(app.texts()).toContain("CI failed on main");
  await app.waitFor("id:notify-body");
  await app.waitFor("id:notify-read");

  // A search across the feed finds the words anywhere in a notification.
  await app.type("id:notify-search", "disk");
  await app.waitFor("id:notify-list >> text:Disk almost full");
  expect(app.shows("id:notify-list >> text:CI failed on main")).toBe(false);
  await app.press("id:notify-search-clear");
  await app.waitFor("id:notify-list >> text:CI failed on main");

  // Archive takes it out at once; Undo puts it back.
  await app.press("id:notify-list >> text:CI failed on main");
  await app.waitFor("id:notify-archive");
  await app.press("id:notify-archive");
  await app.waitFor("id:notify-list >> text:CI failed on main", { state: "gone" });
  await app.waitFor("text:Notification archived");
  await app.press("id:toast-action");
  await app.waitFor("id:notify-list >> text:CI failed on main");
});

test("a mail notification opens in Mail", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
  await app.waitFor("id:rail-notifications");
  await app.press("id:rail-notifications");
  await app.press("id:notify-list >> text:Q4 roadmap draft");
  await app.waitFor("id:notify-open-mail");
  await app.press("id:notify-open-mail");
  await app.waitFor("id:reader >> text:Q4-roadmap.pdf");
});

test("adding a webhook source shows its URL once, and again only by rotating it", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
  await app.waitFor("id:open-settings");
  await app.press("id:open-settings");
  await app.press("id:settings-sources");
  await app.waitFor("id:sources >> text:GitHub");
  await app.press("id:sources-add");
  await app.waitFor("id:add-source");
  // The chats wait for the server's bot connections; the three ingest kinds do not.
  expect(app.shows("id:kind-webhook")).toBe(true);
  expect(app.shows("id:kind-slack")).toBe(false);
  await app.press("id:kind-webhook");
  await app.waitFor("id:source-form");
  await app.press("id:source-create");
  await app.waitFor("text:Name the source.");
  await app.type("id:source-name", "Buildkite");
  await app.press("id:source-color-chart-3");
  await app.press("id:source-priority-high");
  await app.press("id:source-create");

  await app.waitFor("id:source-url");
  const url = app.texts().find((text) => text.startsWith("https://") && text.includes("/hooks/"));
  expect(url).toBeTruthy();
  await app.press("id:source-copy");
  expect(app.clipboard()).toBe(url);
  await app.press("id:source-test");
  await app.waitFor("text:/^Test message sent from Buildkite/");
  await app.press("id:source-done");
  await app.waitFor("id:sources >> text:Buildkite");
  // The URL is not kept on screen: rotating the source makes a new one.
  expect(app.texts().some((text) => text.includes("/hooks/"))).toBe(false);
  await app.press("id:source-more-10");
  await app.press("text:Show ingest URL");
  await app.waitFor("id:source-url");
  const rotated = app.texts().find((text) => text.startsWith("https://") && text.includes("/hooks/"));
  expect(rotated).not.toBe(url);
});

test("an API key is created, copied once, listed and revoked", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
  await app.waitFor("id:open-settings");
  await app.press("id:open-settings");
  await app.press("id:settings-keys");
  await app.waitFor("id:api-keys >> text:Claude Code");
  await app.press("id:keys-add");
  await app.waitFor("id:new-key");
  await app.press("id:key-create");
  await app.waitFor("text:Name the key.");
  await app.type("id:key-name", "Build agent");
  await app.press("id:key-write");
  await app.press("id:key-scope-picked");
  await app.press("id:key-source-src-github");
  await app.press("id:key-create");

  await app.waitFor("id:key-token");
  const token = app.texts().find((text) => text.startsWith("nb_"));
  expect(token).toBeTruthy();
  await app.press("id:key-copy");
  expect(app.clipboard()).toBe(token);
  await app.press("id:key-done");
  await app.waitFor("id:api-keys >> text:Build agent");
  expect(app.texts().some((text) => text.startsWith("nb_") && text === token)).toBe(false);

  app.onDialog((dialog) => {
    expect(dialog.title).toContain("Build agent");
    return "Revoke";
  });
  await app.press("id:key-more-2");
  await app.press("text:Revoke");
  await app.waitFor("id:api-keys >> text:Build agent", { state: "gone" });
  await app.waitFor("text:/^Build agent revoked/");
});

test("the rules keep a mode per source and priority band", async () => {
  const app = await open("./App.tsx", { ...desktop, ...demo });
  await app.waitFor("id:open-settings");
  await app.press("id:open-settings");
  await app.press("id:settings-rules");
  await app.waitFor("id:notify-rules >> text:GitHub");
  await app.press("id:rule-src-github-high-off");
  await app.press("id:rule-src-mail-a-work-low-silent");
  const rules = JSON.parse(app.storage()["nshmail.notify.rules"] ?? "{}");
  expect(rules["src-github"].high).toBe("off");
  expect(rules["src-mail-a-work"].low).toBe("silent");
});

test("phones: the Notifications tab, a notification and back", async () => {
  const app = await open("./App.tsx", { platform: "android", width: 390, height: 844, locale: "en-US", ...demo });
  await app.waitFor("text:Q4 roadmap draft");
  await app.press("id:tab-notifications-button");
  await app.waitFor("id:notify-list >> text:CI failed on main");
  await app.press("id:notify-list >> text:CI failed on main");
  await app.waitFor("id:notify-title");
  expect(app.texts()).toContain("CI failed on main");
  await app.back();
  await app.waitFor("id:notify-list >> text:CI failed on main");
});
