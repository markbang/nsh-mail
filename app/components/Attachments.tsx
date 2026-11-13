// The composer's attachments as chips: name, size (or how much is uploaded),
// and a button that takes the file out of the draft.
import { Text, View } from "nsh";
import { t } from "nsh/i18n";
import { Icon, IconButton, touch } from "../ui";
import { fileSize } from "../lib/format";
import type { Attachments } from "../lib/attachments";

export function AttachmentChips(props: { files: Attachments; error?: string | null }) {
  return (
    <>
      <If when={props.error}>
        <Text class={`text-destructive ${touch ? "px-4 pb-2 text-sm" : "px-3 pb-2 text-xs"}`} testID="compose-attachment-error">
          {props.error}
        </Text>
      </If>
      <If when={props.files.items().length}>
        <View class={`flex-row flex-wrap gap-2 ${touch ? "px-4 pb-3" : "px-3 pb-2"}`} testID="compose-attachments">
          <Each of={props.files.items()} key={(a) => a.key}>
            {(a) => {
              const staging = () => !a().ref && !a().failed;
              const share = () => (a().size ? Math.min(1, a().sent / a().size) : 1);
              return (
                <View
                  class={`relative max-w-80 flex-row items-center gap-2 overflow-hidden rounded-(--radius) border bg-background py-1 pl-3 pr-1 ${a().failed ? "border-destructive" : "border-border"}`}
                  testID="compose-attachment"
                >
                  <Icon name="paperclip" size="sm" tone={a().failed ? "destructive" : "muted-foreground"} />
                  <Text class={`line-clamp-1 shrink text-foreground ${touch ? "text-base" : "text-sm"}`}>{a().name}</Text>
                  <Text class="text-xs tabular-nums text-muted-foreground">
                    {a().failed
                      ? t("compose.attachFailed")
                      : staging()
                        ? t("compose.uploading", { percent: Math.round(share() * 100) })
                        : fileSize(a().size)}
                  </Text>
                  <IconButton icon="close" size="sm" label={t("compose.removeAttachment", { name: a().name })} onPress={() => props.files.remove(a().key)} />
                  <If when={staging()}>
                    <View class="absolute bottom-0 left-0 h-0.5 bg-primary" style={{ width: `${share() * 100}%` }} />
                  </If>
                </View>
              );
            }}
          </Each>
        </View>
      </If>
    </>
  );
}
