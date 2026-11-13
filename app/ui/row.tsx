// Pressable rows: HeroUI's Listbox and its items for navigation, and the list
// row the message list is built from. Rows fill instead of scaling; a
// selected row keeps its fill under the pointer.
import { Text, View, type JSX } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { touch } from "./density";
import { tip } from "./tooltip";
import { Icon, type IconName } from "./icon";
import { wash } from "./motion";

const rowFocus = "focus-visible:outline-2 focus-visible:outline-secondary focus-visible:-outline-offset-2";

/** A full-width pressable row: the message list's. */
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
  return (
    <BaseButton
      onPress={props.onPress}
      selected={props.selected}
      accessibilityLabel={props.accessibilityLabel}
      tooltip={tip(props.tooltip)}
      testID={props.testID}
      class={`${props.selected ? "bg-secondary-container" : "hover:bg-on-surface/8"} active:bg-on-surface/10 ${rowFocus} ${wash} ${props.class ?? ""}`}
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
      class={`group flex-row items-center gap-2 rounded-small px-2 ${touch ? "min-h-11" : "min-h-8 py-1.5"} ${props.selected ? "bg-surface-container-high" : "hover:bg-on-surface/8"} active:bg-on-surface/10 ${rowFocus} ${wash}`}
    >
      <View class="w-4 items-center">
        {props.lead ?? (props.icon ? <Icon name={props.icon} size="sm" tone={props.selected ? "on-secondary-container" : "on-surface-variant"} /> : null)}
      </View>
      <View class="min-w-0 flex-1">
        <Text class={`line-clamp-1 ${touch ? "text-body-large" : "text-body-medium"} ${props.selected ? "font-medium" : ""} text-on-surface`}>{props.label}</Text>
        <If when={props.description}>
          <Text class="line-clamp-1 text-body-small text-on-surface-variant">{props.description}</Text>
        </If>
      </View>
      {props.end ?? (props.detail ? <Text class="text-body-small tabular-nums text-on-surface-variant">{props.detail}</Text> : null)}
    </BaseButton>
  );
}

/** Earlier name of `NavItem`. */
export const ListboxItem = NavItem;

/** HeroUI's ListboxSection title: a quiet tiny heading over a group. */
export function SectionLabel(props: { label: string; class?: string }) {
  return (
    <Text role="header" class={`px-2 pb-1 pt-3 text-body-small text-on-surface-variant ${props.class ?? ""}`}>
      {props.label}
    </Text>
  );
}
