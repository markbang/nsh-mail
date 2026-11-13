// The shell: onboarding until a mode is chosen, then three panes on wide
// windows, the system's tab bar on iOS and Android phones, or one stack in a
// narrow browser; the toast, keys and the tray.
import { keymap, mount, View, type InputRef } from "nsh";
import { NavStack } from "nsh/base";
import { Dialog, systemHeader } from "./ui";
import { Toasts } from "./components/Toasts";
import { PhoneTabs } from "./components/PhoneTabs";
import { Workspace } from "./components/Workspace";
import { wide } from "./layout";
import { closeModal, isModalOpen, modalFit, modalNavigator, modalStack, NavContext, phoneNavigator, phoneStack, selectTab, show, type Route } from "./nav";
import { AddAccountPage } from "./pages/AddAccountPage";
import { ComposePage } from "./pages/ComposePage";
import { FoldersPage } from "./pages/FoldersPage";
import { ImapPage } from "./pages/ImapPage";
import { InboxPage } from "./pages/InboxPage";
import { MessagePage } from "./pages/MessagePage";
import { Onboarding } from "./pages/Onboarding";
import { SearchPage } from "./pages/SearchPage";
import { SettingsPage } from "./pages/SettingsPage";
import { listenForPushToken } from "./push";
import { stayResident } from "./resident";
import { current, isStarted, mailStatus, moveMessages, openMessage, selectedId, start, stepMessage } from "./store";
import type { ComposeMode } from "./lib/format";

function page(route: Route) {
  switch (route.name) {
    case "inbox":
      return <InboxPage />;
    case "message":
      return <MessagePage id={route.id} />;
    case "compose":
      return <ComposePage init={route.init} />;
    case "search":
      return <SearchPage />;
    case "folders":
      return <FoldersPage />;
    case "settings":
      return <SettingsPage />;
    case "addAccount":
      return <AddAccountPage />;
    case "imap":
      return <ImapPage />;
  }
}

function Main() {
  let search: InputRef | undefined;
  const compose = (mode: ComposeMode) => {
    const source = mode === "new" ? undefined : current();
    if (mode !== "new" && !source) return;
    show({ name: "compose", init: { mode, source } }, wide());
  };
  const openFromNotification = (id: string) => {
    openMessage(id);
    if (wide()) return;
    selectTab(0);
    phoneStack.setRoutes([{ name: "inbox" }, { name: "message", id }]);
  };
  stayResident(openFromNotification);

  // Single keys, as mail apps have them; never while typing or in a dialog.
  const key = (run: () => unknown) => () => {
    if (!isModalOpen()) run();
  };
  keymap.bind(
    {
      c: key(() => compose("new")),
      r: key(() => wide() && selectedId() && compose("reply")),
      "/": key(() => (wide() ? search?.focus() : systemHeader ? selectTab(1) : phoneStack.push({ name: "search" }))),
      e: key(() => wide() && selectedId() && moveMessages([selectedId()!], "archive")),
      "shift+3": key(() => wide() && selectedId() && moveMessages([selectedId()!], "trash")),
      j: key(() => wide() && stepMessage(1)),
      k: key(() => wide() && stepMessage(-1)),
    },
    { when: "notEditing" },
  );

  return (
    <If
      when={wide()}
      else={
        <If
          when={systemHeader}
          else={
            <NavContext.Provider value={phoneNavigator}>
              <NavStack state={phoneStack} class="flex-1">
                {page}
              </NavStack>
            </NavContext.Provider>
          }
        >
          <PhoneTabs page={page} onCompose={() => compose("new")} onAddAccount={() => phoneStack.push({ name: "addAccount" })} />
        </If>
      }
    >
      {/* Tablets draw edge to edge: the panes stay clear of the status bar and the taskbar. */}
      <View class="flex-1 pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)] ps-[env(safe-area-inset-left)] pe-[env(safe-area-inset-right)]">
        <Workspace
          onCompose={compose}
          onSettings={() => show({ name: "settings" }, true)}
          onAddAccount={() => show({ name: "addAccount" }, true)}
          searchRef={(ref) => (search = ref)}
        />
      </View>
      <Dialog open={isModalOpen()} onClose={closeModal} label="nsh-mail" size="3xl" height={modalFit()} bare testID="modal">
        <NavContext.Provider value={modalNavigator}>
          <NavStack state={modalStack} class="flex-1">
            {page}
          </NavStack>
        </NavContext.Provider>
      </Dialog>
    </If>
  );
}

function App() {
  void start();
  return (
    <View class="flex-1 bg-surface">
      <If when={isStarted()}>
        <If when={mailStatus().mode !== "none"} else={<Onboarding />}>
          <Main />
        </If>
      </If>
      {/* Phones show it over the tab's content (PhoneTabs). */}
      <If when={!(systemHeader && !wide() && isStarted() && mailStatus().mode !== "none")}>
        <Toasts class="bottom-6 pb-[env(safe-area-inset-bottom)]" />
      </If>
    </View>
  );
}

listenForPushToken();
mount(App);
