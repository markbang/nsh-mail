// Narrow browser windows (M3's compact and medium window classes): the
// navigation bar over a stack per section (Mail, Search, Mailboxes,
// Settings), each kept while another shows, and the FAB menu (compose, add an
// account) over the Mail section's list. Pushed pages cover the bar.
import { effect, signal, untracked, View, type JSX } from "nsh";
import { NavStack } from "nsh/base";
import { t } from "nsh/i18n";
import { FabMenu, NavBar } from "../ui";
import { NavContext, selectedTab, selectTab, tabs, TAB, type Route } from "../nav";
import { unreadNotifications } from "../notify";
import { unreadCount } from "../store";

const icons = { mail: "inbox", notifications: "bell", search: "search", folders: "inboxes", settings: "settings" } as const;

export function WebTabs(props: { page: (route: Route) => JSX.Element; onCompose: () => void; onAddAccount: () => void }) {
  const depth = () => tabs[selectedTab()].stack.routes().length;
  const choose = (index: number) => {
    // Choosing the section already shown goes back to its first page.
    if (index === selectedTab()) tabs[index].stack.popToRoot();
    selectTab(index);
  };
  const opened = signal<Record<number, boolean>>({ 0: true });
  effect(() => {
    const index = selectedTab();
    if (!untracked(opened)[index]) opened.update((o) => ({ ...o, [index]: true }));
  });
  const fab = signal(false);
  const unread = () => unreadCount(null);
  const unreadFeed = () => unreadNotifications();
  return (
    <View class="flex-1 bg-surface">
      <View class="flex-1">
        <Each of={tabs} key={(tab) => tab.name}>
          {(tab, index) => (
            <View class={index() === selectedTab() ? "flex-1" : "hidden"}>
              <If when={opened()[index()]}>
                <NavContext.Provider value={tab().navigator}>
                  <NavStack state={tab().stack} class="flex-1" testID={`tab-${tab().name}`}>
                    {props.page}
                  </NavStack>
                </NavContext.Provider>
              </If>
            </View>
          )}
        </Each>
        <If when={selectedTab() === TAB.mail && depth() === 1}>
          <View class="absolute bottom-4 end-4" style={{ pointerEvents: "box-none" }}>
            <FabMenu
              icon="compose"
              label={t("actions.new")}
              closeLabel={t("common.close")}
              open={fab()}
              onOpenChange={(o) => fab.set(o)}
              items={[
                { icon: "compose", label: t("actions.compose"), onPress: props.onCompose, testID: "compose" },
                { icon: "plus", label: t("accounts.add"), onPress: props.onAddAccount, testID: "fab-add-account" },
              ]}
              testID="fab-menu"
            />
          </View>
        </If>
      </View>
      <View class={depth() > 1 ? "hidden" : ""}>
        <NavBar
          label={t("mailbox.choose")}
          selected={selectedTab()}
          onChange={choose}
          items={tabs.map((tab) => ({
            title: t(`tabs.${tab.name}`),
            icon: icons[tab.name],
            badge:
              tab.name === "mail" && unread() > 0
                ? unread() > 999
                  ? "999+"
                  : String(unread())
                : tab.name === "notifications" && unreadFeed() > 0
                  ? unreadFeed() > 999
                    ? "999+"
                    : String(unreadFeed())
                  : undefined,
            testID: `nav-${tab.name}`,
          }))}
        />
      </View>
    </View>
  );
}
