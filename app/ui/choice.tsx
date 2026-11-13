// M3E selection controls on nsh/base's behaviour: the switch (its handle
// grows from 16 to 24pt when on and to 28pt while pressed, on the fast spatial
// curve) and the checkbox (18pt, 2pt corners).
import { Text, View } from "nsh";
import { Checkbox as BaseCheckbox, Switch as BaseSwitch } from "nsh/base";
import { Icon } from "./icon";
import { focusRing } from "./tokens";

const spatial = "transition-all duration-350 ease-spatial-fast motion-reduce:transition-none";
const effects = "transition-colors duration-150 ease-effects-fast motion-reduce:transition-none";

/** Compose SwitchTokens: a 52×32 track, outlined off, primary on. */
export function Switch(props: { checked: boolean; onChange: (checked: boolean) => void; label: string; description?: string; disabled?: boolean; testID?: string }) {
  return (
    <BaseSwitch
      checked={props.checked}
      disabled={props.disabled}
      onChange={({ checked }) => props.onChange(checked)}
      testID={props.testID}
      class={`group flex-row-reverse items-center gap-4 rounded-medium disabled:opacity-38 ${focusRing}`}
      trackClass={`h-8 w-[52px] shrink-0 justify-center rounded-full border-2 border-outline bg-surface-container-highest px-[6px] checked:border-primary checked:bg-primary ${effects}`}
      thumbClass={`size-4 rounded-full bg-outline group-hover:bg-on-surface-variant checked:translate-x-5 checked:scale-150 checked:bg-on-primary group-hover:checked:bg-primary-container group-active:scale-[1.75] group-active:checked:scale-[1.75] ${spatial}`}
    >
      <View class="flex-1 gap-0.5">
        <Text class="text-body-large tracking-body-large text-on-surface">{props.label}</Text>
        <If when={props.description}>
          <Text class="text-body-medium tracking-body-medium text-on-surface-variant">{props.description}</Text>
        </If>
      </View>
    </BaseSwitch>
  );
}

export function Checkbox(props: { checked: boolean; onChange: (checked: boolean) => void; label: string; indeterminate?: boolean; disabled?: boolean; error?: boolean; testID?: string }) {
  return (
    <BaseCheckbox
      checked={props.checked}
      indeterminate={props.indeterminate}
      disabled={props.disabled}
      onChange={({ checked }) => props.onChange(checked)}
      testID={props.testID}
      class={`group flex-row items-center gap-3 self-start rounded-small disabled:opacity-38 ${focusRing}`}
      indicatorClass={`size-[18px] items-center justify-center rounded-[2px] border-2 ${props.error ? "border-error checked:bg-error indeterminate:bg-error" : "border-on-surface-variant checked:border-primary checked:bg-primary indeterminate:border-primary indeterminate:bg-primary"} ${effects}`}
      indicator={
        <If when={props.checked || props.indeterminate}>
          <Icon name={props.indeterminate ? "minus" : "check"} size="sm" tone={props.error ? "on-error" : "on-primary"} />
        </If>
      }
    >
      <Text class="text-body-large tracking-body-large text-on-surface">{props.label}</Text>
    </BaseCheckbox>
  );
}
