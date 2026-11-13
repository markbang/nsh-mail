// Interim: the ui layer's color × variant props drawn with M3 color roles until
// the components move to M3E's own variants.

export type Color = "default" | "primary" | "secondary" | "success" | "warning" | "danger";
export type Variant = "solid" | "bordered" | "light" | "flat" | "faded" | "shadow" | "ghost";
export type Size = "sm" | "md" | "lg";
export type Radius = "none" | "sm" | "md" | "lg" | "full";

interface Roles {
  fill: string;
  on: string;
  container: string;
  onContainer: string;
  ink: string;
}

/** M3 has no success or warning roles: they take tertiary. */
export const roles: Record<Color, Roles> = {
  default: { fill: "inverse-surface", on: "inverse-on-surface", container: "surface-container-highest", onContainer: "on-surface", ink: "on-surface" },
  primary: { fill: "primary", on: "on-primary", container: "primary-container", onContainer: "on-primary-container", ink: "primary" },
  secondary: { fill: "secondary", on: "on-secondary", container: "secondary-container", onContainer: "on-secondary-container", ink: "secondary" },
  success: { fill: "tertiary", on: "on-tertiary", container: "tertiary-container", onContainer: "on-tertiary-container", ink: "tertiary" },
  warning: { fill: "tertiary", on: "on-tertiary", container: "tertiary-container", onContainer: "on-tertiary-container", ink: "tertiary" },
  danger: { fill: "error", on: "on-error", container: "error-container", onContainer: "on-error-container", ink: "error" },
};

export const rounded: Record<Radius, string> = {
  none: "rounded-none",
  sm: "rounded-small",
  md: "rounded-medium",
  lg: "rounded-large",
  full: "rounded-full",
};

export const shadow = { none: "", sm: "shadow-elevation-1", md: "shadow-elevation-2", lg: "shadow-elevation-3" } as const;

export const disabled = "disabled:opacity-38";

export interface Look {
  frame: string;
  text: string;
  tint: string;
  hover: string;
  tintHover: string;
}

const pair = (role: string) => ({ text: `text-${role}`, tint: `tint-${role}` });
const dim = "hover:opacity-92";

export function look(variant: Variant, color: Color): Look {
  const r = roles[color];
  switch (variant) {
    case "solid":
      return { frame: `bg-${r.fill}`, ...pair(r.on), hover: dim, tintHover: "" };
    case "shadow":
      return { frame: `bg-${r.fill} shadow-elevation-2`, ...pair(r.on), hover: dim, tintHover: "" };
    case "bordered":
      return { frame: "border border-outline-variant bg-transparent", ...pair(r.ink), hover: "hover:bg-on-surface/8", tintHover: "" };
    case "flat":
      return { frame: `bg-${r.container}`, ...pair(r.onContainer), hover: dim, tintHover: "" };
    case "faded":
      return { frame: "border border-outline-variant bg-surface-container-high", ...pair(r.ink), hover: dim, tintHover: "" };
    case "light":
      return { frame: "bg-transparent", ...pair(r.ink), hover: "hover:bg-on-surface/8", tintHover: "" };
    case "ghost":
      return {
        frame: "border border-outline bg-transparent",
        ...pair(r.ink),
        hover: `hover:bg-${r.fill} hover:text-${r.on}`,
        tintHover: `group-hover:tint-${r.on}`,
      };
  }
}
