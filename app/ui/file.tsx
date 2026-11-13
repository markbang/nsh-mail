// A file to open, as a small pressable outlined card: its name and size, over a
// thumbnail when it is an image.
import { Image, Text, View } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { touch } from "./density";
import { CircularProgress } from "./progress";
import { Icon } from "./icon";
import { focusRing } from "./tokens";

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
      class={`group overflow-hidden rounded-medium border border-outline-variant bg-surface-container-low transition-all duration-350 ease-spatial-fast active:rounded-large motion-reduce:transition-none ${focusRing}`}
    >
      <View accessibilityElementsHidden class="absolute inset-0 bg-on-surface opacity-0 transition-opacity duration-150 group-hover:opacity-8 group-active:opacity-10" style={{ pointerEvents: "none" }} />
      <If when={props.thumbnail}>
        {(src) => <Image src={src()} accessibilityElementsHidden class="h-32 w-56 bg-surface-container-high object-cover" />}
      </If>
      <View class={`flex-row items-center gap-2 px-3 ${touch ? "min-h-11" : "min-h-9"} ${props.thumbnail ? "w-56" : "max-w-80"}`}>
        <If when={props.busy} else={<Icon name={props.thumbnail ? "image" : "paperclip"} size="sm" tone="on-surface-variant" />}>
          <CircularProgress label={props.busyLabel} size={18} />
        </If>
        <Text class={`line-clamp-1 shrink text-label-large font-medium text-on-surface`}>{props.name}</Text>
        <Text class="text-body-small tabular-nums text-on-surface-variant">{props.detail}</Text>
      </View>
    </BaseButton>
  );
}
