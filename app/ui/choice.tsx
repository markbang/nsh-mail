// HeroUI's Switch, Checkbox and Tabs on nsh/base's behaviour.
import { Text, View } from "nsh";
import { Checkbox as BaseCheckbox, Switch as BaseSwitch, Tab, Tabs as BaseTabs } from "nsh/base";
import { Icon } from "./icon";
import { focusRing, shadow, type Color, type Size } from "./tokens";

const track: Record<Size, string> = { sm: "w-10 h-6", md: "w-12 h-7", lg: "w-14 h-8" };
const thumb: Record<Size, string> = {
  sm: "size-4 checked:translate-x-4",
  md: "size-5 checked:translate-x-5",
  lg: "size-6 checked:translate-x-6",
};
const labelText: Record<Size, string> = { sm: "text-small", md: "text-medium", lg: "text-large" };

/** HeroUI's Switch: a default-200 track that fills with the color, a white thumb with a small shadow. */
export function Switch(props: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: string;
  disabled?: boolean;
  color?: Color;
  size?: Size;
  testID?: string;
}) {
  const size = () => props.size ?? "md";
  const c = () => props.color ?? "primary";
  return (
    <BaseSwitch
      checked={props.checked}
      disabled={props.disabled}
      onChange={({ checked }) => props.onChange(checked)}
      testID={props.testID}
      class={`group flex-row items-center gap-3 disabled:opacity-50 ${focusRing} rounded-medium`}
      trackClass={`${track[size()]} shrink-0 justify-center rounded-full bg-default-200 px-1 transition-colors duration-250 checked:bg-${c()}`}
      thumbClass={`${thumb[size()]} rounded-full bg-white ${shadow.sm} transition duration-250 ease-css motion-reduce:transition-none`}
    >
      <View class="flex-1 gap-0.5">
        <Text class={`text-foreground ${props.description ? "text-small" : labelText[size()]}`}>{props.label}</Text>
        <If when={props.description}>
          <Text class="text-tiny text-foreground-500">{props.description}</Text>
        </If>
      </View>
    </BaseSwitch>
  );
}

const box: Record<Size, string> = { sm: "size-4 rounded-[4px]", md: "size-5 rounded-[7px]", lg: "size-6 rounded-[9px]" };

/** HeroUI's Checkbox: a 2px default border that fills with the color and shows a check. */
export function Checkbox(props: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  indeterminate?: boolean;
  disabled?: boolean;
  color?: Color;
  size?: Size;
  testID?: string;
}) {
  const size = () => props.size ?? "md";
  const c = () => props.color ?? "primary";
  return (
    <BaseCheckbox
      checked={props.checked}
      indeterminate={props.indeterminate}
      disabled={props.disabled}
      onChange={({ checked }) => props.onChange(checked)}
      testID={props.testID}
      class={`group flex-row items-center gap-2 self-start rounded-small disabled:opacity-50 ${focusRing}`}
      indicatorClass={`${box[size()]} items-center justify-center border-2 border-default transition-colors duration-250 group-hover:bg-default-100 checked:border-${c()} checked:bg-${c()} indeterminate:border-${c()} indeterminate:bg-${c()}`}
      indicator={
        <If when={props.checked || props.indeterminate}>
          <Icon name={props.indeterminate ? "minus" : "check"} size="sm" tone={`${c()}-foreground`} />
        </If>
      }
    >
      <Text class={`text-foreground ${labelText[size()]}`}>{props.label}</Text>
    </BaseCheckbox>
  );
}

const tabHeight: Record<Size, string> = { sm: "h-7 text-tiny", md: "h-8 text-small", lg: "h-9 text-medium" };

/** HeroUI's Tabs (solid): a default-100 list whose cursor, a background pill with a small shadow, slides to the selected tab. */
export function Tabs<T extends string>(props: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  size?: Size;
  variant?: "solid" | "light" | "underlined" | "bordered";
  fullWidth?: boolean;
  class?: string;
}) {
  const size = () => props.size ?? "md";
  const variant = () => props.variant ?? "solid";
  const list = () =>
    ({
      solid: "bg-default-100",
      light: "",
      underlined: "",
      bordered: `border-2 border-default-200 ${shadow.sm}`,
    })[variant()];
  const cursor = () =>
    variant() === "underlined"
      ? "bottom-0 h-[2px] bg-foreground"
      : `inset-y-1 rounded-small bg-background dark:bg-default ${shadow.sm}`;
  return (
    <BaseTabs
      class={`flex-row gap-2 p-1 ${size() === "lg" ? "rounded-large" : "rounded-medium"} ${list()} ${props.fullWidth ? "self-stretch" : "self-start"} ${props.class ?? ""}`}
      indicatorClass={`${cursor()} transition-all duration-250 ease-out motion-reduce:transition-none`}
    >
      <Each of={props.options} key={(o) => o.value}>
        {(option) => (
          <Tab
            selected={props.value === option().value}
            onPress={() => props.onChange(option().value)}
            class={`group ${props.fullWidth ? "flex-1" : ""} items-center justify-center rounded-small px-3 ${tabHeight[size()]} transition-opacity ${props.value === option().value ? "" : "hover:opacity-50"} ${focusRing}`}
          >
            <Text class="text-default-500 transition-colors group-selected:text-foreground">{option().label}</Text>
          </Tab>
        )}
      </Each>
    </BaseTabs>
  );
}

/** Earlier name: full-width solid Tabs. */
export function Segmented<T extends string>(props: { value: T; options: { value: T; label: string }[]; onChange: (value: T) => void; class?: string }) {
  return <Tabs value={props.value} options={props.options} onChange={props.onChange} fullWidth class={props.class} />;
}
