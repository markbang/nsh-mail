// The phone's first page: the chosen mailbox. iOS and Android reach search,
// the mailboxes and compose from the tab bar and its button, and turn the top
// bar into the selection's actions while rows are selected; a narrow browser
// has search and compose in the header, the mailbox list in a sheet.
import { signal, Text, View } from "nsh";
import { BackHandler } from "nsh/mobile";
import { t } from "nsh/i18n";
import { AppBar, IconButton, Menu, Page, Sheet, systemHeader } from "../ui";
import { MailboxList, mailboxTitle } from "../components/Mailbox";
import { Mailboxes } from "../components/Mailboxes";
import { PageHeader } from "../components/PageHeader";
import { NavContext, selectTab } from "../nav";
import { clearPicked, mailbox, messages, moveMessages, openMessage, pickedIds, picking, refresh, setFlags } from "../store";

export function InboxPage() {
  const nav = NavContext();
  const sheet = signal(false);
  const open = (id: string) => {
    openMessage(id);
    nav.stack.push({ name: "message", id });
  };
  // Android back ends a selection; a browser has no BackHandler (it throws there).
  if (systemHeader) BackHandler.on("press", () => {
    if (!picking()) return false;
    clearPicked();
    return true;
  });
  const menu = () => <IconButton icon="menu" label={t("mailbox.choose")} onPress={() => sheet.set(true)} testID="mailboxes" />;
  const actions = () => (
    <>
      <IconButton icon="search" label={t("search.label")} onPress={() => nav.stack.push({ name: "search" })} testID="open-search" />
      <IconButton icon="compose" label={t("actions.compose")} onPress={() => nav.stack.push({ name: "compose", init: { mode: "new" } })} testID="compose" />
    </>
  );
  // The top bar of iOS and Android: search and more, or the selection's actions.
  const picked = () => messages().filter((m) => pickedIds().includes(m.id));
  const move = (folder: "archive" | "trash") => {
    const ids = pickedIds();
    clearPicked();
    moveMessages(ids, folder);
  };
  const mark = () => {
    const unread = picked().some((m) => !m.seen);
    for (const m of picked()) void setFlags(m.id, { seen: unread });
    clearPicked();
  };
  const more = () => (
    <Menu
      label={t("actions.more")}
      trigger={<IconButton icon="more" label={t("actions.more")} testID="inbox-more" />}
      actions={[
        { id: "refresh", label: t("actions.refresh"), icon: "refresh", onSelect: () => void refresh() },
        { id: "settings", label: t("settings.title"), icon: "settings", onSelect: () => selectTab(3) },
      ]}
    />
  );
  const pickedActions = () => (
    <>
      <If when={mailbox().folder !== "archive" && mailbox().folder !== "trash"}>
        <IconButton icon="archive" label={t("actions.archive")} onPress={() => move("archive")} testID="picked-archive" />
      </If>
      <If when={mailbox().folder !== "trash"}>
        <IconButton icon="trash" label={t("actions.delete")} onPress={() => move("trash")} testID="picked-delete" />
      </If>
      <IconButton
        icon={picked().some((m) => !m.seen) ? "mailOpen" : "mail"}
        label={picked().some((m) => !m.seen) ? t("actions.markRead") : t("actions.markUnread")}
        onPress={mark}
        testID="picked-seen"
      />
    </>
  );
  const barActions = () => (
    <If
      when={picking()}
      else={
        <>
          <IconButton icon="search" label={t("search.label")} onPress={() => selectTab(1)} testID="open-search" />
          {more()}
        </>
      }
    >
      {pickedActions()}
    </If>
  );
  const endSelection = () => <IconButton icon="close" label={t("actions.endSelection")} onPress={clearPicked} testID="end-selection" />;
  // Android: the search bar over an emphasized headline; while rows are selected, the selection's bar.
  const appBar = () => (
    <If
      when={picking()}
      else={<AppBar title={mailboxTitle()} search={{ label: t("search.placeholder"), onPress: () => selectTab(1), end: more() }} testID="app-bar" />}
    >
      <View class="bg-surface-container pt-[env(safe-area-inset-top)]">
        <View class="h-16 flex-row items-center gap-1 px-2">
          {endSelection()}
          <Text class="flex-1 px-2 text-title-large font-medium text-on-surface" testID="selection-count">
            {t("actions.selected", { count: pickedIds().length })}
          </Text>
          {pickedActions()}
        </View>
      </View>
    </If>
  );
  return (
    <Page
      title={picking() ? t("actions.selected", { count: pickedIds().length }) : mailboxTitle()}
      header={<PageHeader title={mailboxTitle()} leading={menu()} actions={actions()} />}
      headerLeft={picking() ? endSelection() : undefined}
      headerRight={barActions()}
      appBar={appBar()}
      root
    >
      <View class="flex-1 bg-surface">
        <MailboxList onOpen={open} onAddAccount={() => nav.stack.push({ name: "addAccount" })} searchable={false} touchActions={systemHeader} />
      </View>
      <If when={!systemHeader}>
        <Sheet open={sheet()} onClose={() => sheet.set(false)} side="left" label={t("mailbox.choose")}>
          <Mailboxes
            onSettings={() => {
              sheet.set(false);
              nav.stack.push({ name: "settings" });
            }}
            onChosen={() => sheet.set(false)}
          />
        </Sheet>
      </If>
    </Page>
  );
}
