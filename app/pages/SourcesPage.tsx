// Settings → Sources: where notifications come from, one row each. Pausing
// keeps a source's history and stops new notifications; the menu sends a test
// message, shows the ingest URL again (a new one) and deletes the source.
import { Text, View } from "nsh";
import { AlertDialog } from "nsh/base";
import { t } from "nsh/i18n";
import type { Source } from "nsh-mail/native";
import { AppBar, Button, Card, Divider, Icon, IconButton, Menu, Page, SectionLabel } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { SourceMark } from "../components/SourceMark";
import { NavContext } from "../nav";
import { deleteSource, rotateSource, setSourceEnabled, sources, testSource } from "../notify";

export function SourcesPage() {
  const nav = NavContext();
  const remove = async (source: Source) => {
    const { action } = await AlertDialog.show({
      title: t("sources.deleteTitle", { name: source.name }),
      description: t("sources.deleteBody"),
      actions: [
        { id: "cancel", label: t("common.cancel"), role: "cancel" },
        { id: "delete", label: t("sources.delete"), role: "destructive" },
      ],
    });
    if (action !== "delete") return;
    try {
      await deleteSource(source);
    } catch {
      // `deleteSource` said why.
    }
  };
  const showUrl = async (source: Source) => {
    try {
      const rotated = await rotateSource(source);
      if (rotated.ingestUrl) nav.stack.push({ name: "sourceIngest", source: rotated.source, url: rotated.ingestUrl, created: false });
    } catch (e) {
      void e;
    }
  };
  const row = (source: Source, index: number) => (
    <View class="min-h-[72px] flex-row items-center gap-3 py-2 pe-2 ps-4">
      <SourceMark source={source} />
      <View class="min-w-0 flex-1">
        <Text class="line-clamp-1 text-body-large tracking-body-large text-on-surface" testID={`source-${source.id}`}>
          {source.name}
        </Text>
        <Text class="line-clamp-1 text-body-medium tracking-body-medium text-on-surface-variant">
          {t(`source.kind.${source.kind}`)}
          {source.enabled ? "" : ` · ${t("sources.paused")}`}
          {source.minPriority > 0 ? ` · ${t("sources.fromPriority", { priority: t(`priority.${source.minPriority >= 7 ? "high" : "normal"}`) })}` : ""}
        </Text>
      </View>
      <Menu
        label={t("sources.actions", { name: source.name })}
        trigger={<IconButton icon="more" label={t("sources.actions", { name: source.name })} testID={`source-more-${index}`} />}
        actions={[
          { id: "test", label: t("sources.test"), icon: "bell", onSelect: () => void testSource(source) },
          {
            id: "toggle",
            label: source.enabled ? t("sources.pause") : t("sources.resume"),
            icon: source.enabled ? "minus" : "check",
            onSelect: () => void setSourceEnabled(source, !source.enabled),
          },
          ...(source.kind === "mail" ? [] : [{ id: "url", label: t("sources.showUrl"), icon: "key" as const, onSelect: () => void showUrl(source) }]),
          ...(source.kind === "mail" ? [] : [{ id: "delete", label: t("sources.delete"), icon: "trash" as const, destructive: true, onSelect: () => void remove(source) }]),
        ]}
      />
    </View>
  );
  return (
    <Page
      title={t("sources.title")}
      header={<PageHeader title={t("sources.title")} actions={<IconButton icon="plus" label={t("sources.add")} onPress={() => nav.stack.push({ name: "addSource" })} testID="sources-add-top" />} />}
      appBar={<AppBar title={t("sources.title")} actions={<IconButton icon="plus" label={t("sources.add")} onPress={() => nav.stack.push({ name: "addSource" })} testID="sources-add-top" />} />}
    >
      <PageScroll contentClass="gap-6 p-4" testID="sources">
        <View class="flex-row items-start gap-3 rounded-large bg-surface-container-low p-4">
          <Icon name="bell" tone="on-surface-variant" />
          <Text class="flex-1 text-body-medium tracking-body-medium text-on-surface-variant">{t("sources.intro")}</Text>
        </View>
        <View class="gap-1">
          <SectionLabel label={t("sources.list")} class="px-1" />
          <If
            when={sources().length}
            else={
              <Card class="items-start gap-2 p-4">
                <Text class="text-body-large tracking-body-large text-on-surface">{t("sources.empty")}</Text>
                <Text class="text-body-medium tracking-body-medium text-on-surface-variant">{t("sources.emptyHint")}</Text>
                <Button icon="plus" label={t("sources.add")} onPress={() => nav.stack.push({ name: "addSource" })} class="mt-2" testID="sources-add-empty" />
              </Card>
            }
          >
            <Card>
              <Each of={sources()} key={(s) => s.id}>
                {(source, index) => (
                  <>
                    <If when={index() > 0}>
                      <Divider inset />
                    </If>
                    {row(source(), index())}
                  </>
                )}
              </Each>
              <Divider inset />
              <View class="p-3">
                <Button variant="text" icon="plus" label={t("sources.add")} onPress={() => nav.stack.push({ name: "addSource" })} class="self-start" testID="sources-add" />
              </View>
            </Card>
          </If>
        </View>
      </PageScroll>
    </Page>
  );
}
