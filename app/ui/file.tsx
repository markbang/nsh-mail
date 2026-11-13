// A file to open: its name and size, over a thumbnail when it is an image.
import { Image, Text, View } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { touch } from "./density";
import { Spinner } from "./feedback";
import { Icon } from "./icon";
import { press } from "./motion";

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
      class={`overflow-hidden rounded-(--radius) border border-border bg-background hover:bg-hover active:bg-pressed focus-visible:outline-2 focus-visible:outline-ring focus-visible:outline-offset-2 ${press}`}
    >
      <If when={props.thumbnail}>
        {(src) => <Image src={src()} accessibilityElementsHidden class="h-32 w-56 bg-muted object-cover" />}
      </If>
      <View class={`flex-row items-center gap-2 px-3 ${touch ? "min-h-11" : "min-h-9"} ${props.thumbnail ? "w-56" : "max-w-80"}`}>
        <If when={props.busy} else={<Icon name={props.thumbnail ? "image" : "paperclip"} size="sm" tone="muted-foreground" />}>
          <Spinner label={props.busyLabel} />
        </If>
        <Text class={`line-clamp-1 shrink text-foreground ${touch ? "text-base" : "text-sm"}`}>{props.name}</Text>
        <Text class="text-xs tabular-nums text-muted-foreground">{props.detail}</Text>
      </View>
    </BaseButton>
  );
}
