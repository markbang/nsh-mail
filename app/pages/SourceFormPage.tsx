// A new source: its name, the colour and icon it shows in the feed, and the
// priority it pushes from. Webhooks, Gotify and ntfy then show their URL once.
import { signal, Text, View } from "nsh";
import { t } from "nsh/i18n";
import type { SourceKind } from "nsh-mail/native";
import { AppBar, Button, Card, Choice, Page, SectionLabel, SelectField, TextField, type IconName } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { kindIcon, sourceColours } from "../components/SourceMark";
import { NavContext } from "../nav";
import { createSource } from "../notify";
import { errorText, showToast } from "../store";

const icons: IconName[] = ["bolt", "bell", "bellRing", "message", "hash", "send", "activity", "terminal"];

const priorities = [
  { value: "0", key: "all" },
  { value: "3", key: "normal" },
  { value: "7", key: "high" },
];

export function SourceFormPage(props: { kind: SourceKind }) {
  const nav = NavContext();
  const name = signal("");
  const icon = signal<IconName>(kindIcon(props.kind));
  const color = signal<string>(sourceColours[0].id);
  const minPriority = signal("0");
  const error = signal<string | null>(null);
  const saving = signal(false);

  const save = async () => {
    if (!name().trim()) {
      error.set(t("sources.nameRequired"));
      return;
    }
    error.set(null);
    saving.set(true);
    try {
      const created = await createSource({
        kind: props.kind,
        name: name().trim(),
        icon: icon(),
        color: color(),
        minPriority: Number(minPriority()),
      });
      if (created.url) {
        nav.stack.replace({ name: "sourceIngest", source: created.source, url: created.url, created: true });
      } else {
        showToast(t("sources.added", { name: created.source.name }));
        nav.close();
      }
    } catch (e) {
      error.set(errorText(e));
    } finally {
      saving.set(false);
    }
  };

  return (
    <Page title={t("sources.new")} header={<PageHeader title={t("sources.new")} />} appBar={<AppBar title={t("sources.new")} />}>
      <PageScroll contentClass="gap-6 p-4" testID="source-form">
        <View class="flex-row items-center gap-3 px-1">
          <Text class="text-title-medium text-on-surface">{t(`source.kind.${props.kind}`)}</Text>
          <Text class="flex-1 text-body-medium tracking-body-medium text-on-surface-variant">{t(`source.hint.${props.kind}`)}</Text>
        </View>
        <TextField value={name()} onChange={name.set} label={t("sources.name")} supporting={t("sources.nameHint")} error={error()} autoFocus testID="source-name" />
        <View class="gap-3">
          <SectionLabel label={t("sources.color")} class="px-1" />
          <View class="flex-row flex-wrap items-center gap-3 px-1" testID="source-colors">
            <Each of={sourceColours} key={(c) => c.id}>
              {(c) => (
                <View
                  role="radio"
                  accessibilityLabel={c().id}
                  accessibilityState={{ checked: color() === c().id }}
                  onPress={() => color.set(c().id)}
                  class={`size-11 items-center justify-center rounded-full ${c().mark} ${color() === c().id ? "border-2 border-on-surface" : ""}`}
                  testID={`source-color-${c().id}`}
                >
                  <View class={`size-6 rounded-full ${c().dot}`} />
                </View>
              )}
            </Each>
          </View>
        </View>
        <View class="gap-2">
          <SectionLabel label={t("sources.icon")} class="px-1" />
          <SelectField
            label={t("sources.icon")}
            value={icon()}
            items={icons.map((i) => ({ value: i, title: i }))}
            onChange={(v) => icon.set(v as IconName)}
            testID="source-icon"
          />
        </View>
        <View class="gap-3">
          <SectionLabel label={t("sources.minPriority")} class="px-1" />
          <Text class="px-1 text-body-medium tracking-body-medium text-on-surface-variant">{t("sources.minPriorityHint")}</Text>
          <Choice
            label={t("sources.minPriority")}
            value={minPriority()}
            options={priorities.map((p) => ({ value: p.value, label: t(`priority.${p.key}`), testID: `source-priority-${p.key}` }))}
            onChange={minPriority.set}
          />
        </View>
        <If when={error()}>
          {(text) => (
            <Text role="alert" class="px-1 text-body-medium text-error" testID="source-error">
              {text()}
            </Text>
          )}
        </If>
        <Card class="p-3">
          <Button icon="check" label={t("sources.create")} onPress={() => void save()} loading={saving()} testID="source-create" />
        </Card>
      </PageScroll>
    </Page>
  );
}
