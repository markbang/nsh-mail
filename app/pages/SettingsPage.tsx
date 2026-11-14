// Settings: the accounts, where mail comes from, reading, sending, notifications, signing out.
import { appInfo, keymap, Notifications, platform, Text, View } from "nsh";
import { AlertDialog } from "nsh/base";
import { t } from "nsh/i18n";
import { Mail, type Account } from "nsh-mail/native";
import { AccountMark, AppBar, Button, Card, Choice, Divider, Icon, IconButton, ListItem, Menu, Page, SectionLabel, Switch, systemHeader, touch } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { NavContext } from "../nav";
import { providerName as named } from "../lib/format";
import { shortcutsOpen } from "../lib/shortcuts";
import { setUndoDelay, UNDO_DELAYS, undoDelay } from "../outbox";
import {
  accounts,
  conversationsEnabled,
  errorText,
  mailStatus,
  notificationsEnabled,
  sessionChanged,
  setConversationsEnabled,
  setNotificationsEnabled,
  showToast,
} from "../store";

/** "QQ Mail", "IMAP": who runs the account, as people know it. */
const providerName = (a: Account) => named(a.provider, t("provider.imap"));

export function SettingsPage() {
  const nav = NavContext();

  const remove = async (a: Account) => {
    const { action } = await AlertDialog.show({
      title: t("accounts.removeTitle", { email: a.email }),
      description: t("accounts.removeBody"),
      actions: [
        { id: "cancel", label: t("common.cancel"), role: "cancel" },
        { id: "remove", label: t("accounts.remove"), role: "destructive" },
      ],
    });
    if (action !== "remove") return;
    try {
      await Mail.removeAccount(a.id);
      showToast(t("accounts.removed", { email: a.email }));
    } catch (e) {
      showToast(errorText(e), "error");
    }
  };

  const signOut = async () => {
    const { action } = await AlertDialog.show({
      title: t("settings.signOutTitle"),
      description: mailStatus().mode === "local" ? t("settings.signOutLocal") : t("settings.signOutCloud"),
      actions: [
        { id: "cancel", label: t("common.cancel"), role: "cancel" },
        { id: "out", label: t("settings.signOut"), role: "destructive" },
      ],
    });
    if (action !== "out") return;
    try {
      await Mail.signOut();
      nav.done();
      await sessionChanged();
    } catch (e) {
      showToast(errorText(e), "error");
    }
  };

  const toggleNotifications = async (on: boolean) => {
    if (on) {
      const permission = await Notifications.requestPermission().catch(() => "unavailable" as const);
      if (permission === "denied") showToast(t("settings.notificationsDenied"), "error");
    }
    await setNotificationsEnabled(on);
  };

  return (
    <Page title={t("settings.title")} header={<PageHeader title={t("settings.title")} />} root={systemHeader} appBar={<AppBar title={t("settings.title")} />}>
      <PageScroll contentClass={touch ? "gap-6 p-4" : "gap-6 p-5"} testID="settings">
        <View class="gap-1">
          <SectionLabel label={t("settings.accounts")} class="px-1" />
          <Card>
            <Each of={accounts()} key={(a) => a.id}>
              {(a, index) => (
                <>
                  <If when={index() > 0}>
                    <Divider inset />
                  </If>
                  <View class="min-h-[72px] flex-row items-center gap-4 py-2 pe-2 ps-4">
                    <AccountMark index={index()} size="md" />
                    <View class="min-w-0 flex-1">
                      <Text class="line-clamp-1 text-body-large tracking-body-large text-on-surface">{a().email}</Text>
                      <Text class="line-clamp-1 text-body-medium tracking-body-medium text-on-surface-variant">
                        {a().name ? `${a().name} · ${providerName(a())}` : providerName(a())}
                      </Text>
                    </View>
                    <Menu
                      label={t("accounts.actions", { email: a().email })}
                      trigger={<IconButton icon="more" label={t("accounts.actions", { email: a().email })} testID={`account-more-${index()}`} />}
                      actions={[
                        { id: "signature", label: t("signature.menu"), icon: "compose", onSelect: () => nav.stack.push({ name: "signature", account: a().id }) },
                        { id: "remove", label: t("accounts.removeMenu"), icon: "trash", destructive: true, onSelect: () => void remove(a()) },
                      ]}
                    />
                  </View>
                </>
              )}
            </Each>
            <If when={accounts().length}>
              <Divider inset />
            </If>
            <View class="p-3">
              <Button variant="text" icon="plus" label={t("accounts.add")} onPress={() => nav.stack.push({ name: "addAccount" })} class="self-start" testID="add-account" />
            </View>
          </Card>
        </View>

        <View class="gap-1">
          <SectionLabel label={t("settings.sync")} class="px-1" />
          <Card class="gap-4 p-4">
            <View class="flex-row items-center gap-3">
              <Icon name={mailStatus().mode === "cloud" ? "cloud" : "device"} tone="on-surface-variant" />
              <View class="flex-1 gap-0.5">
                <Text class="text-body-large tracking-body-large text-on-surface">{t(`mode.${mailStatus().mode}`)}</Text>
                <If when={mailStatus().mode === "cloud" && mailStatus().user}>
                  {(user) => (
                    <Text class="text-body-medium tracking-body-medium text-on-surface" testID="signed-in-as">
                      {t("settings.signedInAs", { email: user() })}
                    </Text>
                  )}
                </If>
                <Text class="text-body-medium tracking-body-medium text-on-surface-variant">
                  {mailStatus().mode === "cloud" ? (mailStatus().server ?? "") : t("settings.localDetail")}
                </Text>
              </View>
            </View>
            <If when={mailStatus().mode === "local" && platform.os === "android"}>
              <Text class="text-body-medium tracking-body-medium text-on-surface-variant">{t("settings.androidLocal")}</Text>
            </If>
            <Button variant="outlined" tone="error" icon="logOut" label={t("settings.signOut")} onPress={() => void signOut()} class="self-start" testID="sign-out" />
          </Card>
        </View>

        <View class="gap-1">
          <SectionLabel label={t("settings.reading")} class="px-1" />
          <Card class="px-4 py-3">
            <Switch
              checked={conversationsEnabled()}
              onChange={(on) => void setConversationsEnabled(on)}
              label={t("settings.conversations")}
              description={t("settings.conversationsDetail")}
              testID="conversations"
            />
          </Card>
        </View>

        <View class="gap-1">
          <SectionLabel label={t("settings.sending")} class="px-1" />
          <Card class="gap-3 p-4">
            <View class="gap-0.5">
              <Text class="text-body-large tracking-body-large text-on-surface">{t("settings.undoSend")}</Text>
              <Text class="text-body-medium tracking-body-medium text-on-surface-variant">{t("settings.undoSendDetail")}</Text>
            </View>
            <Choice
              label={t("settings.undoSend")}
              value={String(undoDelay())}
              options={UNDO_DELAYS.map((s) => ({ value: String(s), label: s ? t("settings.seconds", { n: s }) : t("settings.undoOff"), testID: `undo-${s}` }))}
              onChange={(v) => void setUndoDelay(Number(v))}
              fullWidth={touch}
            />
          </Card>
        </View>

        <View class="gap-1">
          <SectionLabel label={t("settings.notifications")} class="px-1" />
          <Card class="px-4 py-3">
            <Switch
              checked={notificationsEnabled()}
              onChange={(on) => void toggleNotifications(on)}
              label={t("settings.newMailNotifications")}
              description={t("settings.notificationsDetail")}
              testID="notifications"
            />
          </Card>
        </View>

        <View class="gap-1">
          <SectionLabel label={t("settings.help")} class="px-1" />
          <Card>
            <ListItem
              icon="keyboard"
              headline={t("shortcuts.title")}
              supporting={t("shortcuts.summary")}
              onPress={() => shortcutsOpen.set(true)}
              trailing={<Text class="text-label-large text-on-surface-variant">{keymap.label("shift+/")}</Text>}
              testID="open-shortcuts"
            />
          </Card>
        </View>

        <Text class="text-center text-body-small tracking-body-small text-on-surface-variant">
          {t("settings.about", { version: appInfo().version ?? "" })}
        </Text>
      </PageScroll>
    </Page>
  );
}
