// Cloud mode end to end: two copies of the app, Alice's and Bob's, against
// a running nsh-mail-server on 127.0.0.1:18080 whose mailboxes are
// GreenMail's (IMAPS 3993, SMTPS 3465, auth disabled). `.e2e/run.sh` starts
// both and runs this file; `nsh test` does not look in hidden directories.
// A test file has no network of its own, so everything goes through the apps.
import { expect, open, test, type App } from "nsh/test";

const server = "http://127.0.0.1:18080";
const greenmail = "127.0.0.1";
const desktop = { platform: "linux", width: 1280, height: 820, locale: "en-US", network: "live" } as const;
const slow = { timeout: 30_000 };

/** Opens the app and signs up on the server as `email`. */
async function signUp(email: string) {
  const app = await open("../App.tsx", desktop);
  await app.waitFor("id:onboarding");
  await app.press("id:mode-cloud");
  await app.press("text:Create account");
  await app.type("id:server", server);
  await app.type("id:cloud-email", email);
  await app.type("id:cloud-password", "correct horse battery");
  await app.press("id:onboarding-go");
  await app.waitFor("id:open-settings", slow);
  expect(app.storage()["nshmail.mode"]).toBe("cloud");
  // Headless hosts keep no files: the session is in the app's storage, as on the web.
  expect(app.storage()["nshmail.cloud.token"]).toBeTruthy();
  return app;
}

async function openAddAccount(app: App) {
  await app.press("id:open-settings");
  await app.waitFor("id:signed-in-as");
  await app.press("id:add-account");
}

/** Adds the GreenMail mailbox `email` from the add-account page. */
async function addMailbox(app: App, email: string) {
  await app.press("id:add-imap");
  await app.type("id:imap-email", email);
  await app.press("id:imap-continue");
  await app.waitFor("id:imap-password", slow);
  for (const [id, value] of [
    ["imap-host", greenmail],
    ["imap-port", "3993"],
    ["smtp-host", greenmail],
    ["smtp-port", "3465"],
  ]) {
    await app.clear(`id:${id}`);
    await app.type(`id:${id}`, value);
  }
  await app.type("id:imap-password", "app-password");
  await app.press("id:imap-add");
  await app.waitFor(`text:Added ${email}`, slow);
  await app.waitFor("id:modal", { state: "gone" });
  await app.waitFor(`text:${email}`);
}

async function send(app: App, to: string, subject: string) {
  await app.press("id:compose");
  await app.type("id:compose-to", to);
  await app.type("id:compose-subject", subject);
  await app.type("id:compose-body", `${subject}, from Bob.`);
  await app.press("id:send");
  await app.waitFor("text:Message sent", slow);
}

test("cloud mode: sign up, add a mailbox, read, get new mail live, reply, sign out", async () => {
  const run = Date.now();
  const alice = `alice${run}@test.local`;
  const bob = `bob${run}@test.local`;

  const bobs = await signUp(bob);
  await openAddAccount(bobs);
  await addMailbox(bobs, bob);
  await send(bobs, alice, "Hello before you joined");

  const alices = await signUp(alice);

  // Android's Gradle host hands over its FCM token; cloud mode registers it.
  await alices.host.send("push.token", { kind: "fcm", token: `fcm-${run}` });
  await alices.waitUntil(() => alices.requests().some((r) => r.method === "POST" && r.url === `${server}/v1/devices`));
  expect(alices.requests().find((r) => r.url === `${server}/v1/devices`)?.body).toBe(`{"kind":"fcm","token":"fcm-${run}"}`);

  // A provider's sign-in opens the server's start page and comes back on
  // nshmail://oauth-done, here with the error of a server without Google.
  await openAddAccount(alices);
  await alices.press("id:add-google");
  await alices.waitUntil(() => alices.openedUrls().length > 0);
  expect(alices.openedUrls()[0]).toMatch(/^http:\/\/127\.0\.0\.1:18080\/v1\/oauth\/google\/start\?token=[^&]+&return=native$/);
  await alices.openWithUrl("nshmail://oauth-done?error=google+sign-in+is+not+configured+on+this+server");
  await alices.waitFor("text:/google sign-in is not configured on this server/");

  // The server syncs the new mailbox; its events reload the list.
  await addMailbox(alices, alice);
  await alices.waitFor("id:messages >> text:Hello before you joined", slow);

  // New mail arrives over the event stream: nothing in Alice's app asks for it.
  await send(bobs, alice, "Lunch at noon?");
  await alices.waitFor("id:messages >> text:Lunch at noon?", slow);

  await alices.press("id:messages >> text:Lunch at noon?");
  await alices.press("id:reply");
  await alices.type("id:compose-body", "Sounds good.");
  await alices.press("id:send");
  await alices.waitFor("text:Message sent", slow);
  await bobs.waitFor("id:messages >> text:Re: Lunch at noon?", slow);

  // Signing out ends the session on the server and on the device.
  bobs.onDialog(() => "Sign out");
  await bobs.press("id:open-settings");
  await bobs.press("id:sign-out");
  await bobs.waitFor("id:onboarding", slow);
  expect(bobs.storage()["nshmail.mode"]).toBeUndefined();
  expect(bobs.storage()["nshmail.cloud.token"]).toBeUndefined();
}, 240_000);
