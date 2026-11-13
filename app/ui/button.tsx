// Buttons: one size scale sets height, padding, text and icon together; each
// variant owns its rest, hover, pressed, focus and disabled looks.
import { Text, View, type ImageAsset, type JSX } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { touch } from "./density";
import { BrandMark, Icon, type IconName, type IconTone } from "./icon";
import { press } from "./motion";

export type ButtonVariant = "primary" | "default" | "ghost" | "danger";
export type ButtonSize = "sm" | "md" | "lg";

const frame: Record<ButtonVariant, string> = {
  primary: "bg-primary hover:bg-primary-hover active:bg-primary-hover",
  default: "border border-border bg-background hover:bg-hover active:bg-pressed",
  ghost: "hover:bg-hover active:bg-pressed",
  danger: "border border-border bg-background hover:bg-destructive/10 active:bg-destructive/20",
};
const ink: Record<ButtonVariant, string> = {
  primary: "text-primary-foreground",
  default: "text-foreground",
  ghost: "text-foreground",
  danger: "text-destructive",
};
const tone: Record<ButtonVariant, IconTone> = {
  primary: "primary-foreground",
  default: "foreground",
  ghost: "foreground",
  danger: "destructive",
};
const box: Record<ButtonSize, string> = touch
  ? { sm: "min-h-9 px-3 gap-1.5", md: "min-h-11 px-4 gap-2", lg: "min-h-12 px-5 gap-2" }
  : { sm: "h-7 px-2.5 gap-1.5", md: "h-8 px-3 gap-2", lg: "h-10 px-4 gap-2" };
const label: Record<ButtonSize, string> = touch
  ? { sm: "text-sm", md: "text-base", lg: "text-base" }
  : { sm: "text-xs", md: "text-sm", lg: "text-base" };
const square: Record<ButtonSize, string> = touch
  ? { sm: "size-9", md: "size-11", lg: "size-12" }
  : { sm: "size-7", md: "size-8", lg: "size-10" };

const base =
  "flex-row items-center justify-center rounded-(--radius) focus-visible:outline-2 focus-visible:outline-ring focus-visible:outline-offset-2 disabled:opacity-50";

export interface ButtonProps {
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: IconName;
  /** A brand's mark in its own colors, in the icon's place. */
  mark?: ImageAsset;
  label: string;
  disabled?: boolean;
  /** Work is under way: presses do nothing and the label dims, size kept. */
  loading?: boolean;
  selected?: boolean;
  onPress?: () => void;
  /** Layout from the caller only: width, flex, margin, alignment. */
  class?: string;
  testID?: string;
  tooltip?: string;
}

export function Button(props: ButtonProps) {
  const variant = () => props.variant ?? "default";
  const size = () => props.size ?? "md";
  return (
    <BaseButton
      onPress={props.onPress}
      disabled={props.disabled || props.loading}
      selected={props.selected}
      testID={props.testID}
      tooltip={props.tooltip}
      class={`${base} ${press} ${frame[variant()]} ${box[size()]} ${props.loading ? "opacity-70" : ""} ${props.class ?? ""}`}
    >
      <If when={props.icon}>{(name) => <Icon name={name()} tone={tone[variant()]} size={size() === "lg" ? "md" : "sm"} />}</If>
      <If when={props.mark}>{(src) => <BrandMark src={src()} size={size() === "lg" ? "md" : "sm"} />}</If>
      <Text class={`line-clamp-1 font-medium ${label[size()]} ${ink[variant()]}`}>
        {props.label}
      </Text>
    </BaseButton>
  );
}

export interface IconButtonProps {
  icon: IconName;
  /** The accessible name, and the tooltip on desktops. */
  label: string;
  /** Shown after the label in the tooltip: a keyboard shortcut. */
  shortcut?: string;
  size?: ButtonSize;
  variant?: "ghost" | "default";
  tone?: IconTone;
  selected?: boolean;
  disabled?: boolean;
  onPress?: () => void;
  class?: string;
  testID?: string;
}

export function IconButton(props: IconButtonProps) {
  const size = () => props.size ?? "md";
  return (
    <BaseButton
      onPress={props.onPress}
      disabled={props.disabled}
      selected={props.selected}
      accessibilityLabel={props.label}
      testID={props.testID}
      tooltip={touch ? undefined : props.shortcut ? `${props.label}  ${props.shortcut}` : props.label}
      class={`${base} ${press} ${frame[props.variant ?? "ghost"]} ${square[size()]} selected:bg-selected ${props.class ?? ""}`}
    >
      <Icon name={props.icon} tone={props.tone ?? "foreground"} size={size() === "lg" ? "md" : size() === "sm" ? "sm" : "md"} />
    </BaseButton>
  );
}

/** A row of buttons with one gap. */
export function ButtonRow(props: { children?: JSX.Element; class?: string }) {
  return <View class={`flex-row flex-wrap items-center gap-2 ${props.class ?? ""}`}>{props.children}</View>;
}
