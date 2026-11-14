// A source's mark in the feed and in Settings: its icon on its own colour,
// the priority dot of a high-priority notification, and a tag's pill. Colours
// are the theme's chart roles, so light and dark come from the same token.
import { Text, View } from "nsh";
import type { Source } from "nsh-mail/native";
import { Icon, type IconName } from "../ui";

/** The icon of each kind, as its service is drawn everywhere. */
const kindIcons: Record<Source["kind"], IconName> = {
  mail: "mail",
  webhook: "bolt",
  gotify: "bellRing",
  ntfy: "message",
  telegram: "send",
  discord: "gamepad",
  slack: "hash",
  feishu: "feather",
  dingtalk: "sparkles",
  wecom: "users",
};

/** The icons a source may take over its kind's. */
const overrides = ["bell", "bellRing", "bolt", "message", "hash", "users", "send", "activity", "terminal", "mail", "clock", "alert", "gamepad", "feather", "sparkles", "tag"];

/** The icon of a kind, as a source takes it until it is given another. */
export const kindIcon = (kind: Source["kind"]): IconName => kindIcons[kind];

export const sourceIcon = (source: Source): IconName =>
  source.icon && overrides.includes(source.icon) ? (source.icon as IconName) : kindIcons[source.kind];

/** A source's colour as classes: the theme's chart roles, never a raw hex. */
export const sourceColours = [
  { id: "chart-1", mark: "bg-chart-1/15", ink: "tint-chart-1", dot: "bg-chart-1" },
  { id: "chart-2", mark: "bg-chart-2/15", ink: "tint-chart-2", dot: "bg-chart-2" },
  { id: "chart-3", mark: "bg-chart-3/15", ink: "tint-chart-3", dot: "bg-chart-3" },
  { id: "chart-4", mark: "bg-chart-4/15", ink: "tint-chart-4", dot: "bg-chart-4" },
  { id: "chart-5", mark: "bg-chart-5/15", ink: "tint-chart-5", dot: "bg-chart-5" },
] as const;

const colour = (source: Source) => sourceColours.find((c) => c.id === source.color) ?? sourceColours[0];

/** A source's icon in a rounded tile of its colour. */
export function SourceMark(props: { source: Source; size?: "sm" | "md" }) {
  const small = () => props.size === "sm";
  return (
    <View class={`items-center justify-center rounded-medium ${small() ? "size-8" : "size-10"} ${colour(props.source).mark}`}>
      <Icon name={sourceIcon(props.source)} size={small() ? "sm" : "md"} class={colour(props.source).ink} />
    </View>
  );
}

/** A high-priority notification's dot: the error colour, as mail's unread dot is the primary one. */
export function PriorityDot(props: { priority: number }) {
  return (
    <If when={props.priority >= 7}>
      <View accessibilityLabel="" class="size-2 rounded-full bg-error" />
    </If>
  );
}

/** A tag, as the feed and the reader list them: a small outlined pill. */
export function Tags(props: { tags: string[] }) {
  return (
    <If when={props.tags.length}>
      <View class="flex-row flex-wrap items-center gap-1">
        <Each of={props.tags} key={(tag) => tag}>
          {(tag) => (
            <View class="rounded-full border border-outline-variant px-2 py-0.5">
              <Text class="text-label-small tracking-label-small text-on-surface-variant">{tag()}</Text>
            </View>
          )}
        </Each>
      </View>
    </If>
  );
}
