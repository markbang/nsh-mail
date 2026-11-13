// HeroUI's Tooltip for an element's `tooltip` prop: a small content1 bubble
// that springs in from 0.85. Phones show none.
import { Text, type JSX } from "nsh";
import { Tooltip as BaseTooltip } from "nsh/base";
import { touch } from "./density";
import { tooltip } from "./motion";
import { shadow } from "./variants";

/** HeroUI's Tooltip, for an element's `tooltip` prop: a small content1 bubble. */
export function Tooltip(props: { text: string; placement?: "top" | "bottom" | "left" | "right"; delay?: number }) {
  return (
    <BaseTooltip
      placement={props.placement ?? "top"}
      delay={props.delay ?? 500}
      offset={7}
      class={`max-w-xs rounded-medium bg-surface-container-low px-2.5 py-1 ${shadow.sm} ${tooltip.class}`}
      startingClass={tooltip.starting}
      endingClass={tooltip.ending}
    >
      <Text class="text-body-medium text-on-surface">{props.text}</Text>
    </BaseTooltip>
  );
}

/** The `tooltip` prop value for a hint, or none on touch screens. */
export const tip = (text: string | undefined): JSX.Element | undefined => (touch || !text ? undefined : <Tooltip text={text} />);
