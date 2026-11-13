// A file to open, as a small pressable HeroUI card: its name and size, over a
// thumbnail when it is an image.
import { Image, Text, View } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { touch } from "./density";
import { Spinner } from "./feedback";
import { Icon } from "./icon";
import { press } from "./motion";
import { focusRing } from "./tokens";
import { shadow } from "./variants";

export interface FileTileProps {
  name: string;
  /** Its size, shown after the name. */
  detail: string;
  /** An image `src`; the tile grows a preview above the name. */
  thumbnail?: string | null;
  /** Opening: the tile shows a spinner and takes no presses. */
  busy?: boolean;
  busyLabel: string;
  onPress: () => void;
  testID?: string;
}

export function FileTile(props: FileTileProps) {
  return (
    <BaseButton
      onPress={props.onPress}
      disabled={props.busy}
      accessibilityLabel={`${props.name}, ${props.detail}`}
      testID={props.testID}
      class={`overflow-hidden rounded-large bg-surface-container-low hover:bg-surface-container-high ${shadow.sm} ${focusRing} ${press}`}
    >
      <If when={props.thumbnail}>
        {(src) => <Image src={src()} accessibilityElementsHidden class="h-32 w-56 bg-surface-container-high object-cover" />}
      </If>
      <View class={`flex-row items-center gap-2 px-3 ${touch ? "min-h-11" : "min-h-9"} ${props.thumbnail ? "w-56" : "max-w-80"}`}>
        <If when={props.busy} else={<Icon name={props.thumbnail ? "image" : "paperclip"} size="sm" tone="on-surface-variant" />}>
          <Spinner label={props.busyLabel} size="sm" />
        </If>
        <Text class={`line-clamp-1 shrink text-on-surface ${touch ? "text-body-large" : "text-body-medium"}`}>{props.name}</Text>
        <Text class="text-body-small tabular-nums text-on-surface-variant">{props.detail}</Text>
      </View>
    </BaseButton>
  );
}
