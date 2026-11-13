// Motion on M3E's expressive springs: spatial springs move and resize (and
// overshoot), effects springs fade and recolor. Class transitions take
// Material's web curves for them (tokens.springCurve); reduced motion turns
// transitions and scales off.
import { springCurve } from "./tokens";

export const duration = {
  fast: springCurve.effects.fast.ms,
  base: springCurve.effects.default.ms,
  popover: springCurve.spatial.fast.ms,
  modal: springCurve.spatial.default.ms,
  modalExit: springCurve.effects.slow.ms,
} as const;

/** A pressable control: colors on the fast effects spring, the press on the fast spatial one. */
export const press =
  "transition duration-350 ease-spatial-fast active:scale-[0.97] motion-reduce:transition-none motion-reduce:active:scale-100";

/** A row or list item: colors only. */
export const wash = "transition-colors duration-150 ease-effects-fast motion-reduce:transition-none";

/** Anchored popups (menus, selects, popovers) grow from their trigger. */
export const popover = {
  class: "transition duration-350 ease-spatial-fast motion-reduce:transition-none",
  starting: "opacity-0 scale-[0.8]",
  ending: "opacity-0 scale-[0.96] duration-150 ease-effects-fast",
} as const;

export const tooltip = {
  class: "transition duration-150 ease-effects-fast motion-reduce:transition-none",
  starting: "opacity-0 scale-[0.85]",
  ending: "opacity-0 duration-150",
} as const;
