// M3E lists and the navigation rail. List items morph their corners with
// their state (Compose ListTokens' expressive shapes): extra-small at rest,
// medium under the pointer, large when pressed or selected, the selected one
// filled with secondary-container. Rail items put the label beside a 56pt
// pill (expanded) or under a 56×32 one (collapsed).
import { Scroll, Text, token, View, type JSX } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { touch } from "./density";
import { tip } from "./tooltip";
import { Icon, type IconName } from "./icon";

const shapeMorph = "transition-all duration-350 ease-spatial-fast motion-reduce:transition-none";
// Touch rows only recolor: a moving shape under a finger fights the swipe around them.
const wash = "transition-colors duration-150 ease-effects-fast motion-reduce:transition-none";
const rowFocus = "focus-visible:outline-3 focus-visible:outline-secondary focus-visible:-outline-offset-3";

/** The state layer of a row: on-surface at 8% hovered, 10% pressed. */
function Layer(props: { on?: string }) {
  const on = () => props.on ?? "on-surface";
  return (
    <View
      accessibilityElementsHidden
      class={`absolute inset-0 bg-${on()} opacity-0 transition-opacity duration-150 ease-effects-fast group-hover:opacity-8 group-active:opacity-10`}
      style={{ pointerEvents: "none" }}
    />
  );
}

/** A pressable list row whose content the caller lays out: the message list's.
 * Touch screens get the platform's press feedback (Android's ripple) and long presses. */
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
  const shape = () => (props.selected ? "rounded-large bg-secondary-container" : "rounded-extra-small bg-surface");
  if (touch) {
    return (
      <View
        role="button"
        accessibilityLabel={props.accessibilityLabel}
        accessibilityState={{ selected: !!props.selected }}
        onPress={() => props.onPress?.()}
        onLongPress={props.onLongPress ? () => props.onLongPress?.() : undefined}
        android={{ ripple: { color: `${token("on-surface").slice(0, 7)}1f` } }}
        testID={props.testID}
        class={`${props.selected ? "bg-secondary-container" : "bg-surface"} ios:active:bg-on-surface/10 ${wash} ${props.class ?? ""}`}
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
      class={`group relative overflow-hidden ${shape()} ${props.selected ? "" : "hover:rounded-medium active:rounded-large"} ${shapeMorph} ${rowFocus} ${props.class ?? ""}`}
    >
      <Layer on={props.selected ? "on-secondary-container" : "on-surface"} />
      {props.children}
    </BaseButton>
  );
}

/** M3's one- to three-line list item: leading, headline, supporting text, trailing. */
export function ListItem(props: {
  headline: string;
  supporting?: string;
  overline?: string;
  leading?: JSX.Element;
  icon?: IconName;
  trailing?: JSX.Element;
  selected?: boolean;
  onPress?: () => void;
  testID?: string;
  class?: string;
}) {
  const on = () => !!props.selected;
  const body = () => (
    <>
      {props.leading ?? (props.icon ? <Icon name={props.icon} tone={on() ? "on-secondary-container" : "on-surface-variant"} /> : null)}
      <View class="min-w-0 flex-1">
        <If when={props.overline}>
          <Text class="text-label-small tracking-label-small text-on-surface-variant">{props.overline}</Text>
        </If>
        <Text class={`line-clamp-1 text-body-large tracking-body-large ${on() ? "text-on-secondary-container" : "text-on-surface"}`}>{props.headline}</Text>
        <If when={props.supporting}>
          <Text class={`line-clamp-2 text-body-medium tracking-body-medium ${on() ? "text-on-secondary-container" : "text-on-surface-variant"}`}>{props.supporting}</Text>
        </If>
      </View>
      {props.trailing}
    </>
  );
  const frame = `min-h-14 flex-row items-center gap-4 px-4 py-2 ${props.class ?? ""}`;
  return (
    <If when={props.onPress} else={<View class={frame} testID={props.testID}>{body()}</View>}>
      <Row selected={props.selected} onPress={props.onPress} testID={props.testID} class={frame}>
        {body()}
      </Row>
    </If>
  );
}

/** A quiet heading over a group of list items or rail items. */
export function SectionLabel(props: { label: string; class?: string }) {
  return (
    <Text role="header" class={`px-4 pb-2 pt-4 text-title-small font-medium tracking-title-small text-on-surface-variant ${props.class ?? ""}`}>
      {props.label}
    </Text>
  );
}

export interface RailItem {
  key: string;
  label: string;
  icon?: IconName;
  /** In the icon's place: an account's mark. */
  lead?: JSX.Element;
  /** An unread count, after the label (expanded) or on the icon (collapsed). */
  badge?: number;
  tooltip?: string;
  /** A heading over this item and the ones after it (expanded rails). */
  section?: string;
  testID?: string;
}

/**
 * M3E's navigation rail on the window's start edge: collapsed 96pt (icons
 * over labels) or expanded 220–360pt (labels beside icons, sections). The
 * header holds the menu button and the FAB; the footer the quiet status.
 */
export function NavigationRail(props: {
  label: string;
  items: RailItem[];
  selected: string;
  onSelect: (key: string) => void;
  expanded?: boolean;
  header?: JSX.Element;
  footer?: JSX.Element;
  testID?: string;
}) {
  const count = (n?: number) => (n ? (n > 999 ? "999+" : String(n)) : "");
  return (
    <View
      role="navigation"
      accessibilityLabel={props.label}
      testID={props.testID}
      class={`h-full bg-surface ${props.expanded ? "w-full" : "w-24 items-center"}`}
    >
      <View class={`gap-2 pb-4 pt-5 ${props.expanded ? "px-5" : "items-center"}`}>{props.header}</View>
      <Scroll class="flex-1" contentClass={props.expanded ? "px-3 pb-3" : "items-center gap-1 pb-3"}>
        <Each of={props.items} key={(item) => item.key}>
          {(item) => {
            const on = () => props.selected === item().key;
            const icon = () =>
              item().lead ?? (item().icon ? <Icon name={item().icon!} size="md" tone={on() ? "on-secondary-container" : "on-surface-variant"} /> : null);
            return (
              <>
                <If when={item().section}>
                  <If when={props.expanded} else={<View class="my-2 h-px w-10 bg-outline-variant" />}>
                    <Text role="header" class="px-4 pb-2 pt-5 text-title-small font-medium tracking-title-small text-on-surface-variant">
                      {item().section}
                    </Text>
                  </If>
                </If>
                <If
                  when={props.expanded}
                  else={
                    <BaseButton
                      onPress={() => props.onSelect(item().key)}
                      selected={on()}
                      accessibilityLabel={item().badge ? `${item().label}, ${count(item().badge)}` : item().label}
                      tooltip={tip(item().tooltip ?? item().label)}
                      testID={item().testID}
                      class={`group w-20 items-center gap-1 py-1.5 ${rowFocus} rounded-medium`}
                    >
                      <View class="relative h-8 w-14 items-center justify-center">
                        <View class={`absolute inset-0 rounded-full ${on() ? "scale-100 bg-secondary-container" : "scale-x-50 opacity-0 group-hover:scale-100 group-hover:bg-on-surface/8 group-hover:opacity-100"} ${shapeMorph}`} />
                        {icon()}
                        <If when={item().badge}>
                          <View class="absolute start-[30px] top-0.5 h-4 min-w-4 items-center justify-center rounded-full bg-error px-1">
                            <Text class="text-label-small tabular-nums text-on-error">{count(item().badge)}</Text>
                          </View>
                        </If>
                      </View>
                      <Text class={`line-clamp-1 max-w-20 text-label-medium tracking-label-medium ${on() ? "font-bold text-secondary" : "font-medium text-on-surface-variant"}`}>
                        {item().label}
                      </Text>
                    </BaseButton>
                  }
                >
                  <BaseButton
                    onPress={() => props.onSelect(item().key)}
                    selected={on()}
                    tooltip={tip(item().tooltip)}
                    testID={item().testID}
                    class={`group relative ${touch ? "h-14" : "h-12"} flex-row items-center gap-3 overflow-hidden rounded-full px-4 ${on() ? "bg-secondary-container" : ""} ${shapeMorph} ${rowFocus}`}
                  >
                    <Layer on={on() ? "on-secondary-container" : "on-surface"} />
                    <View class="w-6 items-center">{icon()}</View>
                    <Text class={`line-clamp-1 flex-1 text-label-large tracking-label-large ${on() ? "font-bold text-on-secondary-container" : "font-medium text-on-surface-variant"}`}>
                      {item().label}
                    </Text>
                    <If when={item().badge}>
                      <Text class={`text-label-large tabular-nums ${on() ? "text-on-secondary-container" : "text-on-surface-variant"}`}>{count(item().badge)}</Text>
                    </If>
                  </BaseButton>
                </If>
              </>
            );
          }}
        </Each>
      </Scroll>
      <If when={props.footer}>
        <View class={props.expanded ? "px-3 pb-3" : "items-center pb-4"}>{props.footer}</View>
      </If>
    </View>
  );
}
