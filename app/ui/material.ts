// Android draws the app in Material 3 Expressive: the Material roles of theme.json
// (`md-*`, the 2025 color spec's content scheme from seed #0062d6) take the place of
// the HeroUI roles the screens name (material.json), and motion runs on Material's
// springs. Desktops and the web keep HeroUI's look.
import { platform, setTheme } from "nsh";
import theme from "../theme.json";
import roles from "../material.json";

export const material = platform.os === "android";

type Vars = Record<string, string>;
const mapped = (vars: Vars): Vars => ({ ...vars, ...Object.fromEntries(Object.entries(roles as Vars).map(([name, role]) => [name, vars[role]])) });

/** Shows Android in Material's colors; elsewhere nothing changes. */
export function applyMaterialTheme() {
  if (!material) return;
  const vars = theme.cssVars as { theme: Record<string, string>; light: Vars; dark: Vars };
  setTheme({ cssVars: { theme: vars.theme, light: mapped(vars.light), dark: mapped(vars.dark) } });
}

/** Material 3 Expressive's spring tokens (Compose ExpressiveMotionTokens: stiffness and damping ratio),
 * as NSH springs, whose `damping` is the coefficient: ratio × 2√stiffness at mass 1. Spatial springs move
 * and resize, effects springs fade and recolor without overshoot. */
const spring = (stiffness: number, ratio: number) => ({ stiffness, damping: ratio * 2 * Math.sqrt(stiffness), mass: 1 });
export const springs = {
  fastSpatial: spring(800, 0.6),
  spatial: spring(380, 0.8),
  slowSpatial: spring(200, 0.8),
  fastEffects: spring(3800, 1),
  effects: spring(1600, 1),
  slowEffects: spring(800, 1),
} as const;

/** The same springs as class transitions (Material's curves for them: `ease-spatial-fast` … in nsh.json). */
export const transition = {
  fastSpatial: "transition-all duration-350 ease-spatial-fast motion-reduce:transition-none",
  spatial: "transition-all duration-500 ease-spatial motion-reduce:transition-none",
  fastEffects: "transition-colors duration-150 ease-effects-fast motion-reduce:transition-none",
  effects: "transition duration-200 ease-effects motion-reduce:transition-none",
} as const;

/** A pressed button's shape morph: from its resting corners to Material's pressed `shape-sm`, on the fast spatial spring. */
export const pressMorph = `${transition.fastSpatial} active:rounded-shape-sm`;
