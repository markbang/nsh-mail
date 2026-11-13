// Motion tokens, HeroUI's: state changes take 250ms on CSS `ease`, presses sink
// to 0.97, popovers spring in from 0.8 (300ms) and leave at 0.96 (150ms),
// tooltips from 0.85 (200ms), modals fade (400ms in, 300ms out). Reduced
// motion turns transitions and scales off.

export const duration = {
  instant: 100,
  fast: 150,
  base: 250,
  popover: 300,
  modal: 400,
  modalExit: 300,
} as const;

/** A pressable control: colors and opacity ease, the control sinks to 0.97. */
export const press =
  "transition duration-250 ease-css active:scale-[0.97] motion-reduce:transition-none motion-reduce:active:scale-100";

/** A row or list item: colors only, rows deepen instead of scaling. */
export const wash = "transition-colors duration-250 ease-css motion-reduce:transition-none";

/** Anchored popups (menus, selects, popovers): HeroUI's scaleSpringOpacity. */
export const popover = {
  class: "transition duration-300 ease-out motion-reduce:transition-none",
  starting: "opacity-0 scale-[0.8]",
  ending: "opacity-0 scale-[0.96] duration-150",
} as const;

/** Tooltips: HeroUI's scaleSpring. */
export const tooltip = {
  class: "transition duration-200 ease-out motion-reduce:transition-none",
  starting: "opacity-0 scale-[0.85]",
  ending: "opacity-0 scale-[0.85] duration-150",
} as const;
