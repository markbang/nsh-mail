// The notification inbox's screens, light and dark, wide and narrow: on a
// real target (`nsh test --target web`) they are saved under .nsh/.
import { open, test } from "nsh/test";

const desktop = { platform: "linux", width: 1440, height: 900, locale: "en-US" } as const;
const demo = { storage: { "nshmail.mode": "demo", "nshmail.undoSend": "0" } };

test("notification screens light and dark", async () => {
  for (const scheme of ["light", "dark"] as const) {
    const app = await open("./App.tsx", { ...desktop, colorScheme: scheme, ...demo });
    // Headless runs draw nothing; a real target (`nsh test --target web`) saves the pictures.
    const drawn = await app.screenshot().catch(() => null);
    if (!drawn?.length) return;
    await app.waitFor("id:rail-notifications");
    await app.press("id:rail-notifications");
    await app.waitFor("id:notify-list >> text:CI failed on main");
    await app.press("id:notify-list >> text:CI failed on main");
    await app.waitFor("id:notify-title");
    await app.screenshot(`.nsh/notify-wide-${scheme}.png`);
    await app.press("id:notify-sources");
    await app.waitFor("id:sources");
    await app.screenshot(`.nsh/notify-sources-${scheme}.png`);
    await app.press("id:sources-add");
    await app.waitFor("id:add-source");
    await app.screenshot(`.nsh/notify-source-kinds-${scheme}.png`);
    await app.press("id:header-back");
    await app.waitFor("id:sources");
    await app.press("id:header-back");
    await app.waitFor("id:notifications");

    await app.press("id:open-settings");
    await app.press("id:settings-keys");
    await app.waitFor("id:api-keys");
    await app.screenshot(`.nsh/notify-keys-${scheme}.png`);
    await app.press("id:keys-add-top");
    await app.waitFor("id:new-key");
    await app.screenshot(`.nsh/notify-new-key-${scheme}.png`);
    await app.press("id:header-back");
    await app.waitFor("id:api-keys");
    await app.press("id:header-back");
    await app.waitFor("id:settings");
    await app.press("id:settings-rules");
    await app.waitFor("id:notify-rules");
    await app.screenshot(`.nsh/notify-rules-${scheme}.png`);

    const narrow = await open("./App.tsx", { platform: "linux", width: 390, height: 844, locale: "en-US", colorScheme: scheme, ...demo });
    await narrow.waitFor("id:nav-notifications");
    await narrow.press("id:nav-notifications");
    await narrow.waitFor("id:notify-list >> text:CI failed on main");
    await narrow.screenshot(`.nsh/notify-narrow-${scheme}.png`);
    await narrow.press("id:notify-list >> text:CI failed on main");
    await narrow.waitFor("id:notify-title");
    await narrow.screenshot(`.nsh/notify-narrow-open-${scheme}.png`);
  }
}, 120_000);
