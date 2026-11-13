// M3 cards: filled (surface-container-highest), outlined (outline-variant) or
// elevated (surface-container-low, level 1), medium corners. A pressable card
// takes the state layer and rounds to large while pressed.
import { View, type JSX } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { focusRing } from "./tokens";

export interface CardProps {
  children?: JSX.Element;
  variant?: "filled" | "outlined" | "elevated";
  onPress?: () => void;
  accessibilityLabel?: string;
  class?: string;
  testID?: string;
}

const look = {
  filled: "bg-surface-container-highest",
  outlined: "border border-outline-variant bg-surface",
  elevated: "bg-surface-container-low shadow-elevation-1",
} as const;

export function Card(props: CardProps) {
  const frame = () => `overflow-hidden rounded-medium text-on-surface ${look[props.variant ?? "filled"]} ${props.class ?? ""}`;
  return (
    <If when={props.onPress} else={<View class={frame()} testID={props.testID} accessibilityLabel={props.accessibilityLabel}>{props.children}</View>}>
      <BaseButton
        onPress={() => props.onPress?.()}
        accessibilityLabel={props.accessibilityLabel}
        testID={props.testID}
        class={`group ${frame()} transition-all duration-350 ease-spatial-fast active:rounded-large motion-reduce:transition-none ${focusRing}`}
      >
        <View accessibilityElementsHidden class="absolute inset-0 bg-on-surface opacity-0 transition-opacity duration-150 group-hover:opacity-8 group-active:opacity-10" style={{ pointerEvents: "none" }} />
        {props.children}
      </BaseButton>
    </If>
  );
}
