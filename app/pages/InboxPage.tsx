// The phone's first page: the chosen mailbox. iOS and Android reach search,
// the mailboxes and compose from the tab bar and its button, and turn the top
// bar into the selection's actions while rows are selected; a narrow browser
// reaches them from the navigation bar and the FAB menu.
import { Text, View } from "nsh";
import { BackHandler } from "nsh/mobile";
import { t } from "nsh/i18n";
import { AppBar, IconButton, Menu, Page, systemHeader } from "../ui";
import { MailboxList, mailboxTitle } from "../components/Mailbox";
import { PageHeader } from "../components/PageHeader";
import { NavContext, selectTab, TAB } from "../nav";
import { clearPicked, mailbox, moveMessages, openRow, pickedIds, picking, refresh, rows, setFlags, setThreadFlags, type Row } from "../store";

export function InboxPage() {
  const nav = NavContext();
  const open = (row: Row) => {
    openRow(row);
    nav.stack.push({ name: "message", id: row.latest.id });
  };
  // Android back ends a selection; a browser has no BackHandler (it throws there).
  if (systemHeader) BackHandler.on("press", () => {
    if (!picking()) return false;
    clearPicked();
    return true;
  });
  // The top bar of iOS and Android: search and more, or the selection's actions.
  const picked = () => rows().filter((r) => pickedIds().includes(r.key));
  const move = (folder: "archive" | "trash") => {
    const chosen = picked();
    clearPicked();
    moveMessages(
      chosen.map((r) => r.key),
      folder,
      chosen.some((r) => r.thread),
    );
  };
  const mark = () => {
    const unread = picked().some((r) => r.unread > 0);
    for (const r of picked()) void (r.thread ? setThreadFlags(r.key, { seen: unread }) : setFlags(r.key, { seen: unread }));
    clearPicked();
  };
  const more = () => (
    <Menu
      label={t("actions.more")}
      trigger={<IconButton icon="more" label={t("actions.more")} testID="inbox-more" />}
      actions={[
        { id: "refresh", label: t("actions.refresh"), icon: "refresh", onSelect: () => void refresh() },
        { id: "settings", label: t("settings.title"), icon: "settings", onSelect: () => selectTab(TAB.settings) },
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
        icon={picked().some((r) => r.unread > 0) ? "mailOpen" : "mail"}
        label={picked().some((r) => r.unread > 0) ? t("actions.markRead") : t("actions.markUnread")}
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
          <IconButton icon="search" label={t("search.label")} onPress={() => selectTab(TAB.search)} testID="open-search" />
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
      else={<AppBar title={mailboxTitle()} search={{ label: t("search.placeholder"), onPress: () => selectTab(TAB.search), end: more() }} testID="app-bar" />}
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
      header={<PageHeader title={mailboxTitle()} />}
      headerLeft={picking() ? endSelection() : undefined}
      headerRight={barActions()}
      appBar={appBar()}
      root
    >
      <View class="flex-1 bg-surface">
        <MailboxList
          onOpen={open}
          onAddAccount={() => nav.stack.push({ name: "addAccount" })}
          onCompose={() => nav.stack.push({ name: "compose", init: { mode: "new" } })}
          searchable={false}
          touchActions={systemHeader}
        />
      </View>
    </Page>
  );
}
