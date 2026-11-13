// M3E buttons: common buttons, icon buttons, button groups, the split button
// and the FAB (the FAB menu is in m3.tsx). The container morphs its corners while pressed
// (and when a toggle is selected) on the fast spatial spring's curve; a state
// layer of the content color washes in on the fast effects one.
import { Text, View, type ImageAsset, type JSX } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { BrandMark, Icon, type IconName, type IconTone } from "./icon";
import { tip } from "./tooltip";
import { LoadingIndicator } from "./m3";
import { Menu, type MenuAction } from "./overlay";
import { focusRing, springCurve, type ColorRole } from "./tokens";

export type ButtonVariant = "filled" | "tonal" | "outlined" | "text" | "elevated";
export type IconButtonVariant = "standard" | "filled" | "tonal" | "outlined";
export type ButtonSize = "xs" | "sm" | "md";
export type ButtonShape = "round" | "square";

interface Ink {
  /** Container, border and shadow at rest. */
  frame: string;
  /** Label and icon color. */
  on: ColorRole;
  /** Container and content once a toggle is selected. */
  selected?: { frame: string; on: ColorRole };
}

const inks: Record<ButtonVariant, Ink> = {
  filled: { frame: "bg-primary", on: "on-primary", selected: { frame: "selected:bg-primary", on: "on-primary" } },
  tonal: { frame: "bg-secondary-container", on: "on-secondary-container", selected: { frame: "selected:bg-secondary", on: "on-secondary" } },
  outlined: { frame: "border border-outline-variant", on: "on-surface-variant", selected: { frame: "selected:border-0 selected:bg-inverse-surface", on: "inverse-on-surface" } },
  text: { frame: "", on: "primary" },
  elevated: { frame: "bg-surface-container-low shadow-elevation-1 hover:shadow-elevation-2", on: "primary", selected: { frame: "selected:bg-primary", on: "on-primary" } },
};
const errorInks: Partial<Record<ButtonVariant, Ink>> = {
  outlined: { frame: "border border-outline-variant", on: "error" },
  text: { frame: "", on: "error" },
};
const errorInk: Ink = { frame: "bg-error", on: "on-error" };

const iconInks: Record<IconButtonVariant, Ink> = {
  standard: { frame: "", on: "on-surface-variant", selected: { frame: "", on: "primary" } },
  filled: { frame: "bg-primary", on: "on-primary", selected: { frame: "selected:bg-primary", on: "on-primary" } },
  tonal: { frame: "bg-secondary-container", on: "on-secondary-container", selected: { frame: "selected:bg-secondary", on: "on-secondary" } },
  outlined: { frame: "border border-outline-variant", on: "on-surface-variant", selected: { frame: "selected:border-0 selected:bg-inverse-surface", on: "inverse-on-surface" } },
};
// An unselected filled toggle sits on surface-container (Compose's FilledIconButton toggle).
const unselectedFilled = "bg-surface-container";

// Compose ButtonXSmall/Small/MediumTokens: height, side padding, icon size, rest / pressed / selected corners.
const sizes: Record<ButtonSize, { h: string; px: string; gap: string; icon: number; square: string; pressed: string; label: string }> = {
  xs: { h: "h-8", px: "px-4", gap: "gap-2", icon: 20, square: "rounded-medium", pressed: "active:rounded-small", label: "text-label-large" },
  sm: { h: "h-10", px: "px-4", gap: "gap-2", icon: 20, square: "rounded-medium", pressed: "active:rounded-small", label: "text-label-large" },
  md: { h: "h-14", px: "px-6", gap: "gap-2", icon: 24, square: "rounded-large", pressed: "active:rounded-medium", label: "text-title-medium" },
};
// Compose X/Small/MediumIconButtonTokens: square size, icon, padding per width.
const iconSizes: Record<ButtonSize, { box: number; icon: number; pad: Record<"default" | "narrow" | "wide", number>; square: string; pressed: string; selectedRound: string }> = {
  xs: { box: 32, icon: 20, pad: { default: 6, narrow: 4, wide: 10 }, square: "rounded-medium", pressed: "active:rounded-small", selectedRound: "selected:rounded-medium" },
  sm: { box: 40, icon: 24, pad: { default: 8, narrow: 4, wide: 14 }, square: "rounded-medium", pressed: "active:rounded-small", selectedRound: "selected:rounded-medium" },
  md: { box: 56, icon: 24, pad: { default: 16, narrow: 12, wide: 24 }, square: "rounded-large", pressed: "active:rounded-medium", selectedRound: "selected:rounded-large" },
};

const morph = `transition-all duration-${springCurve.spatial.fast.ms} ease-spatial-fast motion-reduce:transition-none`;
const base = `group relative flex-row items-center justify-center overflow-hidden ${morph} ${focusRing} disabled:border-0 disabled:bg-on-surface/10 disabled:shadow-none`;

/** The state layer: the content color at 8% under the pointer, 10% pressed or focused. */
function StateLayer(props: { on: string }) {
  return (
    <View
      accessibilityElementsHidden
      class={`absolute inset-0 bg-${props.on} opacity-0 transition-opacity duration-150 ease-effects-fast group-hover:opacity-8 group-focus-visible:opacity-10 group-active:opacity-10 group-disabled:opacity-0`}
      style={{ pointerEvents: "none" }}
    />
  );
}

const iconPt = (n: number) => (n <= 20 ? "sm" : "md") as "sm" | "md";

export interface ButtonProps {
  label: string;
  variant?: ButtonVariant;
  size?: ButtonSize;
  shape?: ButtonShape;
  icon?: IconName;
  /** A brand's mark in its own colors, in the icon's place. */
  mark?: ImageAsset;
  /** Destructive: the error container. */
  tone?: "error";
  /** A toggle button: selected morphs round to square (square to round) and takes the selected colors. */
  selected?: boolean;
  disabled?: boolean;
  /** Work under way: the loading indicator takes the icon's place and presses do nothing. */
  loading?: boolean;
  fullWidth?: boolean;
  onPress?: () => void;
  /** Layout from the caller only: width, flex, margin, alignment. */
  class?: string;
  testID?: string;
  tooltip?: string;
}

export function Button(props: ButtonProps) {
  const ink = () => (props.tone === "error" ? (errorInks[props.variant ?? "filled"] ?? errorInk) : inks[props.variant ?? "filled"]);
  const s = () => sizes[props.size ?? "sm"];
  const toggle = () => props.selected !== undefined && !!ink().selected;
  const on = () => (toggle() && props.selected ? ink().selected!.on : ink().on);
  const corners = () => {
    const square = props.shape === "square";
    const rest = square ? s().square : "rounded-full";
    const picked = toggle() ? (square ? "selected:rounded-full" : `selected:${s().square}`) : "";
    return `${rest} ${picked} ${s().pressed}`;
  };
  return (
    <BaseButton
      onPress={props.onPress}
      disabled={props.disabled || props.loading}
      selected={toggle() ? props.selected : undefined}
      testID={props.testID}
      tooltip={tip(props.tooltip)}
      class={`${base} ${corners()} ${ink().frame} ${toggle() ? ink().selected!.frame : ""} ${s().h} ${s().px} ${s().gap} ${props.fullWidth ? "self-stretch" : ""} ${props.class ?? ""}`}
    >
      <StateLayer on={on()} />
      <If when={!props.loading} else={<LoadingIndicator label={props.label} size={s().icon} tone={on()} />}>
        <If when={props.icon}>{(name) => <Icon name={name()} size={iconPt(s().icon)} class={`tint-${on()} group-disabled:tint-on-surface/38`} />}</If>
        <If when={props.mark}>{(src) => <BrandMark src={src()} size={iconPt(s().icon)} />}</If>
      </If>
      <Text class={`line-clamp-1 ${s().label} font-medium text-${on()} group-disabled:text-on-surface/38`}>{props.label}</Text>
    </BaseButton>
  );
}

export interface IconButtonProps {
  icon: IconName;
  /** The accessible name, and the tooltip on desktops. */
  label: string;
  /** Shown after the label in the tooltip: a keyboard shortcut. */
  shortcut?: string;
  variant?: IconButtonVariant;
  size?: ButtonSize;
  width?: "default" | "narrow" | "wide";
  shape?: ButtonShape;
  /** A toggle: selected morphs round to square and takes the selected colors. */
  selected?: boolean;
  /** An icon color over the variant's: a set flag. */
  tone?: IconTone;
  disabled?: boolean;
  onPress?: () => void;
  class?: string;
  testID?: string;
}

export function IconButton(props: IconButtonProps) {
  const variant = () => props.variant ?? "standard";
  const ink = () => iconInks[variant()];
  const s = () => iconSizes[props.size ?? "sm"];
  const toggle = () => props.selected !== undefined;
  const on = (): IconTone => props.tone ?? (toggle() && props.selected ? ink().selected!.on : ink().on);
  const frame = () => (toggle() && variant() === "filled" ? `${unselectedFilled} ${ink().selected!.frame}` : `${ink().frame} ${toggle() ? ink().selected!.frame : ""}`);
  const corners = () => {
    const square = props.shape === "square";
    const picked = toggle() ? (square ? "selected:rounded-full" : s().selectedRound) : "";
    return `${square ? s().square : "rounded-full"} ${picked} ${s().pressed}`;
  };
  const width = () => s().icon + 2 * s().pad[props.width ?? "default"];
  return (
    <BaseButton
      onPress={props.onPress}
      disabled={props.disabled}
      selected={toggle() ? props.selected : undefined}
      accessibilityLabel={props.label}
      testID={props.testID}
      tooltip={tip(props.shortcut ? `${props.label}  ${props.shortcut}` : props.label)}
      class={`${base} ${corners()} ${frame()} ${props.class ?? ""}`}
      style={{ width: width(), height: s().box }}
    >
      <StateLayer on={on()} />
      <Icon name={props.icon} size={s().icon <= 20 ? "sm" : "md"} class={`tint-${on()} group-disabled:tint-on-surface/38`} />
    </BaseButton>
  );
}

/**
 * A button group: standard (12pt apart) or connected (2pt apart, inner corners
 * small, a selected item's fully round). Holds Buttons or IconButtons.
 */
export function ButtonGroup(props: { label: string; connected?: boolean; children?: JSX.Element; class?: string }) {
  return (
    <View accessibilityLabel={props.label} class={`flex-row items-center ${props.connected ? "gap-0.5" : "gap-3"} ${props.class ?? ""}`}>
      {props.children}
    </View>
  );
}

/** A connected button group choosing one of a few options (M3E's segmented replacement). */
export function Choice<T extends string>(props: {
  label: string;
  value: T;
  options: { value: T; label: string; icon?: IconName; testID?: string }[];
  onChange: (value: T) => void;
  fullWidth?: boolean;
  class?: string;
}) {
  const last = () => props.options.length - 1;
  // Connected group: the outer ends are round, inner corners small; the selected one goes fully round.
  const corner = (i: number) =>
    `${i === 0 ? "rounded-s-full" : "rounded-s-small"} ${i === last() ? "rounded-e-full" : "rounded-e-small"} active:rounded-extra-small selected:rounded-full`;
  return (
    <View accessibilityLabel={props.label} role="radiogroup" class={`flex-row gap-0.5 ${props.fullWidth ? "self-stretch" : "self-start"} ${props.class ?? ""}`}>
      <Each of={props.options} key={(o) => o.value}>
        {(option, index) => (
          <BaseButton
            selected={props.value === option().value}
            onPress={() => props.onChange(option().value)}
            accessibilityLabel={option().label}
            testID={option().testID}
            class={`${base} h-10 gap-2 px-4 ${corner(index())} bg-secondary-container selected:bg-secondary ${props.fullWidth ? "flex-1" : ""}`}
          >
            <StateLayer on={props.value === option().value ? "on-secondary" : "on-secondary-container"} />
            <If when={option().icon}>
              {(icon) => <Icon name={icon()} size="sm" class={props.value === option().value ? "tint-on-secondary" : "tint-on-secondary-container"} />}
            </If>
            <Text class={`text-label-large font-medium ${props.value === option().value ? "text-on-secondary" : "text-on-secondary-container"}`}>
              {option().label}
            </Text>
          </BaseButton>
        )}
      </Each>
    </View>
  );
}

/**
 * The split button: a leading action and a trailing menu button, 2pt apart,
 * inner corners extra-small that grow to medium when pressed; the menu button
 * goes round while its menu is open. `testID` names the leading button,
 * `menuTestID` the menu's.
 */
export function SplitButton(props: {
  label: string;
  icon?: IconName;
  variant?: "filled" | "tonal" | "outlined" | "elevated";
  onPress: () => void;
  /** Work under way: the loading indicator takes the icon's place; neither half takes presses. */
  loading?: boolean;
  menuLabel: string;
  actions: MenuAction[];
  /** Where the menu lines up with the menu button. */
  menuAlign?: "start" | "end";
  class?: string;
  testID?: string;
  menuTestID?: string;
}) {
  const ink = () => inks[props.variant ?? "filled"];
  const on = () => ink().on;
  return (
    <View class={`flex-row items-center gap-0.5 ${props.class ?? ""}`}>
      <BaseButton
        onPress={props.onPress}
        disabled={props.loading}
        testID={props.testID}
        class={`${base} h-10 gap-2 rounded-s-full rounded-e-extra-small ps-4 pe-3 active:rounded-e-medium hover:rounded-e-medium ${ink().frame}`}
      >
        <StateLayer on={on()} />
        <If when={!props.loading} else={<LoadingIndicator label={props.label} size={20} tone={on()} />}>
          <If when={props.icon}>{(name) => <Icon name={name()} size="sm" class={`tint-${on()} group-disabled:tint-on-surface/38`} />}</If>
        </If>
        <Text class={`text-label-large font-medium text-${on()} group-disabled:text-on-surface/38`}>{props.label}</Text>
      </BaseButton>
      <Menu
        label={props.menuLabel}
        actions={props.actions}
        align={props.menuAlign}
        trigger={
          <BaseButton
            accessibilityLabel={props.menuLabel}
            tooltip={tip(props.menuLabel)}
            disabled={props.loading}
            testID={props.menuTestID}
            class={`${base} h-10 w-12 rounded-s-extra-small rounded-e-full aria-expanded:rounded-full active:rounded-s-medium hover:rounded-s-medium ${ink().frame}`}
          >
            <StateLayer on={on()} />
            <Icon name="chevronDown" size="sm" class={`tint-${on()} group-disabled:tint-on-surface/38 transition-transform duration-350 ease-spatial-fast group-aria-expanded:rotate-180`} />
          </BaseButton>
        }
      />
    </View>
  );
}

export type FabSize = "sm" | "md" | "lg";
// Compose FabSmall/Baseline/LargeTokens: 40/56/96 square with medium/large/extra-large corners.
const fabBox: Record<FabSize, { box: string; corner: string; icon: "sm" | "md" | "lg" }> = {
  sm: { box: "h-10 min-w-10", corner: "rounded-medium", icon: "md" },
  md: { box: "h-14 min-w-14", corner: "rounded-large", icon: "md" },
  lg: { box: "h-24 min-w-24", corner: "rounded-extra-large", icon: "lg" },
};

/** The floating action button: a screen's one main action; `extended` puts the label beside the icon. */
export function Fab(props: { icon: IconName; label: string; extended?: boolean; size?: FabSize; onPress: () => void; class?: string; testID?: string }) {
  const s = () => fabBox[props.size ?? "md"];
  return (
    <BaseButton
      accessibilityLabel={props.label}
      onPress={props.onPress}
      testID={props.testID}
      tooltip={props.extended ? undefined : tip(props.label)}
      class={`${base} ${s().box} ${s().corner} gap-2 bg-primary-container shadow-elevation-3 hover:shadow-elevation-4 ${props.extended ? "px-4" : ""} ${props.class ?? ""}`}
    >
      <StateLayer on="on-primary-container" />
      <Icon name={props.icon} size={s().icon} class="tint-on-primary-container" />
      <If when={props.extended}>
        <Text class="text-label-large font-medium text-on-primary-container">{props.label}</Text>
      </If>
    </BaseButton>
  );
}
