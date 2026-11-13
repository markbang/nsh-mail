// HeroUI v2's design language (@heroui/theme 2.4.26, MIT) as typed values for
// the places classes can't reach: style props, motions, Rust-free math.
// Colors live in theme.json; sizes, radii, shadows and easings in nsh.json.

/** HeroUI's semantic colors. Each has a 50–900 scale, a DEFAULT and a foreground in theme.json. */
export type Color = "default" | "primary" | "secondary" | "success" | "warning" | "danger";

/** HeroUI's variants for buttons, chips and menus. */
export type Variant = "solid" | "bordered" | "light" | "flat" | "faded" | "shadow" | "ghost";

export type Size = "sm" | "md" | "lg";
export type Radius = "none" | "sm" | "md" | "lg" | "full";

/** `radius.small/medium/large` in points (`rounded-small` …). */
export const radius = { small: 8, medium: 12, large: 14 } as const;

/** HeroUI's TRANSITION_EASINGS, plus CSS `ease`, which its 250ms state transitions use. */
export const easing = {
  css: [0.25, 0.1, 0.25, 1],
  soft: [0.36, 0.66, 0.4, 1],
  easeIn: [0.4, 0, 1, 1],
  easeOut: [0, 0, 0.2, 1],
  easeInOut: [0.4, 0, 0.2, 1],
  spring: [0.155, 1.105, 0.295, 1.12],
  softSpring: [0.16, 1.11, 0.3, 1.02],
} as const;

/** `rounded-*` per radius prop. */
export const rounded: Record<Radius, string> = {
  none: "rounded-none",
  sm: "rounded-small",
  md: "rounded-medium",
  lg: "rounded-large",
  full: "rounded-full",
};

/** HeroUI's shadows swap to their dark layers (inset rim, deeper drop) in dark mode. */
export const shadow = {
  none: "",
  sm: "shadow-small dark:shadow-small-dark",
  md: "shadow-medium dark:shadow-medium-dark",
  lg: "shadow-large dark:shadow-large-dark",
} as const;

/** The keyboard focus: 2px focus-colored outline, 2px off the edge. */
export const focusRing = "focus-visible:outline-2 focus-visible:outline-focus focus-visible:outline-offset-2";

/** Disabled controls fade to HeroUI's disabledOpacity. */
export const disabled = "disabled:opacity-50";
