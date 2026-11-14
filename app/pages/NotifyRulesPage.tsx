// Settings → Notifications → Per source and priority: what each source's
// notifications do — a system notification, a line in the feed only, or
// nothing. The app's own switch stays the master switch.
import { Text, View } from "nsh";
import { t } from "nsh/i18n";
import { AppBar, Card, Choice, Page } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { SourceMark } from "../components/SourceMark";
import { band, rulesOf, setRule, sources, type Band, type NotifyMode } from "../notify";

const bands: Band[] = ["high", "normal", "low"];
const modes: NotifyMode[] = ["notify", "silent", "off"];

export function NotifyRulesPage() {
  return (
    <Page title={t("rules.title")} header={<PageHeader title={t("rules.title")} />} appBar={<AppBar title={t("rules.title")} />}>
      <PageScroll contentClass="gap-6 p-4" testID="notify-rules">
        <Text class="px-1 text-body-medium tracking-body-medium text-on-surface-variant">{t("rules.intro")}</Text>
        <Each of={sources()} key={(s) => s.id}>
          {(source) => (
            <View class="gap-2">
              <View class="flex-row items-center gap-3 px-1">
                <SourceMark source={source()} size="sm" />
                <Text class="flex-1 text-title-small font-medium tracking-title-small text-on-surface">{source().name}</Text>
              </View>
              <Card class="gap-3 p-4">
                <Each of={bands} key={(b) => b}>
                  {(at) => (
                    <View class="gap-1.5">
                      <Text class="text-label-large font-medium tracking-label-large text-on-surface-variant">{t(`band.${at()}`)}</Text>
                      <Choice
                        label={t("rules.label", { source: source().name, priority: t(`band.${at()}`) })}
                        value={rulesOf(source().id)[at()]}
                        options={modes.map((mode) => ({
                          value: mode,
                          label: t(`notifyMode.${mode}`),
                          testID: `rule-${source().id}-${at()}-${mode}`,
                        }))}
                        onChange={(mode) => void setRule(source().id, at(), mode)}
                        fullWidth
                      />
                    </View>
                  )}
                </Each>
              </Card>
            </View>
          )}
        </Each>
        <If when={!sources().length}>
          <Text class="px-1 text-body-medium tracking-body-medium text-on-surface-variant" testID="rules-empty">
            {t("rules.empty")}
          </Text>
        </If>
      </PageScroll>
    </Page>
  );
}
