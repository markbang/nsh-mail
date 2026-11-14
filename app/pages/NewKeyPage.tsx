// A new API key: what it is called, whether its agent may mark read and
// archive, which sources it sees, and when it expires. The token comes next.
import { signal, Text, View } from "nsh";
import { t } from "nsh/i18n";
import { AppBar, Button, Card, Checkbox, Choice, Page, SectionLabel, Switch, TextField } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { NavContext } from "../nav";
import { createKey, sources } from "../notify";
import { errorText } from "../store";

const expiry = [
  { value: "30", key: "days30" },
  { value: "90", key: "days90" },
  { value: "365", key: "year" },
  { value: "", key: "never" },
];

export function NewKeyPage() {
  const nav = NavContext();
  const name = signal("");
  const write = signal(false);
  const allSources = signal(true);
  const picked = signal<Record<string, boolean>>({});
  const expires = signal("90");
  const error = signal<string | null>(null);
  const saving = signal(false);

  const save = async () => {
    if (!name().trim()) {
      error.set(t("keys.nameRequired"));
      return;
    }
    error.set(null);
    saving.set(true);
    try {
      const created = await createKey({
        name: name().trim(),
        write: write(),
        sourceIds: allSources() ? [] : Object.keys(picked()).filter((id) => picked()[id]),
        expiresDays: expires() ? Number(expires()) : null,
      });
      nav.stack.replace({ name: "keyCreated", key: created.key, token: created.token });
    } catch (e) {
      error.set(errorText(e));
    } finally {
      saving.set(false);
    }
  };

  return (
    <Page title={t("keys.new")} header={<PageHeader title={t("keys.new")} />} appBar={<AppBar title={t("keys.new")} />}>
      <PageScroll contentClass="gap-6 p-4" testID="new-key">
        <TextField value={name()} onChange={name.set} label={t("keys.name")} supporting={t("keys.nameHint")} error={error()} autoFocus testID="key-name" />
        <Card class="px-4 py-3">
          <Switch checked={write()} onChange={write.set} label={t("keys.allowWrite")} description={t("keys.allowWriteDetail")} testID="key-write" />
        </Card>
        <View class="gap-3">
          <SectionLabel label={t("keys.sources")} class="px-1" />
          <Choice
            label={t("keys.scope")}
            value={allSources() ? "all" : "picked"}
            options={[
              { value: "all", label: t("keys.allSources"), testID: "key-scope-all" },
              { value: "picked", label: t("keys.pickedSources"), testID: "key-scope-picked" },
            ]}
            onChange={(v) => allSources.set(v === "all")}
          />
          <If when={!allSources()}>
            <Card class="gap-3 p-4">
              <Each of={sources()} key={(s) => s.id}>
                {(s) => (
                  <Checkbox
                    checked={!!picked()[s().id]}
                    onChange={(on) => picked.update((p) => ({ ...p, [s().id]: on }))}
                    label={s().name}
                    testID={`key-source-${s().id}`}
                  />
                )}
              </Each>
            </Card>
          </If>
        </View>
        <View class="gap-3">
          <SectionLabel label={t("keys.expiry")} class="px-1" />
          <Choice
            label={t("keys.expiry")}
            value={expires()}
            options={expiry.map((e) => ({ value: e.value, label: t(`keys.${e.key}`), testID: `key-expiry-${e.key}` }))}
            onChange={expires.set}
          />
        </View>
        <If when={error()}>
          {(text) => (
            <Text role="alert" class="px-1 text-body-medium text-error" testID="key-error">
              {text()}
            </Text>
          )}
        </If>
        <Card class="p-3">
          <Button icon="key" label={t("keys.create")} onPress={() => void save()} loading={saving()} testID="key-create" />
        </Card>
      </PageScroll>
    </Page>
  );
}
