// The ui layer's components behave as HeroUI's do; on a real target
// (`nsh test --target web`) the gallery is also pictured, light and dark, in .nsh/.
import { open, test } from "nsh/test";

const desktop = { platform: "linux", width: 1440, height: 900, locale: "en-US" } as const;

test("ui components keep their states and open their overlays", async () => {
  const app = await open("./Gallery.tsx", desktop);
  await app.waitFor("text:on checked inbox inbox");
  await app.press("id:switch");
  await app.press("id:checkbox");
  await app.press("id:nav-sent");
  await app.press("role:tab[Drafts] >> nth=0");
  await app.waitFor("text:off unchecked sent drafts");
  await app.press("id:menu-trigger");
  await app.waitFor("text:Mark as read");
  await app.press("text:Archive");
  await app.waitFor("text:Mark as read", { state: "gone" });
  await app.press("id:open-modal");
  await app.waitFor("text:Discard draft?");
  await app.press("id:keep");
  await app.waitFor("id:modal", { state: "gone" });
});

test("gallery pictures", async () => {
  for (const scheme of ["light", "dark"] as const) {
    const app = await open("./Gallery.tsx", { ...desktop, colorScheme: scheme });
    // Headless runs draw nothing; a real target (`nsh test --target web`) saves the pictures.
    const drawn = await app.screenshot().catch(() => null);
    if (!drawn?.length) return;
    await app.setViewport(1440, 1180);
    await app.screenshot(`.nsh/heroui-gallery-${scheme}.png`);
  }
});
