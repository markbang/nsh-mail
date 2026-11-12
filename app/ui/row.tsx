// Pressable rows: list rows and navigation items. Rows deepen their fill
// when pressed instead of scaling; the selection is a fill, never an edge.
import { Text, View, type JSX } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { touch } from "./density";
import { Icon, type IconName } from "./icon";
import { wash } from "./motion";

export function Row(props: {
  selected?: boolean;
  onPress?: () => void;
  onLongPress?: () => void;
  accessibilityLabel?: string;
  testID?: string;
  class?: string;
  children?: JSX.Element;
}) {
  return (
    <BaseButton
      onPress={props.onPress}
      selected={props.selected}
      accessibilityLabel={props.accessibilityLabel}
      testID={props.testID}
      class={`hover:bg-hover active:bg-pressed selected:bg-selected focus-visible:outline-2 focus-visible:outline-ring focus-visible:-outline-offset-2 ${wash} ${props.class ?? ""}`}
    >
      {props.children}
    </BaseButton>
  );
}

/** A destination in a sidebar: icon or color mark, label, trailing detail. */
export function NavItem(props: {
  label: string;
  icon?: IconName;
  lead?: JSX.Element;
  detail?: string;
  selected?: boolean;
  onPress: () => void;
  testID?: string;
}) {
  return (
    <Row
      selected={props.selected}
      onPress={props.onPress}
      testID={props.testID}
      class={`flex-row items-center gap-2.5 rounded-(--radius) ${touch ? "min-h-11 px-3" : "h-8 px-2"}`}
    >
      <View class="w-4 items-center">
        {props.lead ?? (props.icon ? <Icon name={props.icon} size="sm" tone={props.selected ? "foreground" : "muted-foreground"} /> : null)}
      </View>
      <Text
        class={`line-clamp-1 flex-1 ${touch ? "text-base" : "text-sm"} ${props.selected ? "font-medium text-sidebar-foreground" : "text-sidebar-foreground"}`}
      >
        {props.label}
      </Text>
      <If when={props.detail}>
        <Text class="text-xs tabular-nums text-muted-foreground">{props.detail}</Text>
      </If>
    </Row>
  );
}

/** A quiet heading over a group of rows. */
export function SectionLabel(props: { label: string; class?: string }) {
  return (
    <Text role="header" class={`px-2 pb-1 pt-3 text-xs font-medium text-muted-foreground ${props.class ?? ""}`}>
      {props.label}
    </Text>
  );
}
