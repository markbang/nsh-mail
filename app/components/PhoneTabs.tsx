// iOS and Android phones: the system's tab bar over a stack per tab (Mail,
// Search, Mailboxes, Settings), each kept while another shows, and the
// compose button over the Mail tab's list. Pushed pages cover the bar.
import { effect, platform, signal, Text, untracked, View, type JSX } from "nsh";
import { BackHandler, TabBar } from "nsh/mobile";
import { NavStack } from "nsh/base";
import { t } from "nsh/i18n";
import { Fab } from "../ui";
import { NavContext, selectedTab, selectTab, tabs, type Route } from "../nav";
import { unreadCount } from "../store";

// Android: framework drawables (an APK made by `nsh package` has none of its own); iOS: SF Symbols.
const icons = {
  mail: platform.select({ ios: "tray", android: "android:stat_notify_email_generic" }),
  search: platform.select({ ios: "magnifyingglass", android: "android:ic_search_api_material" }),
  folders: platform.select({ ios: "folder", android: "android:ic_folder_24dp" }),
  settings: platform.select({ ios: "gearshape", android: "android:ic_settings_24dp" }),
} as const;

export function PhoneTabs(props: { page: (route: Route) => JSX.Element; onCompose: () => void }) {
  const depth = () => tabs[selectedTab()].stack.routes().length;
  // Android back on a tab's first page goes to Mail first, as Gmail's does; the stacks pop before that.
  BackHandler.on("press", () => {
    if (selectedTab() === 0 || depth() > 1) return false;
    selectTab(0);
    return true;
  });
  const choose = (index: number) => {
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
        <If when={selectedTab() === 0 && depth() === 1}>
          <View class="absolute bottom-4 end-4">
            <Fab icon="compose" label={t("actions.compose")} extended onPress={props.onCompose} testID="compose" />
          </View>
        </If>
      </View>
      <View class={covered() ? "hidden" : ""}>
        <TabBar
          items={tabs.map((tab) => ({ title: t(`tabs.${tab.name}`), icon: icons[tab.name] }))}
          selectedIndex={selectedTab()}
          onChange={({ index }) => choose(index)}
          testID="tab-bar"
        />
        {/* NSH 0.5's TabBar has no badges: Material 3's large badge, drawn over the Mail tab's icon. */}
        <If when={platform.os === "android" && unread() > 0}>
          <View
            accessibilityElementsHidden
            class="absolute top-[14px] h-4 min-w-4 items-center justify-center rounded-full bg-error px-1"
            style={{ left: `${50 / tabs.length}%`, marginLeft: 2, pointerEvents: "none" }}
          >
            <Text class="text-[11px] font-medium leading-4 text-on-error">{unread() > 999 ? "999+" : String(unread())}</Text>
          </View>
        </If>
      </View>
    </View>
  );
}
