// Settings → API keys: the keys agents read and write the feed with. A key's
// token is shown once, when it is created; revoking stops its agent at once.
import { signal, Text, View } from "nsh";
import { AlertDialog } from "nsh/base";
import { t } from "nsh/i18n";
import type { ApiKey } from "nsh-mail/native";
import { AppBar, Button, Card, Divider, IconButton, ListItem, Menu, Page, SectionLabel } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { fullDate } from "../lib/format";
import { NavContext } from "../nav";
import { apiKeys, apiKeysState, keySources, loadKeys, revokeKey } from "../notify";

/** What a key may do, in a line. */
export function keyScopes(key: ApiKey) {
  return key.write ? t("keys.scopeWrite") : t("keys.scopeRead");
}

export function keyExpiry(key: ApiKey) {
  return key.expiresAt ? t("keys.expires", { date: fullDate(key.expiresAt) }) : t("keys.noExpiry");
}

export function ApiKeysPage() {
  const nav = NavContext();
  const busy = signal<string | null>(null);
  // The list loads when the page first shows, once per visit.
  const asked = signal(false);
  if (!asked()) {
    asked.set(true);
    void loadKeys();
  }
  const revoke = async (key: ApiKey) => {
    const { action } = await AlertDialog.show({
      title: t("keys.revokeTitle", { name: key.name }),
      description: t("keys.revokeBody"),
      actions: [
        { id: "cancel", label: t("common.cancel"), role: "cancel" },
        { id: "revoke", label: t("keys.revoke"), role: "destructive" },
      ],
    });
    if (action !== "revoke") return;
    busy.set(key.id);
    try {
      await revokeKey(key);
    } catch {
      // `revokeKey` said why.
    } finally {
      busy.set(null);
    }
  };
  const lastUsed = (key: ApiKey) => (key.lastUsedAt ? t("keys.lastUsed", { date: fullDate(key.lastUsedAt) }) : t("keys.neverUsed"));
  return (
    <Page
      title={t("keys.title")}
      header={<PageHeader title={t("keys.title")} actions={<IconButton icon="plus" label={t("keys.create")} onPress={() => nav.stack.push({ name: "newKey" })} testID="keys-add-top" />} />}
      appBar={<AppBar title={t("keys.title")} actions={<IconButton icon="plus" label={t("keys.create")} onPress={() => nav.stack.push({ name: "newKey" })} testID="keys-add-top" />} />}
    >
      <PageScroll contentClass="gap-6 p-4" testID="api-keys">
        <Text class="px-1 text-body-medium tracking-body-medium text-on-surface-variant">{t("keys.intro")}</Text>
        <View class="gap-1">
          <SectionLabel label={t("keys.list")} class="px-1" />
          <If
            when={apiKeys().length}
            else={
              <Card class="items-start gap-2 p-4">
                <Text class="text-body-large tracking-body-large text-on-surface">{t("keys.empty")}</Text>
                <Text class="text-body-medium tracking-body-medium text-on-surface-variant">{t("keys.emptyHint")}</Text>
                <Button icon="plus" label={t("keys.create")} onPress={() => nav.stack.push({ name: "newKey" })} class="mt-2" testID="keys-add-empty" />
              </Card>
            }
          >
            <Card>
              <Each of={apiKeys()} key={(k) => k.id}>
                {(key, index) => (
                  <>
                    <If when={index() > 0}>
                      <Divider inset />
                    </If>
                    <View class="min-h-[72px] flex-row items-center gap-3 py-2 pe-2 ps-4">
                      <View class="min-w-0 flex-1 gap-0.5">
                        <Text class="line-clamp-1 text-body-large tracking-body-large text-on-surface" testID={`key-${key().id}`}>
                          {key().name}
                        </Text>
                        <Text class="line-clamp-1 text-body-medium tracking-body-medium text-on-surface-variant">
                          {`${key().prefix}… · ${keyScopes(key())} · ${keySources(key())}`}
                        </Text>
                        <Text class="line-clamp-1 text-body-small tracking-body-small text-on-surface-variant">
                          {`${lastUsed(key())} · ${keyExpiry(key())}`}
                        </Text>
                      </View>
                      <Menu
                        label={t("keys.actions", { name: key().name })}
                        trigger={<IconButton icon="more" label={t("keys.actions", { name: key().name })} testID={`key-more-${index()}`} />}
                        actions={[{ id: "revoke", label: t("keys.revoke"), icon: "trash", destructive: true, onSelect: () => void revoke(key()) }]}
                      />
                    </View>
                  </>
                )}
              </Each>
              <Divider inset />
              <View class="p-3">
                <Button variant="text" icon="plus" label={t("keys.create")} onPress={() => nav.stack.push({ name: "newKey" })} class="self-start" testID="keys-add" />
              </View>
            </Card>
          </If>
        </View>
        <If when={apiKeysState() === "error"}>
          <ListItem icon="alert" headline={t("keys.loadFailed")} supporting={t("keys.loadFailedHint")} onPress={() => void loadKeys()} testID="keys-retry" />
        </If>
      </PageScroll>
    </Page>
  );
}
