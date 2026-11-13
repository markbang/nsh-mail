// Small status pieces: account marks, avatars, badges, separators, empty
// states, the spinner and the toast.
import { createMotion, reduceMotion, Text, View, type JSX } from "nsh";
import { touch } from "./density";
import { Icon, type IconName } from "./icon";

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

/** Initials in a circle. */
export function Avatar(props: { name: string; size?: "sm" | "md" | "lg" }) {
  const initials = () => {
    const words = props.name.trim().split(/[\s@._-]+/).filter(Boolean);
    const first = words[0] ?? "?";
    // CJK names read best as their first character.
    if (/[\u3400-\u9fff]/.test(first)) return first.slice(0, 1);
    return ((first[0] ?? "") + (words[1]?.[0] ?? "")).toUpperCase();
  };
  const box = () => ({ sm: "size-7", md: touch ? "size-10" : "size-9", lg: "size-11" })[props.size ?? "md"];
  return (
    <View accessibilityElementsHidden class={`${box()} items-center justify-center rounded-full bg-muted`}>
      <Text class={`font-semibold text-muted-foreground ${props.size === "sm" ? "text-xs" : "text-sm"}`}>{initials()}</Text>
    </View>
  );
}

/** A short count or state. */
export function Badge(props: { label: string; tone?: "neutral" | "primary" }) {
  return (
    <View class={`rounded-full px-1.5 ${props.tone === "primary" ? "bg-primary" : "bg-muted"}`}>
      <Text class={`text-xs font-medium tabular-nums ${props.tone === "primary" ? "text-primary-foreground" : "text-muted-foreground"}`}>
        {props.label}
      </Text>
    </View>
  );
}

export function Separator(props: { class?: string }) {
  return <View class={`h-px bg-border ${props.class ?? ""}`} />;
}

/** What a region shows with nothing in it, and the next step. */
export function Empty(props: { icon: IconName; title: string; description?: string; action?: JSX.Element }) {
  const enter = createMotion({ enter: reduceMotion() ? undefined : "fadeIn" });
  return (
    <View class="flex-1 items-center justify-center gap-3 p-8" style={enter.style}>
      <Icon name={props.icon} size="lg" tone="muted-foreground" />
      <View class="items-center gap-1">
        <Text class={`text-center font-medium text-foreground ${touch ? "text-base" : "text-sm"}`}>{props.title}</Text>
        <If when={props.description}>
          <Text class={`max-w-80 text-center text-muted-foreground ${touch ? "text-sm" : "text-xs"}`}>{props.description}</Text>
        </If>
      </View>
      {props.action}
    </View>
  );
}

/** Work under way that has no known shape: a turning refresh glyph. */
export function Spinner(props: { label: string }) {
  const turn = createMotion();
  if (!reduceMotion()) turn.play({ duration: 900, iterations: "infinite", easing: "linear", keyframes: { rotate: ["0deg", "360deg"] } });
  return (
    <View accessibilityLabel={props.label} role="progressbar" style={turn.style}>
      <Icon name="refresh" size="sm" tone="muted-foreground" />
    </View>
  );
}

/** One message at the bottom edge that rises in and leaves on its own. */
export function Toast(props: { text: string; tone?: "neutral" | "error" }) {
  const enter = createMotion({ enter: reduceMotion() ? undefined : "slideInUp", duration: 200 });
  return (
    <View
      role="alert"
      class={`max-w-[440px] flex-row items-center gap-2 rounded-(--radius-lg) border border-border bg-popover px-4 py-2.5 shadow-lg`}
      style={enter.style}
    >
      <If when={props.tone === "error"}>
        <Icon name="close" size="sm" tone="destructive" />
      </If>
      <Text class={`text-popover-foreground ${touch ? "text-base" : "text-sm"}`}>{props.text}</Text>
    </View>
  );
}
