// Pressable rows: HeroUI's Listbox and its items for navigation, and the list
// row the message list is built from. Rows fill instead of scaling; a
// selected row keeps its fill under the pointer.
import { Text, token, View, type JSX } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { touch } from "./density";
import { tip } from "./tooltip";
import { Icon, type IconName } from "./icon";
import { wash } from "./motion";
import { material } from "./material";

const rowFocus = "focus-visible:outline-2 focus-visible:outline-focus focus-visible:-outline-offset-2";

/** A full-width pressable row: the message list's. Touch screens get the platform's press
 * feedback (Android's ripple) and long presses. */
export function Row(props: {
  selected?: boolean;
  onPress?: () => void;
  onLongPress?: () => void;
  accessibilityLabel?: string;
  tooltip?: string;
  testID?: string;
  class?: string;
  children?: JSX.Element;
}) {
  if (touch) {
    return (
      <View
        role="button"
        accessibilityLabel={props.accessibilityLabel}
        accessibilityState={{ selected: !!props.selected }}
        onPress={() => props.onPress?.()}
        onLongPress={props.onLongPress ? () => props.onLongPress?.() : undefined}
        android={{ ripple: { color: `${token("foreground").slice(0, 7)}1f` } }}
        testID={props.testID}
        class={`${props.selected ? "bg-selected" : "bg-background"} ios:active:bg-pressed ${wash} ${props.class ?? ""}`}
      >
        {props.children}
      </View>
    );
  }
  return (
    <BaseButton
      onPress={props.onPress}
      selected={props.selected}
      accessibilityLabel={props.accessibilityLabel}
      tooltip={tip(props.tooltip)}
      testID={props.testID}
      class={`${props.selected ? "bg-selected" : "hover:bg-hover"} active:bg-pressed ${rowFocus} ${wash} ${props.class ?? ""}`}
    >
      {props.children}
    </BaseButton>
  );
}

/** HeroUI's Listbox: a column of items with a 4pt gap. */
export function Listbox(props: { label: string; children?: JSX.Element; class?: string }) {
  return (
    <View accessibilityLabel={props.label} class={`gap-0.5 p-1 ${props.class ?? ""}`}>
      {props.children}
    </View>
  );
}

/** HeroUI's ListboxItem (flat): icon or mark, label, end content; the selected item fills with default-100. */
export function NavItem(props: {
  label: string;
  icon?: IconName;
  lead?: JSX.Element;
  detail?: string;
  /** End content in place of `detail`: a badge. */
  end?: JSX.Element;
  description?: string;
  tooltip?: string;
  selected?: boolean;
  onPress: () => void;
  testID?: string;
}) {
  return (
    <BaseButton
      selected={props.selected}
      onPress={props.onPress}
      tooltip={tip(props.tooltip)}
      testID={props.testID}
      class={`group flex-row items-center gap-2 rounded-small px-2 ${touch ? "min-h-11" : "min-h-8 py-1.5"} ${props.selected ? "bg-default-100" : "hover:bg-default/40"} active:bg-default/60 ${rowFocus} ${wash}`}
    >
      <View class="w-4 items-center">
        {props.lead ?? (props.icon ? <Icon name={props.icon} size="sm" tone={props.selected ? "foreground" : "default-500"} /> : null)}
      </View>
      <View class="min-w-0 flex-1">
        <Text class={`line-clamp-1 ${touch ? "text-medium" : "text-small"} ${props.selected ? "font-medium" : ""} text-foreground`}>{props.label}</Text>
        <If when={props.description}>
          <Text class="line-clamp-1 text-tiny text-foreground-500">{props.description}</Text>
        </If>
      </View>
      {props.end ?? (props.detail ? <Text class="text-tiny tabular-nums text-foreground-500">{props.detail}</Text> : null)}
    </BaseButton>
  );
}

/** Earlier name of `NavItem`. */
export const ListboxItem = NavItem;

/** HeroUI's ListboxSection title: a quiet tiny heading over a group. */
export function SectionLabel(props: { label: string; class?: string }) {
  return (
    <Text role="header" class={`px-2 pb-1 pt-3 ${material ? "text-label-l font-medium text-md-primary" : "text-tiny text-foreground-500"} ${props.class ?? ""}`}>
      {props.label}
    </Text>
  );
}
