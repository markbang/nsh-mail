// Regenerates the color roles of app/theme.json from one seed with Google's
// material-color-utilities, light and dark. Fidelity is Material Theme
// Builder's "color match": the brand blue stays the primary container and the
// surfaces stay neutral for reading. (Spec 2025 applies to tonal spot, vibrant,
// expressive and neutral; switch SCHEME to try them.)
// Run: cd scripts/m3e && npm ci && npm run gen
import { readFileSync, writeFileSync } from "node:fs";
import { registerHooks } from "node:module";

// 0.4.0 ships a few extensionless ESM imports ("./dynamic_color"); resolve them as .js.
registerHooks({
  resolve(spec, ctx, next) {
    try {
      return next(spec, ctx);
    } catch (e) {
      if (spec.startsWith(".") && !spec.endsWith(".js")) return next(spec + ".js", ctx);
      throw e;
    }
  },
});
const { argbFromHex, Blend, hexFromArgb, Hct, MaterialDynamicColors, SchemeFidelity, TonalPalette } = await import("@material/material-color-utilities");

const SEED = "#2f62d0"; // the app icon's blue
const SCHEME = SchemeFidelity;
const SPEC = "2025";
const ACCOUNT_HUES = ["#8e24aa", "#ef6c00", "#2e7d32", "#d81b60", "#00838f"];
const themePath = new URL("../../app/theme.json", import.meta.url);


// App roles on the M3 palette, and the shadcn names nsh/base draws with.
const ALIASES = {
  unread: "primary",
  flag: "tertiary",
  "state-layer": "on-surface",
  foreground: "on-surface",
  card: "surface-container-low",
  "card-foreground": "on-surface",
  popover: "surface-container",
  "popover-foreground": "on-surface",
  "primary-foreground": "on-primary",
  "secondary-foreground": "on-secondary",
  muted: "surface-container-highest",
  "muted-foreground": "on-surface-variant",
  accent: "secondary-container",
  "accent-foreground": "on-secondary-container",
  destructive: "error",
  border: "outline-variant",
  input: "outline",
  ring: "secondary",
  sidebar: "surface-container",
  "sidebar-foreground": "on-surface",
  "sidebar-primary": "primary",
  "sidebar-primary-foreground": "on-primary",
  "sidebar-accent": "secondary-container",
  "sidebar-accent-foreground": "on-secondary-container",
  "sidebar-border": "outline-variant",
  "sidebar-ring": "secondary",
};

function roles(isDark) {
  const scheme = new SCHEME(Hct.fromInt(argbFromHex(SEED)), isDark, 0, SPEC, "phone");
  const dyn = new MaterialDynamicColors();
  const out = {};
  // allColors lacks these three; primary_dim and its kin are the watch's.
  for (const c of [...dyn.allColors, dyn.surfaceVariant(), dyn.surfaceTint(), dyn.scrim()]) {
    if (!c || /^(primary|secondary|tertiary|error)_dim$/.test(c.name)) continue;
    out[c.name.replace(/_/g, "-")] = hexFromArgb(c.getArgb(scheme));
  }
  // Account marks: M3 custom colors (harmonized to the seed, tone 40 light / 80
  // dark), five hues that never read as the unread blue.
  ACCOUNT_HUES.forEach((hex, i) => {
    const palette = TonalPalette.fromInt(Blend.harmonize(argbFromHex(hex), argbFromHex(SEED)));
    out[`chart-${i + 1}`] = hexFromArgb(palette.tone(isDark ? 80 : 40));
  });
  for (const [k, v] of Object.entries(ALIASES)) out[k] = out[v];
  return out;
}

const theme = JSON.parse(readFileSync(themePath, "utf8"));
theme.cssVars.light = roles(false);
theme.cssVars.dark = roles(true);
writeFileSync(themePath, JSON.stringify(theme, null, 2) + "\n");
console.log(`theme.json: ${Object.keys(theme.cssVars.light).length} colors per appearance from ${SEED}`);
