// Status pieces in HeroUI's look: account marks, Avatar, Badge, Chip, Kbd,
// Divider, empty states, Spinner and the toast.
import { createMotion, reduceMotion, Text, View, type JSX } from "nsh";
import { Avatar as BaseAvatar, Button as BaseButton } from "nsh/base";
import { tip } from "./tooltip";
import { Icon, type IconName } from "./icon";
import { press } from "./motion";
import { touch } from "./density";
import { focusRing, rounded, shadow, type Color, type Radius, type Size, type Variant } from "./tokens";
import { look } from "./variants";

const marks = ["bg-chart-1", "bg-chart-2", "bg-chart-3", "bg-chart-4", "bg-chart-5"];

/** An account's color: the nth account takes the nth chart color. A dot next
 * to a name, or a bar along a list row's start edge. */
export function AccountMark(props: { index: number; size?: "sm" | "md"; shape?: "dot" | "bar" }) {
  const color = () => marks[Math.max(0, props.index) % marks.length];
  return (
    <View
      accessibilityElementsHidden
      class={
        props.shape === "bar"
          ? `absolute inset-y-0 start-0 w-[3px] ${color()}`
          : `rounded-full ${props.size === "md" ? "size-2.5" : "size-2"} ${color()}`
      }
    />
  );
}

/** Initials as people read them: CJK names by their first character. */
export function initials(name: string) {
  const words = name.trim().split(/[\s@._-]+/).filter(Boolean);
  const first = words[0] ?? "?";
  if (/[\u3400-\u9fff]/.test(first)) return first.slice(0, 1);
  return ((first[0] ?? "") + (words[1]?.[0] ?? "")).toUpperCase();
}

const avatarBox: Record<Size, string> = { sm: "size-8", md: "size-10", lg: "size-14" };
const avatarText: Record<Size, string> = { sm: "text-tiny", md: "text-tiny", lg: "text-small" };

/** HeroUI's Avatar: a picture over initials on a solid color, full radius by default. */
export function Avatar(props: {
  name: string;
  src?: string;
  size?: Size;
  color?: Color;
  radius?: Radius;
  bordered?: boolean;
  /** Screen readers skip it by default: the name is next to it. */
  label?: string;
}) {
  const size = () => props.size ?? "md";
  const c = () => props.color ?? "default";
  const ring = () => (props.bordered ? `outline-2 outline-offset-2 outline-${c() === "default" ? "default" : c()}` : "");
  return (
    <BaseAvatar
      name={props.name}
      src={props.src ?? ""}
      accessibilityLabel={props.label ?? ""}
      fallbackDelay={0}
      class={`${avatarBox[size()]} ${rounded[props.radius ?? "full"]} overflow-hidden ${ring()}`}
      fallbackClass={`items-center justify-center bg-${c()}`}
      imageClass="size-full"
      fallback={<Text class={`${avatarText[size()]} text-${c()}-foreground`}>{initials(props.name)}</Text>}
    />
  );
}

const badgeSize: Record<Size, string> = {
  sm: "min-w-4 h-4 px-1 text-tiny",
  md: "min-w-5 h-5 px-1 text-small",
  lg: "min-w-6 h-6 px-1 text-small",
};

/**
 * HeroUI's Badge: a count or a dot. With children it sits on their top end
 * corner, outlined in the background; without, it's an inline pill.
 */
export function Badge(props: {
  label?: string;
  color?: Color;
  variant?: "solid" | "flat" | "faded" | "shadow";
  size?: Size;
  dot?: boolean;
  invisible?: boolean;
  /** Earlier name for `color`: `neutral` is flat default, `primary` solid primary. */
  tone?: "neutral" | "primary";
  children?: JSX.Element;
}) {
  const c = (): Color => props.color ?? (props.tone === "primary" ? "primary" : "default");
  const v = (): Variant => props.variant ?? (props.tone === "neutral" || (!props.tone && !props.color) ? "flat" : "solid");
  const l = () => look(v(), c());
  const pill = () =>
    `items-center justify-center rounded-full ${l().frame} ${l().text} ${props.dot ? "size-2.5" : badgeSize[props.size ?? "sm"]} transition duration-250 ${props.invisible ? "scale-0 opacity-0" : ""}`;
  const content = () => (
    <View class={pill()}>
      <If when={!props.dot}>
        <Text class="font-normal tabular-nums">{props.label}</Text>
      </If>
    </View>
  );
  return (
    <If when={props.children} else={content()}>
      <View class="relative">
        {props.children}
        <View class="absolute -end-1.5 -top-1.5 rounded-full border-2 border-background">{content()}</View>
      </View>
    </If>
  );
}

const chipBox: Record<Size, string> = { sm: "h-6 px-1 gap-0.5", md: "h-7 px-1 gap-1", lg: "h-8 px-2 gap-1" };
const chipText: Record<Size, string> = { sm: "px-1 text-tiny", md: "px-2 text-small", lg: "px-2 text-medium" };

export interface ChipProps {
  label: string;
  variant?: Variant | "dot";
  color?: Color;
  size?: Size;
  radius?: Radius;
  icon?: IconName;
  /** Content before the label in place of an icon: an avatar, a thumbnail. */
  start?: JSX.Element;
  /** Content after the label: a size, a count. */
  end?: JSX.Element;
  /** A press opens what the chip stands for (an attachment). */
  onPress?: () => void;
  /** Shows HeroUI's close button; `closeLabel` names it for screen readers. */
  onClose?: () => void;
  closeLabel?: string;
  accessibilityLabel?: string;
  tooltip?: string;
  class?: string;
  testID?: string;
}

/** HeroUI's Chip: a compact label, optionally pressable and closable. */
export function Chip(props: ChipProps) {
  const size = () => props.size ?? "md";
  const dot = () => props.variant === "dot";
  const l = () => (dot() ? look("bordered", "default") : look(props.variant as Variant ?? "solid", props.color ?? "default"));
  const frame = () =>
    `flex-row items-center self-start ${chipBox[size()]} ${rounded[props.radius ?? "full"]} ${dot() ? "border-2 border-default bg-transparent" : l().frame} ${dot() ? "text-foreground" : l().text} ${props.class ?? ""}`;
  const inner = () => (
    <>
      <If when={dot()}>
        <View class={`ms-1 size-2 rounded-full ${props.color && props.color !== "default" ? `bg-${props.color}` : "bg-default-400"}`} />
      </If>
      {props.start ?? (props.icon ? <Icon name={props.icon} size="sm" class={l().tint} /> : null)}
      <Text class={`line-clamp-1 shrink font-normal ${chipText[size()]}`}>{props.label}</Text>
      {props.end}
      <If when={props.onClose}>
        <BaseButton
          onPress={() => props.onClose?.()}
          accessibilityLabel={props.closeLabel ?? "Remove"}
          class={`rounded-full opacity-70 transition-opacity hover:opacity-100 active:opacity-50 ${focusRing}`}
        >
          <Icon name="close" size="sm" class={l().tint} />
        </BaseButton>
      </If>
    </>
  );
  return (
    <If
      when={props.onPress}
      else={
        <View class={frame()} accessibilityLabel={props.accessibilityLabel} testID={props.testID}>
          {inner()}
        </View>
      }
    >
      <BaseButton
        onPress={() => props.onPress?.()}
        accessibilityLabel={props.accessibilityLabel}
        tooltip={tip(props.tooltip)}
        testID={props.testID}
        class={`${frame()} ${press} ${focusRing} ${l().hover}`}
      >
        {inner()}
      </BaseButton>
    </If>
  );
}

/** HeroUI's Kbd: a key or shortcut. */
export function Kbd(props: { keys: string; label?: string }) {
  return (
    <View accessibilityLabel={props.label} class={`self-start rounded-small bg-default-100 px-1.5 py-0.5 ${shadow.sm}`}>
      <Text class="text-small text-foreground-600">{props.keys}</Text>
    </View>
  );
}

/** HeroUI's Divider: a 1px rule in the divider color. */
export function Divider(props: { orientation?: "horizontal" | "vertical"; class?: string }) {
  return (
    <View
      accessibilityElementsHidden
      class={`shrink-0 bg-divider ${props.orientation === "vertical" ? "h-full w-px" : "h-px w-full"} ${props.class ?? ""}`}
    />
  );
}

/** Earlier name of `Divider`. */
export const Separator = Divider;

/** What a region shows with nothing in it, and the next step. */
export function Empty(props: { icon: IconName; title: string; description?: string; action?: JSX.Element }) {
  const enter = createMotion({ enter: reduceMotion() ? undefined : "fadeIn" });
  return (
    <View class="flex-1 items-center justify-center gap-3 p-8" style={enter.style}>
      <Icon name={props.icon} size="lg" tone="default-400" />
      <View class="items-center gap-1">
        <Text class="text-center text-medium font-medium text-foreground">{props.title}</Text>
        <If when={props.description}>
          <Text class="max-w-80 text-center text-small text-foreground-500">{props.description}</Text>
        </If>
      </View>
      {props.action}
    </View>
  );
}

const spinnerBox: Record<Size, string> = { sm: "size-5 border-2", md: "size-8 border-3", lg: "size-10 border-3" };

/**
 * HeroUI's Spinner: two arcs turning at 0.8s, one easing and one linear and
 * dotted. Reduced motion leaves them resting.
 */
export function Spinner(props: { label: string; size?: Size; color?: Color | "current"; class?: string; showLabel?: boolean }) {
  const arc = () =>
    props.class ?? (props.color === "current" ? "border-b-current" : `border-b-${props.color ?? "primary"}`);
  const ring = () => `absolute inset-0 rounded-full border-t-transparent border-l-transparent border-r-transparent ${spinnerBox[props.size ?? "md"]} ${arc()}`;
  const turn = (easing: "ease" | "linear") =>
    reduceMotion()
      ? undefined
      : ({ animationName: "spin", animationDuration: 800, animationIterationCount: "infinite", animationTimingFunction: easing } as const);
  return (
    <View accessibilityLabel={props.label} role="progressbar" class="items-center gap-2">
      <View class={`relative ${spinnerBox[props.size ?? "md"].split(" ")[0]}`}>
        <View class={`${ring()} border-solid`} style={turn("ease")} />
        <View class={`${ring()} border-dotted opacity-75`} style={turn("linear")} />
      </View>
      <If when={props.showLabel}>
        <Text class="text-small text-foreground">{props.label}</Text>
      </If>
    </View>
  );
}

/** One message at the bottom edge that rises in and leaves on its own. */
/** A short message at the bottom, with at most one action (Undo). Touch screens get Material 3's
 * snackbar (an inverse surface), desktops HeroUI's toast. */
export function Toast(props: { text: string; tone?: "neutral" | "error"; action?: { label: string; run: () => void } }) {
  const enter = createMotion({ enter: reduceMotion() ? undefined : "slideInUp", duration: 250 });
  return (
    <View
      role="alert"
      android={{ elevation: 3 }}
      class={
        touch
          ? "min-h-12 w-full max-w-[560px] flex-row items-center gap-3 rounded-small bg-foreground-800 py-1.5 ps-4 pe-2"
          : `max-w-[440px] flex-row items-center gap-3 rounded-large border border-default-200 bg-content1 px-4 py-3 ${shadow.sm}`
      }
      style={enter.style}
    >
      <If when={props.tone === "error" && !touch}>
        <Icon name="close" size="sm" tone="danger" />
      </If>
      <Text class={`flex-1 py-2 text-small ${touch ? "text-background" : "text-content1-foreground"}`}>{props.text}</Text>
      <If when={props.action}>
        {(action) => (
          <BaseButton
            onPress={() => action().run()}
            class={`min-h-10 items-center justify-center rounded-full px-3 active:bg-primary-300/20 ${focusRing}`}
            testID="toast-action"
          >
            <Text class={`text-small font-medium ${touch ? "text-primary-300" : "text-primary"}`}>{action().label}</Text>
          </BaseButton>
        )}
      </If>
    </View>
  );
}
