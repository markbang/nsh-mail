// HeroUI's Card: a content1 surface with a large radius and a medium shadow,
// header, body and footer padded 12pt; pressable cards sink like buttons.
import { View, type JSX } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { press } from "./motion";
import { focusRing, rounded, shadow, type Radius } from "./tokens";

export interface CardProps {
  children?: JSX.Element;
  shadow?: "none" | "sm" | "md" | "lg";
  radius?: Radius;
  /** A hairline instead of a shadow, for cards inside panes. */
  bordered?: boolean;
  onPress?: () => void;
  accessibilityLabel?: string;
  class?: string;
  testID?: string;
}

export function Card(props: CardProps) {
  const frame = () =>
    `overflow-hidden bg-content1 text-foreground ${rounded[props.radius ?? "lg"]} ${props.bordered ? "border border-divider" : shadow[props.shadow ?? "md"]} ${props.class ?? ""}`;
  return (
    <If when={props.onPress} else={<View class={frame()} testID={props.testID} accessibilityLabel={props.accessibilityLabel}>{props.children}</View>}>
      <BaseButton
        onPress={() => props.onPress?.()}
        accessibilityLabel={props.accessibilityLabel}
        testID={props.testID}
        class={`${frame()} ${press} ${focusRing} hover:bg-content2`}
      >
        {props.children}
      </BaseButton>
    </If>
  );
}

export function CardHeader(props: { children?: JSX.Element; class?: string }) {
  return <View class={`flex-row items-center gap-3 p-3 ${props.class ?? ""}`}>{props.children}</View>;
}

export function CardBody(props: { children?: JSX.Element; class?: string }) {
  return <View class={`gap-3 p-3 ${props.class ?? ""}`}>{props.children}</View>;
}

export function CardFooter(props: { children?: JSX.Element; class?: string }) {
  return <View class={`flex-row items-center gap-2 p-3 ${props.class ?? ""}`}>{props.children}</View>;
}
