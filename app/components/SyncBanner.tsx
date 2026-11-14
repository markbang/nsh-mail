// A sync that failed, over the list: what happened and a retry. It stays until a
// sync succeeds, since the mail on screen may be out of date.
import { Text, View } from "nsh";
import { t } from "nsh/i18n";
import { Icon, IconButton } from "../ui";
import { errorInfoText, mailStatus, refresh } from "../store";

export function SyncBanner() {
  return (
    <If when={mailStatus().error}>
      {(error) => (
        <View
          role="alert"
          testID="sync-banner"
          class="mx-3 mb-2 flex-row items-center gap-3 rounded-large bg-error-container py-2 pe-1 ps-4"
        >
          <Icon name="alert" size="sm" tone="on-error-container" />
          <View class="min-w-0 flex-1">
            <Text class="text-label-large font-medium text-on-error-container">{t("sync.failed")}</Text>
            <Text class="text-body-small tracking-body-small text-on-error-container">{errorInfoText(error())}</Text>
          </View>
          <IconButton icon="refresh" tone="on-error-container" label={t("actions.retry")} onPress={() => void refresh()} testID="sync-retry" />
        </View>
      )}
    </If>
  );
}
