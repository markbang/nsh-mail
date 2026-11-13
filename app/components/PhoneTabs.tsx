// iOS and Android phones: the system's tab bar over a stack per tab (Mail,
// Search, Mailboxes, Settings), each kept while another shows, and the
// compose button over the Mail tab's list. Pushed pages cover the bar.
import { effect, platform, signal, untracked, View, type JSX } from "nsh";
import { BackHandler, TabBar } from "nsh/mobile";
import { NavStack } from "nsh/base";
import { t } from "nsh/i18n";
import { Fab, FabMenu, material, NavBar, transition, type IconName } from "../ui";
import { NavContext, selectedTab, selectTab, tabs, type Route } from "../nav";
import { picking, toast, unreadCount } from "../store";
import { Toasts } from "./Toasts";

// Android: framework drawables (an APK made by `nsh package` has none of its own); iOS: SF Symbols.
const icons = {
  mail: platform.select({ ios: "tray", android: "android:stat_notify_email_generic" }),
  search: platform.select({ ios: "magnifyingglass", android: "android:ic_search_api_material" }),
  folders: platform.select({ ios: "folder", android: "android:ic_folder_24dp" }),
  settings: platform.select({ ios: "gearshape", android: "android:ic_settings_24dp" }),
} as const;
const navIcons = { mail: "inbox", search: "search", folders: "inboxes", settings: "settings" } as const satisfies Record<string, IconName>;

export function PhoneTabs(props: { page: (route: Route) => JSX.Element; onCompose: () => void; onAddAccount: () => void }) {
  const menu = signal(false);
  const depth = () => tabs[selectedTab()].stack.routes().length;
  // Android back on a tab's first page goes to Mail first, as Gmail's does; the stacks pop before that.
  BackHandler.on("press", () => {
    if (menu()) {
      menu.set(false);
      return true;
    }
    if (selectedTab() === 0 || depth() > 1) return false;
    selectTab(0);
    return true;
  });
  const choose = (index: number) => {
    menu.set(false);
    // Choosing the tab already shown goes back to its first page.
    if (index === selectedTab()) tabs[index].stack.popToRoot();
    selectTab(index);
  };
  const unread = () => unreadCount(null);
  // A tab's stack starts the first time it shows (Search focuses its field then), and stays.
  const opened = signal<Record<number, boolean>>({ 0: true });
  effect(() => {
    const index = selectedTab();
    if (!untracked(opened)[index]) opened.update((o) => ({ ...o, [index]: true }));
  });
  const covered = () => platform.os === "android" && depth() > 1;
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
        <If when={selectedTab() === 0 && depth() === 1 && !picking()}>
          <If
            when={material}
            else={
              <View class="absolute bottom-4 end-4">
                <Fab icon="compose" label={t("actions.compose")} extended onPress={props.onCompose} testID="compose" />
              </View>
            }
          >
            <View
              class={`absolute inset-0 bg-scrim ${transition.effects} ${menu() ? "opacity-30" : "opacity-0"}`}
              style={{ pointerEvents: menu() ? "auto" : "none" }}
              onPress={() => menu.set(false)}
              accessibilityElementsHidden
            />
            {/* Material: the button rises over a snackbar. */}
            <View class={`absolute bottom-4 end-4 ${transition.spatial} ${toast() ? "-translate-y-[64px]" : ""}`} style={{ pointerEvents: "box-none" }}>
              <FabMenu
                icon="compose"
                label={t("actions.newMenu")}
                closeLabel={t("common.close")}
                open={menu()}
                onOpenChange={(open) => menu.set(open)}
                items={[
                  { icon: "user", label: t("accounts.add"), onPress: props.onAddAccount, testID: "fab-add-account" },
                  { icon: "compose", label: t("actions.compose"), onPress: props.onCompose, testID: "compose" },
                ]}
                testID="fab-menu"
              />
            </View>
          </If>
        </If>
        <Toasts class={covered() ? "bottom-4 pb-[env(safe-area-inset-bottom)]" : "bottom-4"} />
      </View>
      <View class={covered() ? "hidden" : ""}>
        <If
          when={material}
          else={
            <TabBar
              items={tabs.map((tab) => ({ title: t(`tabs.${tab.name}`), icon: icons[tab.name] }))}
              selectedIndex={selectedTab()}
              onChange={({ index }) => choose(index)}
              testID="tab-bar"
            />
          }
        >
          <NavBar
            label={t("tabs.label")}
            items={tabs.map((tab, i) => ({
              title: t(`tabs.${tab.name}`),
              icon: navIcons[tab.name],
              badge: i === 0 && unread() > 0 ? (unread() > 999 ? "999+" : String(unread())) : undefined,
              testID: `tab-${tab.name}-button`,
            }))}
            selected={selectedTab()}
            onChange={choose}
          />
        </If>
      </View>
    </View>
  );
}
