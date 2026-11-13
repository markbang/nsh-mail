// Popup motion on M3E's springs (Material's web curves for them): anchored
// popups grow from their trigger on the fast spatial curve and fade out on the
// fast effects one; tooltips fade. Reduced motion turns transitions off.
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
