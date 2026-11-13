// M3E status pieces: account marks, avatars, badges, chips, dividers, empty
// states and the snackbar.
import { createMotion, reduceMotion, Text, View, type JSX } from "nsh";
import { Avatar as BaseAvatar, Button as BaseButton } from "nsh/base";
import { tip } from "./tooltip";
import { Icon, type IconName } from "./icon";
import { focusRing } from "./tokens";

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
          ? `absolute inset-y-2 start-0 w-1 rounded-e-full ${color()}`
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

const avatarBox = { sm: "size-8", md: "size-10", lg: "size-14" } as const;
const avatarText = { sm: "text-label-medium", md: "text-title-medium", lg: "text-title-large" } as const;

/** A picture over initials on primary-container (Compose ListTokens' leading avatar). */
export function Avatar(props: { name: string; src?: string; size?: "sm" | "md" | "lg"; label?: string }) {
  const size = () => props.size ?? "md";
  return (
    <BaseAvatar
      name={props.name}
      src={props.src ?? ""}
      accessibilityLabel={props.label ?? ""}
      fallbackDelay={0}
      class={`${avatarBox[size()]} overflow-hidden rounded-full`}
      fallbackClass="items-center justify-center bg-primary-container"
      imageClass="size-full"
      fallback={<Text class={`${avatarText[size()]} font-medium text-on-primary-container`}>{initials(props.name)}</Text>}
    />
  );
}

/**
 * M3's badge: a 6pt dot, or a 16pt pill with a count (label-small) on error.
 * `quiet` is for counts that are no alert, such as unread mail in a list of
 * mailboxes: the count alone in on-surface-variant.
 */
export function Badge(props: { label?: string; dot?: boolean; quiet?: boolean; children?: JSX.Element }) {
  const content = () =>
    props.quiet ? (
      <Text class="text-label-large tabular-nums text-on-surface-variant">{props.label}</Text>
    ) : (
      <View class={`items-center justify-center rounded-full bg-error ${props.dot ? "size-1.5" : "h-4 min-w-4 px-1"}`}>
        <If when={!props.dot}>
          <Text class="text-label-small tabular-nums text-on-error">{props.label}</Text>
        </If>
      </View>
    );
  return (
    <If when={props.children} else={content()}>
      <View class="relative">
        {props.children}
        <View class={`absolute ${props.dot ? "end-0 top-0" : "-end-1.5 -top-1"}`} style={{ pointerEvents: "none" }}>
          {content()}
        </View>
      </View>
    </If>
  );
}

export interface ChipProps {
  label: string;
  /** assist (default), filter (toggles `selected`), input (removable), suggestion. */
  kind?: "assist" | "filter" | "input" | "suggestion";
  selected?: boolean;
  icon?: IconName;
  /** Content before the label in place of an icon: an avatar, a thumbnail. */
  start?: JSX.Element;
  /** Content after the label: a size, a count. */
  end?: JSX.Element;
  onPress?: () => void;
  /** An input chip's remove button; `closeLabel` names it. */
  onClose?: () => void;
  closeLabel?: string;
  accessibilityLabel?: string;
  tooltip?: string;
  class?: string;
  testID?: string;
}

/**
 * M3E chips: 32pt, small corners; a selected filter chip fills with
 * secondary-container. The chip squares to small while pressed.
 */
export function Chip(props: ChipProps) {
  const on = () => !!props.selected;
  const frame = () =>
    `group flex-row items-center self-start gap-2 h-8 rounded-small overflow-hidden ${props.start || props.icon || on() ? "ps-2" : "ps-4"} ${props.onClose ? "pe-2" : "pe-4"} ${
      on() ? "bg-secondary-container" : "border border-outline-variant"
    } ${props.class ?? ""}`;
  const ink = () => (on() ? "on-secondary-container" : "on-surface-variant");
  const inner = () => (
    <>
      <View accessibilityElementsHidden class={`absolute inset-0 bg-${ink()} opacity-0 transition-opacity duration-150 group-hover:opacity-8 group-active:opacity-10`} style={{ pointerEvents: "none" }} />
      {props.start ?? (on() && props.kind === "filter" ? <Icon name="check" size="sm" tone="on-secondary-container" /> : props.icon ? <Icon name={props.icon} size="sm" tone={on() ? "on-secondary-container" : "primary"} /> : null)}
      <Text class={`line-clamp-1 shrink text-label-large font-medium tracking-label-large ${on() ? "text-on-secondary-container" : "text-on-surface"}`}>{props.label}</Text>
      {props.end}
      <If when={props.onClose}>
        <BaseButton onPress={() => props.onClose?.()} accessibilityLabel={props.closeLabel ?? "Remove"} class={`size-6 items-center justify-center rounded-full hover:bg-on-surface/8 ${focusRing}`}>
          <Icon name="close" size="sm" tone={ink()} />
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
        selected={props.kind === "filter" ? on() : undefined}
        accessibilityLabel={props.accessibilityLabel}
        tooltip={tip(props.tooltip)}
        testID={props.testID}
        class={`${frame()} transition-all duration-350 ease-spatial-fast motion-reduce:transition-none ${focusRing}`}
      >
        {inner()}
      </BaseButton>
    </If>
  );
}

/** A 1pt rule in outline-variant. `inset` keeps 16pt clear at both ends (list dividers). */
export function Divider(props: { orientation?: "horizontal" | "vertical"; inset?: boolean; class?: string }) {
  return (
    <View
      accessibilityElementsHidden
      class={`shrink-0 bg-outline-variant ${props.orientation === "vertical" ? "h-full w-px" : `h-px ${props.inset ? "mx-4" : "w-full"}`} ${props.class ?? ""}`}
    />
  );
}

/** What a region shows with nothing in it, and the next step. */
export function Empty(props: { icon: IconName; title: string; description?: string; action?: JSX.Element }) {
  const enter = createMotion({ enter: reduceMotion() ? undefined : "fadeIn" });
  return (
    <View class="flex-1 items-center justify-center gap-4 p-8" style={enter.style}>
      <View class="size-16 items-center justify-center rounded-extra-large bg-surface-container-high">
        <Icon name={props.icon} size="lg" tone="on-surface-variant" />
      </View>
      <View class="items-center gap-1">
        <Text class="text-center text-title-medium font-medium tracking-title-medium text-on-surface">{props.title}</Text>
        <If when={props.description}>
          <Text class="max-w-80 text-center text-body-medium tracking-body-medium text-on-surface-variant">{props.description}</Text>
        </If>
      </View>
      {props.action}
    </View>
  );
}

/**
 * M3's snackbar: inverse-surface, extra-small corners, body-medium; an
 * optional action in inverse-primary. It rises from the bottom edge.
 */
export function Snackbar(props: { text: string; tone?: "neutral" | "error"; action?: { label: string; run: () => void } }) {
  const enter = createMotion({ enter: reduceMotion() ? undefined : "slideInUp", duration: 300 });
  return (
    <View
      role="alert"
      android={{ elevation: 3 }}
      class="min-h-12 w-full max-w-[560px] flex-row items-center gap-2 rounded-extra-small bg-inverse-surface py-1 pe-2 ps-4 shadow-elevation-3 android:shadow-none"
      style={enter.style}
    >
      <If when={props.tone === "error"}>
        <Icon name="close" size="sm" tone="inverse-on-surface" />
      </If>
      <Text class="flex-1 py-2 text-body-medium tracking-body-medium text-inverse-on-surface">{props.text}</Text>
      <If when={props.action}>
        {(action) => (
          <BaseButton onPress={() => action().run()} class={`h-10 justify-center rounded-full px-3 hover:bg-inverse-primary/8 active:bg-inverse-primary/10 ${focusRing}`} testID="toast-action">
            <Text class="text-label-large font-medium text-inverse-primary">{action().label}</Text>
          </BaseButton>
        )}
      </If>
    </View>
  );
}
