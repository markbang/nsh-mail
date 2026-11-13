// The shell: onboarding until a mode is chosen, then three panes on wide
// windows or a navigation stack on phones; the toast, keys and the tray.
import { keymap, mount, View, type InputRef } from "nsh";
import { NavStack } from "nsh/base";
import { Dialog, Toast } from "./ui";
import { Workspace } from "./components/Workspace";
import { wide } from "./layout";
import { closeModal, isModalOpen, modalFit, modalNavigator, modalStack, NavContext, phoneNavigator, phoneStack, show, type Route } from "./nav";
import { AddAccountPage } from "./pages/AddAccountPage";
import { ComposePage } from "./pages/ComposePage";
import { ImapPage } from "./pages/ImapPage";
import { InboxPage } from "./pages/InboxPage";
import { MessagePage } from "./pages/MessagePage";
import { Onboarding } from "./pages/Onboarding";
import { SearchPage } from "./pages/SearchPage";
import { SettingsPage } from "./pages/SettingsPage";
import { listenForPushToken } from "./push";
import { stayResident } from "./resident";
import { current, isStarted, mailStatus, openMessage, selectedId, start, stepMessage, toast } from "./store";
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
    if (!wide()) phoneStack.setRoutes([{ name: "inbox" }, { name: "message", id }]);
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
      "/": key(() => (wide() ? search?.focus() : phoneStack.push({ name: "search" }))),
      j: key(() => wide() && stepMessage(1)),
      k: key(() => wide() && stepMessage(-1)),
    },
    { when: "notEditing" },
  );

  return (
    <If
      when={wide()}
      else={
        <NavContext.Provider value={phoneNavigator}>
          <NavStack state={phoneStack} class="flex-1">
            {page}
          </NavStack>
        </NavContext.Provider>
      }
    >
      <Workspace
        onCompose={compose}
        onSettings={() => show({ name: "settings" }, true)}
        onAddAccount={() => show({ name: "addAccount" }, true)}
        searchRef={(ref) => (search = ref)}
      />
      <Dialog open={isModalOpen()} onClose={closeModal} label="nsh-mail" size="lg" height={modalFit()} bare testID="modal">
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
    <View class="flex-1 bg-background">
      <If when={isStarted()}>
        <If when={mailStatus().mode !== "none"} else={<Onboarding />}>
          <Main />
        </If>
      </If>
      <View class="absolute inset-x-0 bottom-6 items-center px-4 pb-[env(safe-area-inset-bottom)]" style={{ pointerEvents: "box-none" }}>
        <Each of={toast() ? [toast()!] : []} key={(x) => x.id}>
          {(x) => <Toast text={x().text} tone={x().tone} />}
        </Each>
      </View>
    </View>
  );
}

listenForPushToken();
mount(App);
