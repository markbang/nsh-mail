// Settings: the accounts, where mail comes from, notifications, signing out.
import { Notifications, platform, Text, View } from "nsh";
import { AlertDialog, NavPage } from "nsh/base";
import { t } from "nsh/i18n";
import { Mail, type Account } from "nsh-mail/native";
import { AccountMark, Button, Card, Divider, Icon, IconButton, Menu, SectionLabel, Switch, touch } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { NavContext } from "../nav";
import { providerName as named } from "../lib/format";
import { accounts, errorText, mailStatus, notificationsEnabled, sessionChanged, setNotificationsEnabled, showToast } from "../store";

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

  const text = touch ? "text-medium" : "text-small";
  const note = touch ? "text-small" : "text-tiny";
  return (
    <NavPage title={t("settings.title")} header={<PageHeader title={t("settings.title")} />}>
      <PageScroll contentClass={touch ? "gap-6 p-4" : "gap-6 p-5"} testID="settings">
        <View class="gap-1">
          <SectionLabel label={t("settings.accounts")} class="px-0" />
          <Card shadow="sm">
            <Each of={accounts()} key={(a) => a.id}>
              {(a, index) => (
                <>
                  <If when={index() > 0}>
                    <Divider />
                  </If>
                  <View class={`flex-row items-center gap-3 ps-3 pe-1 ${touch ? "min-h-14 py-2" : "min-h-12 py-1.5"}`}>
                    <AccountMark index={index()} size="md" />
                    <View class="min-w-0 flex-1">
                      <Text class={`line-clamp-1 text-foreground ${text}`}>
                        {a().email}
                      </Text>
                      <Text class={`line-clamp-1 text-foreground-500 ${note}`}>
                        {a().name ? `${a().name} · ${providerName(a())}` : providerName(a())}
                      </Text>
                    </View>
                    <Menu
                      label={t("accounts.actions", { email: a().email })}
                      trigger={<IconButton icon="more" label={t("accounts.actions", { email: a().email })} size="sm" />}
                      actions={[{ id: "remove", label: t("accounts.removeMenu"), icon: "trash", destructive: true, onSelect: () => void remove(a()) }]}
                    />
                  </View>
                </>
              )}
            </Each>
            <If when={accounts().length}>
              <Divider />
            </If>
            <View class="p-2">
              <Button variant="light" color="primary" icon="plus" label={t("accounts.add")} onPress={() => nav.stack.push({ name: "addAccount" })} class="self-start" testID="add-account" />
            </View>
          </Card>
        </View>

        <View class="gap-1">
          <SectionLabel label={t("settings.sync")} class="px-0" />
          <Card shadow="sm" class="gap-3 p-3">
            <View class="flex-row items-center gap-3">
              <Icon name={mailStatus().mode === "cloud" ? "cloud" : "device"} tone="default-500" />
              <View class="flex-1 gap-0.5">
                <Text class={`text-foreground ${text}`}>{t(`mode.${mailStatus().mode}`)}</Text>
                <If when={mailStatus().mode === "cloud" && mailStatus().user}>
                  {(user) => (
                    <Text class={`text-foreground ${note}`} testID="signed-in-as">
                      {t("settings.signedInAs", { email: user() })}
                    </Text>
                  )}
                </If>
                <Text class={`text-foreground-500 ${note}`}>
                  {mailStatus().mode === "cloud" ? (mailStatus().server ?? "") : t("settings.localDetail")}
                </Text>
              </View>
            </View>
            <If when={mailStatus().mode === "local" && platform.os === "android"}>
              <Text class={`text-foreground-500 ${note}`}>{t("settings.androidLocal")}</Text>
            </If>
            <Button variant="flat" color="danger" icon="logOut" label={t("settings.signOut")} onPress={() => void signOut()} class="self-start" testID="sign-out" />
          </Card>
        </View>

        <View class="gap-1">
          <SectionLabel label={t("settings.notifications")} class="px-0" />
          <Card shadow="sm" class="px-3 py-2">
            <Switch
              checked={notificationsEnabled()}
              onChange={(on) => void toggleNotifications(on)}
              label={t("settings.newMailNotifications")}
              description={t("settings.notificationsDetail")}
              testID="notifications"
            />
          </Card>
        </View>

        <Text class={`text-center text-foreground-500 ${note}`}>{t("settings.about")}</Text>
      </PageScroll>
    </NavPage>
  );
}
