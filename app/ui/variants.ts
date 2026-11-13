// HeroUI's color × variant matrix (utils/variants.ts in @heroui/theme): the
// surface, the ink for labels and icons, and the hover each pair takes.
import type { Color, Variant } from "./tokens";
import { material } from "./material";

export interface Look {
  /** Background, border and shadow. */
  frame: string;
  /** Label color; text classes on a view pass to the texts inside. */
  text: string;
  /** Icon tint, the same color as `tint-*`. */
  tint: string;
  /** Pointer hover (and the matching icon tint for ghost). */
  hover: string;
  tintHover: string;
}

const flatInk: Record<Color, string> = {
  default: "default-700",
  primary: "primary-600",
  secondary: "secondary-600",
  success: "success-700",
  warning: "warning-700",
  danger: "danger-600",
};
const flatInkDark: Partial<Record<Color, string>> = { success: "success", warning: "warning", danger: "danger-500" };

const ink = (c: Color, dark?: string) => ({
  text: `text-${c}${dark ? ` dark:text-${dark}` : ""}`,
  tint: `tint-${c}${dark ? ` dark:tint-${dark}` : ""}`,
});

/** Solid, faded, flat, bordered and shadow surfaces dim to HeroUI's hoverOpacity (.8 light, .9 dark). */
const dim = "hover:opacity-80 dark:hover:opacity-90";

export function look(variant: Variant, color: Color): Look {
  const neutral = color === "default";
  const on = { text: `text-${color}-foreground`, tint: `tint-${color}-foreground` };
  const accent = neutral ? { text: "text-default-foreground", tint: "tint-default-foreground" } : ink(color);
  switch (variant) {
    case "solid":
      return { frame: `bg-${color}`, ...on, hover: dim, tintHover: "" };
    case "shadow":
      return { frame: `bg-${color} shadow-lg shadow-${color}/${neutral ? 50 : 40}`, ...on, hover: dim, tintHover: "" };
    case "bordered":
      return {
        frame: `border-2 border-${color} bg-transparent`,
        ...(neutral ? { text: "text-foreground", tint: "tint-foreground" } : ink(color)),
        hover: dim,
        tintHover: "",
      };
    case "flat": {
      // Android: Material's tonal button, in the color's container.
      if (material && (neutral || color === "primary" || color === "secondary"))
        return { frame: "bg-md-secondary-container", text: "text-md-on-secondary-container", tint: "tint-md-on-secondary-container", hover: "", tintHover: "" };
      if (material && color === "danger")
        return { frame: "bg-md-error-container", text: "text-md-on-error-container", tint: "tint-md-on-error-container", hover: "", tintHover: "" };
      const shade = flatInk[color];
      const dark = flatInkDark[color];
      return {
        frame: neutral ? "bg-default/40" : `bg-${color}/20`,
        text: `text-${shade}${dark ? ` dark:text-${dark}` : ""}`,
        tint: `tint-${shade}${dark ? ` dark:tint-${dark}` : ""}`,
        hover: dim,
        tintHover: "",
      };
    }
    case "faded":
      return { frame: "border-2 border-default bg-default-100", ...accent, hover: dim, tintHover: "" };
    case "light":
      return { frame: "bg-transparent", ...accent, hover: neutral ? "hover:bg-default/40" : `hover:bg-${color}/20`, tintHover: "" };
    case "ghost":
      return {
        frame: `border-2 border-${color} bg-transparent`,
        ...accent,
        hover: neutral ? "hover:bg-default" : `hover:bg-${color} hover:text-${color}-foreground`,
        tintHover: neutral ? "" : `group-hover:tint-${color}-foreground`,
      };
  }
}
