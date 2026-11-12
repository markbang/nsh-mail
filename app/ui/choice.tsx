// A switch for settings that apply at once, and a segmented choice with one
// sliding indicator.
import { Text, View } from "nsh";
import { Switch as BaseSwitch, Tab, Tabs } from "nsh/base";
import { touch } from "./density";

export function Switch(props: { checked: boolean; onChange: (checked: boolean) => void; label: string; description?: string; disabled?: boolean; testID?: string }) {
  return (
    <BaseSwitch
      checked={props.checked}
      disabled={props.disabled}
      onChange={({ checked }) => props.onChange(checked)}
      testID={props.testID}
      class={`flex-row items-center gap-3 ${touch ? "min-h-11" : "min-h-8"} disabled:opacity-50`}
      trackClass="w-9 rounded-full bg-input p-0.5 transition-colors duration-150 checked:bg-primary"
      thumbClass="size-4 rounded-full bg-background shadow-sm transition-transform duration-150 checked:translate-x-4"
    >
      <View class="flex-1 gap-0.5">
        <Text class={`text-foreground ${touch ? "text-base" : "text-sm"}`}>{props.label}</Text>
        <If when={props.description}>
          <Text class={`text-muted-foreground ${touch ? "text-sm" : "text-xs"}`}>{props.description}</Text>
        </If>
      </View>
    </BaseSwitch>
  );
}

export function Segmented<T extends string>(props: { value: T; options: { value: T; label: string }[]; onChange: (value: T) => void; class?: string }) {
  return (
    <Tabs
      class={`flex-row rounded-(--radius) bg-muted p-0.5 ${props.class ?? ""}`}
      indicatorClass="inset-y-0.5 rounded-(--radius-inset) bg-background shadow-sm transition-all duration-200 ease-in-out"
    >
      <Each of={props.options} key={(o) => o.value}>
        {(option) => (
          <Tab
            selected={props.value === option().value}
            onPress={() => props.onChange(option().value)}
            class={`group flex-1 items-center justify-center rounded-(--radius-inset) ${touch ? "min-h-10 px-3" : "h-7 px-3"} focus-visible:outline-2 focus-visible:outline-ring`}
          >
            <Text class={`${touch ? "text-sm" : "text-xs"} font-medium text-muted-foreground group-selected:text-foreground`}>{option().label}</Text>
          </Tab>
        )}
      </Each>
    </Tabs>
  );
}
