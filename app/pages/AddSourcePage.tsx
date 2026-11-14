// Settings → Sources → Add: what kind of source. Mail accounts come from Add
// account; the chat bots arrive with the server's bot connections.
import { Text, View } from "nsh";
import { t } from "nsh/i18n";
import type { SourceKind } from "nsh-mail/native";
import { AppBar, Card, Icon, ListItem, Page } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { kindIcon } from "../components/SourceMark";
import { NavContext } from "../nav";

/** The kinds a person can add now; the chats need the server's bot connections (M2). */
const kinds: { kind: SourceKind; enabled: boolean }[] = [
  { kind: "webhook", enabled: true },
  { kind: "gotify", enabled: true },
  { kind: "ntfy", enabled: true },
  { kind: "telegram", enabled: false },
  { kind: "discord", enabled: false },
  { kind: "slack", enabled: false },
  { kind: "feishu", enabled: false },
  { kind: "dingtalk", enabled: false },
  { kind: "wecom", enabled: false },
];

export function AddSourcePage() {
  const nav = NavContext();
  return (
    <Page
      title={t("sources.add")}
      header={<PageHeader title={t("sources.add")} />}
      appBar={<AppBar title={t("sources.add")} />}
    >
      <PageScroll contentClass="gap-4 p-4" testID="add-source">
        <Text class="px-1 text-body-medium tracking-body-medium text-on-surface-variant">{t("sources.chooseKind")}</Text>
        <Card>
          <Each of={kinds} key={(k) => k.kind}>
            {(entry, index) => (
              <If
                when={entry().enabled}
                else={
                  <View class="min-h-16 flex-row items-center gap-4 px-4 py-2 opacity-38">
                    <Icon name={kindIcon(entry().kind)} tone="on-surface-variant" />
                    <View class="flex-1">
                      <Text class="text-body-large tracking-body-large text-on-surface">{t(`source.kind.${entry().kind}`)}</Text>
                      <Text class="text-body-medium tracking-body-medium text-on-surface-variant">{t("sources.comingSoon")}</Text>
                    </View>
                  </View>
                }
              >
                <>
                  <If when={index() > 0}>
                    <View class="mx-4 h-px bg-outline-variant" />
                  </If>
                  <ListItem
                    icon={kindIcon(entry().kind)}
                    headline={t(`source.kind.${entry().kind}`)}
                    supporting={t(`source.hint.${entry().kind}`)}
                    onPress={() => nav.stack.push({ name: "sourceForm", kind: entry().kind })}
                    trailing={<Icon name="chevronRight" tone="on-surface-variant" />}
                    testID={`kind-${entry().kind}`}
                  />
                </>
              </If>
            )}
          </Each>
        </Card>
      </PageScroll>
    </Page>
  );
}
