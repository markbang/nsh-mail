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
        <Text class={`text-error ${touch ? "px-4 pb-2 text-body-medium" : "px-3 pb-2 text-body-small"}`} testID="compose-attachment-error">
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
                  class={`relative h-8 max-w-80 flex-row items-center gap-2 overflow-hidden rounded-small ps-2 pe-1 ${a().failed ? "bg-error-container" : "border border-outline-variant"}`}
                  testID="compose-attachment"
                >
                  <Icon name="paperclip" size="sm" tone={a().failed ? "error" : "on-surface-variant"} />
                  <Text class={`line-clamp-1 shrink text-label-large font-medium ${a().failed ? "text-on-error-container" : "text-on-surface"}`}>{a().name}</Text>
                  <Text class="text-label-medium tabular-nums text-on-surface-variant">
                    {a().failed
                      ? t("compose.attachFailed")
                      : staging()
                        ? t("compose.uploading", { percent: Math.round(share() * 100) })
                        : fileSize(a().size)}
                  </Text>
                  <IconButton icon="close" size="xs" width="narrow" class="!h-6 !w-6" label={t("compose.removeAttachment", { name: a().name })} onPress={() => props.files.remove(a().key)} />
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
