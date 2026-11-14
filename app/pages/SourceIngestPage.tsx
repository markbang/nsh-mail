// A source's ingest URL, the one moment it is shown: copy it into the
// service's webhook field (or a script), send a test message, done.
import { Clipboard, Text, View } from "nsh";
import { t } from "nsh/i18n";
import type { Source } from "nsh-mail/native";
import { AppBar, Button, Card, Icon, IconButton, Page, SectionLabel } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { SourceMark } from "../components/SourceMark";
import { NavContext } from "../nav";
import { testSource } from "../notify";
import { showToast } from "../store";

export function SourceIngestPage(props: { source: Source; url: string; created: boolean }) {
  const nav = NavContext();
  const back = () => {
    const routes = nav.stack.routes();
    const at = routes.map((route) => route.name).lastIndexOf("sources");
    if (at >= 0) nav.stack.pop(routes.length - 1 - at);
    else nav.close();
  };
  const copy = async () => {
    try {
      await Clipboard.writeText(props.url);
      showToast(t("sources.copied"));
    } catch {
      showToast(t("errors.clipboard"), "error");
    }
  };
  return (
    <Page title={t("sources.ingest")} header={<PageHeader title={t("sources.ingest")} />} appBar={<AppBar title={t("sources.ingest")} />}>
      <PageScroll contentClass="gap-6 p-4" testID="source-ingest">
        <View class="flex-row items-center gap-3 px-1">
          <SourceMark source={props.source} />
          <View class="min-w-0 flex-1">
            <Text class="line-clamp-1 text-title-medium text-on-surface">{props.source.name}</Text>
            <Text class="text-body-medium tracking-body-medium text-on-surface-variant">{t(`source.kind.${props.source.kind}`)}</Text>
          </View>
        </View>
        <If when={props.created}>
          <View class="flex-row items-start gap-3 rounded-large bg-tertiary-container p-4">
            <Icon name="alert" tone="on-tertiary-container" />
            <Text class="flex-1 text-body-medium tracking-body-medium text-on-tertiary-container">{t("sources.onlyOnce")}</Text>
          </View>
        </If>
        <View class="gap-2">
          <SectionLabel label={t("sources.url")} class="px-1" />
          <Card class="gap-3 p-4">
            <View selectable class="rounded-medium bg-surface-container-highest p-3">
              <Text class="text-body-medium tracking-body-medium text-on-surface" testID="source-url">
                {props.url}
              </Text>
            </View>
            <View class="flex-row items-center gap-2">
              <Button variant="tonal" icon="copy" label={t("sources.copy")} onPress={() => void copy()} testID="source-copy" />
              <Button variant="text" icon="bell" label={t("sources.test")} onPress={() => void testSource(props.source)} testID="source-test" />
            </View>
            <Text class="text-body-small tracking-body-small text-on-surface-variant">{t(`source.send.${props.source.kind}`)}</Text>
          </Card>
        </View>
        <Card class="p-3">
          <Button icon="check" label={t("common.done")} onPress={() => back()} testID="source-done" />
        </Card>
      </PageScroll>
    </Page>
  );
}
