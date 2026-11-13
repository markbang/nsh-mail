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
        <Text class={`text-danger ${touch ? "px-4 pb-2 text-small" : "px-3 pb-2 text-tiny"}`} testID="compose-attachment-error">
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
                  class={`relative h-7 max-w-80 flex-row items-center gap-1.5 overflow-hidden rounded-full ps-2.5 pe-1 ${a().failed ? "bg-danger/20" : "bg-default/40"}`}
                  testID="compose-attachment"
                >
                  <Icon name="paperclip" size="sm" tone={a().failed ? "danger" : "default-500"} />
                  <Text class={`line-clamp-1 shrink text-small ${a().failed ? "text-danger" : "text-foreground"}`}>{a().name}</Text>
                  <Text class="text-tiny tabular-nums text-foreground-500">
                    {a().failed
                      ? t("compose.attachFailed")
                      : staging()
                        ? t("compose.uploading", { percent: Math.round(share() * 100) })
                        : fileSize(a().size)}
                  </Text>
                  <IconButton icon="close" size="sm" radius="full" class="!size-6" label={t("compose.removeAttachment", { name: a().name })} onPress={() => props.files.remove(a().key)} />
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
