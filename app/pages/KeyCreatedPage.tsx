// The new key's token, shown once: copy it into the agent's configuration
// (REST, SSE or MCP), warning that it cannot be read again.
import { Clipboard, Text, View } from "nsh";
import { t } from "nsh/i18n";
import type { ApiKey } from "nsh-mail/native";
import { AppBar, Button, Card, Icon, Page, SectionLabel } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { keyExpiry, keyScopes } from "./ApiKeysPage";
import { NavContext } from "../nav";
import { showToast } from "../store";

export function KeyCreatedPage(props: { apiKey: ApiKey; token: string }) {
  const nav = NavContext();
  const back = () => {
    const routes = nav.stack.routes();
    const at = routes.map((route) => route.name).lastIndexOf("apiKeys");
    if (at >= 0) nav.stack.pop(routes.length - 1 - at);
    else nav.close();
  };
  const copy = async () => {
    try {
      await Clipboard.writeText(props.token);
      showToast(t("keys.copied"));
    } catch {
      showToast(t("errors.clipboard"), "error");
    }
  };
  return (
    <Page title={t("keys.created")} header={<PageHeader title={t("keys.created")} />} appBar={<AppBar title={t("keys.created")} />}>
      <PageScroll contentClass="gap-6 p-4" testID="key-created">
        <View class="flex-row items-start gap-3 rounded-large bg-tertiary-container p-4">
          <Icon name="alert" tone="on-tertiary-container" />
          <Text class="flex-1 text-body-medium tracking-body-medium text-on-tertiary-container">{t("keys.onlyOnce")}</Text>
        </View>
        <View class="gap-2">
          <SectionLabel label={props.apiKey.name} class="px-1" />
          <Card class="gap-3 p-4">
            <View selectable class="rounded-medium bg-surface-container-highest p-3">
              <Text class="text-body-medium tracking-body-medium text-on-surface" testID="key-token">
                {props.token}
              </Text>
            </View>
            <View class="flex-row items-center gap-2">
              <Button variant="tonal" icon="copy" label={t("keys.copy")} onPress={() => void copy()} testID="key-copy" />
            </View>
            <Text class="text-body-small tracking-body-small text-on-surface-variant">
              {`${keyScopes(props.apiKey)} · ${keyExpiry(props.apiKey)}`}
            </Text>
            <Text class="text-body-small tracking-body-small text-on-surface-variant">{t("keys.usage")}</Text>
          </Card>
        </View>
        <Card class="p-3">
          <Button icon="check" label={t("common.done")} onPress={() => back()} testID="key-done" />
        </Card>
      </PageScroll>
    </Page>
  );
}
