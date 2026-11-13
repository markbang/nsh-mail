// HeroUI's Button: seven variants × six colors, three sizes, the 0.97 press,
// the hover dim, the 2px focus ring and the disabled fade, on nsh/base's
// Button for presses, focus and the button role.
import { Text, token, View, type ImageAsset, type JSX } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { BrandMark, Icon, type IconName } from "./icon";
import { tip } from "./tooltip";
import { press } from "./motion";
import { Spinner } from "./feedback";
import { disabled, focusRing, rounded, shadow, type Color, type Radius, type Size, type Variant } from "./tokens";
import { look } from "./variants";

/** HeroUI's variants; `primary`, `default` and `danger` are the earlier names (solid primary, flat default, flat danger). */
export type ButtonVariant = Variant | "primary" | "default" | "danger";
export type ButtonSize = Size;

const legacy: Record<string, [Variant, Color]> = {
  primary: ["solid", "primary"],
  default: ["flat", "default"],
  danger: ["flat", "danger"],
};

export function resolve(variant: ButtonVariant | undefined, color: Color | undefined, fallback: Variant = "solid"): [Variant, Color] {
  const old = variant ? legacy[variant] : undefined;
  if (old) return [old[0], color ?? old[1]];
  return [(variant as Variant | undefined) ?? fallback, color ?? "default"];
}

const box: Record<Size, string> = {
  sm: "h-8 min-w-16 px-3 gap-2",
  md: "h-10 min-w-20 px-4 gap-2",
  lg: "h-12 min-w-24 px-6 gap-3",
};
const square: Record<Size, string> = { sm: "size-8", md: "size-10", lg: "size-12" };
const label: Record<Size, string> = { sm: "text-tiny", md: "text-small", lg: "text-medium" };
const corner: Record<Size, Radius> = { sm: "sm", md: "md", lg: "lg" };
const glyph = (size: Size) => (size === "sm" ? "sm" : "md");

const base = `group relative flex-row items-center justify-center overflow-hidden ${focusRing} ${disabled} ${press}`;

export interface ButtonProps {
  variant?: ButtonVariant;
  color?: Color;
  size?: ButtonSize;
  radius?: Radius;
  icon?: IconName;
  /** An icon after the label. */
  endIcon?: IconName;
  /** A brand's mark in its own colors, in the icon's place. */
  mark?: ImageAsset;
  label: string;
  disabled?: boolean;
  /** Work is under way: a spinner takes the icon's place and presses do nothing. */
  loading?: boolean;
  selected?: boolean;
  fullWidth?: boolean;
  onPress?: () => void;
  /** Layout from the caller only: width, flex, margin, alignment. */
  class?: string;
  testID?: string;
  tooltip?: string;
}

export function Button(props: ButtonProps) {
  const pair = () => resolve(props.variant, props.color);
  const l = () => look(...pair());
  const size = () => props.size ?? "md";
  return (
    <BaseButton
      onPress={props.onPress}
      disabled={props.disabled || props.loading}
      selected={props.selected}
      testID={props.testID}
      tooltip={tip(props.tooltip)}
      class={`${base} ${rounded[props.radius ?? corner[size()]]} ${l().frame} ${l().text} ${l().hover} ${box[size()]} ${props.loading ? "!opacity-100" : ""} ${props.fullWidth ? "self-stretch" : ""} ${props.class ?? ""}`}
    >
      <If when={!props.loading} else={<Spinner label={props.label} size="sm" class={l().text.replace(/text-/g, "border-b-")} />}>
        <If when={props.icon}>{(name) => <Icon name={name()} class={`${l().tint} ${l().tintHover}`} size={glyph(size())} />}</If>
        <If when={props.mark}>{(src) => <BrandMark src={src()} size={glyph(size())} />}</If>
      </If>
      <Text class={`line-clamp-1 font-normal ${label[size()]}`}>{props.label}</Text>
      <If when={props.endIcon}>{(name) => <Icon name={name()} class={`${l().tint} ${l().tintHover}`} size={glyph(size())} />}</If>
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
  /** HeroUI variant; default `light`. */
  variant?: ButtonVariant;
  color?: Color;
  radius?: Radius;
  /** An icon color over the variant's: a set flag, say. */
  tone?: string;
  selected?: boolean;
  disabled?: boolean;
  onPress?: () => void;
  class?: string;
  testID?: string;
}

/** HeroUI's `isIconOnly` button: square, the label for screen readers and the tooltip. */
export function IconButton(props: IconButtonProps) {
  const pair = () => resolve(props.variant, props.color, "light");
  const l = () => look(...pair());
  const size = () => props.size ?? "md";
  return (
    <BaseButton
      onPress={props.onPress}
      disabled={props.disabled}
      selected={props.selected}
      accessibilityLabel={props.label}
      testID={props.testID}
      tooltip={tip(props.shortcut ? `${props.label}  ${props.shortcut}` : props.label)}
      class={`${base} ${rounded[props.radius ?? corner[size()]]} ${l().frame} ${l().hover} ${square[size()]} ${props.class ?? ""}`}
    >
      <Icon
        name={props.icon}
        class={props.tone ? `tint-${props.tone}` : `${l().tint} ${l().tintHover}`}
        size={size() === "lg" ? "lg" : glyph(size())}
      />
    </BaseButton>
  );
}

/** A row of buttons with one gap. */
export function ButtonRow(props: { children?: JSX.Element; class?: string }) {
  return <View class={`flex-row flex-wrap items-center gap-2 ${props.class ?? ""}`}>{props.children}</View>;
}

/** HeroUI's ButtonGroup: attached buttons, inner corners squared by the caller's `radius="none"`. */
export function ButtonGroup(props: { children?: JSX.Element; label: string; class?: string }) {
  return (
    <View accessibilityLabel={props.label} class={`flex-row items-center overflow-hidden rounded-medium ${props.class ?? ""}`}>
      {props.children}
    </View>
  );
}

/** Material 3's floating action button, the screen's one main action at its bottom end (phones);
 * `extended` puts the label beside the icon. */
export function Fab(props: { icon: IconName; label: string; extended?: boolean; onPress: () => void; class?: string; testID?: string }) {
  return (
    <View
      role="button"
      accessibilityLabel={props.label}
      onPress={props.onPress}
      testID={props.testID}
      android={{ ripple: { color: `${token("primary-700").slice(0, 7)}1f` }, elevation: 3 }}
      class={`h-14 flex-row items-center justify-center gap-3 overflow-hidden rounded-[16px] bg-primary-100 ${props.extended ? "px-5" : "w-14"} ${shadow.md} android:shadow-none ${press} ${props.class ?? ""}`}
    >
      <Icon name={props.icon} class="tint-primary-700" />
      <If when={props.extended}>
        <Text class="text-medium font-medium text-primary-700">{props.label}</Text>
      </If>
    </View>
  );
}
