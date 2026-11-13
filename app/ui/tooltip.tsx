// M3's plain tooltip for an element's `tooltip` prop: inverse-surface,
// extra-small corners, body-small. Phones show none.
import { Text, type JSX } from "nsh";
import { Tooltip as BaseTooltip } from "nsh/base";
import { touch } from "./density";
import { tooltip } from "./motion";

export function Tooltip(props: { text: string; placement?: "top" | "bottom" | "left" | "right"; delay?: number }) {
  return (
    <BaseTooltip
      placement={props.placement ?? "top"}
      delay={props.delay ?? 500}
      offset={4}
      class={`max-w-xs min-h-6 justify-center rounded-extra-small bg-inverse-surface px-2 py-1 ${tooltip.class}`}
      startingClass={tooltip.starting}
      endingClass={tooltip.ending}
    >
      <Text class="text-body-small tracking-body-small text-inverse-on-surface">{props.text}</Text>
    </BaseTooltip>
  );
}

/** The `tooltip` prop value for a hint, or none on touch screens. */
export const tip = (text: string | undefined): JSX.Element | undefined => (touch || !text ? undefined : <Tooltip text={text} />);
