// The chosen mailbox's conversations (or messages), or the search's results while a search runs.
import { View } from "nsh";
import { t } from "nsh/i18n";
import { Button, Empty, LoadingIndicator, type IconName } from "../ui";
import { MessageList } from "./MessageList";
import { ScheduledList } from "./ScheduledList";
import { SearchEmpty } from "./SearchEmpty";
import { SyncBanner } from "./SyncBanner";
import {
  accountName,
  accounts,
  isPicked,
  listStatus,
  loadMore,
  mailbox,
  moveMessages,
  picking,
  refresh,
  reload,
  rows,
  SEARCH_LIMIT,
  searchActive,
  searchQuery,
  searchResults,
  selectedId,
  selectedKey,
  togglePicked,
  type Mailbox,
  type Row,
} from "../store";

/** The chosen mailbox's name. */
export function mailboxTitle() {
  const box = mailbox();
  return box.folder === "inbox"
    ? box.account
      ? (accountName(box.account) ?? t("folder.inbox"))
      : t("mailbox.allInboxes")
    : t(`folder.${box.folder}`);
}

/** How many messages a search found, as the list's header says it. */
export function resultCount(count: number) {
  if (count >= SEARCH_LIMIT) return t("search.countMore", { count: SEARCH_LIMIT });
  return count === 1 ? t("search.countOne") : t("search.count", { count });
}

const emptyIcons: Record<Mailbox["folder"], IconName> = {
  inbox: "inbox",
  sent: "send",
  drafts: "drafts",
  archive: "archive",
  trash: "trash",
  scheduled: "clock",
};
// An empty Sent, Drafts or Inbox is one message away from not being empty; the other two are places to look.
const canCompose = (folder: Mailbox["folder"]) => folder !== "archive" && folder !== "trash";

export function MailboxList(props: {
  onOpen: (row: Row) => void;
  onAddAccount: () => void;
  onCompose: () => void;
  highlight?: boolean;
  searchable?: boolean;
  touchActions?: boolean;
}) {
  // Phones search on a page of their own; the list there stays the mailbox's.
  const searching = () => props.searchable !== false && searchActive();
  const folder = () => mailbox().folder;
  const open = (list: () => Row[]) => (key: string) => {
    const row = list().find((r) => r.key === key);
    if (row) props.onOpen(row);
  };
  const move = (folder: "archive" | "trash") => (key: string) => {
    const row = rows().find((r) => r.key === key);
    if (row) moveMessages([key], folder, row.thread);
  };
  const empty = () => (
    <If
      when={listStatus() !== "loading"}
      else={
        <View class="flex-1 items-center justify-center p-8">
          <LoadingIndicator label={t("reader.loading")} contained />
        </View>
      }
    >
      <If
        when={listStatus() === "error"}
        else={
          <If
            when={accounts().length}
            else={
              <Empty
                icon="inbox"
                title={t("empty.noAccounts")}
                description={t("empty.noAccountsHint")}
                action={<Button icon="plus" label={t("accounts.add")} onPress={props.onAddAccount} />}
              />
            }
          >
            <Empty
              icon={emptyIcons[folder()]}
              title={t(`empty.${folder()}.title`)}
              description={t(`empty.${folder()}.hint`)}
              action={
                <If when={canCompose(folder())}>
                  <Button icon="compose" label={t("actions.compose")} onPress={props.onCompose} testID="empty-compose" />
                </If>
              }
            />
          </If>
        }
      >
        <Empty
          icon="alert"
          title={t("empty.loadFailed")}
          description={t("empty.loadFailedHint")}
          action={<Button variant="tonal" icon="refresh" label={t("actions.retry")} onPress={() => void reload()} testID="list-retry" />}
        />
      </If>
    </If>
  );
  return (
    <View class="flex-1">
      <SyncBanner />
      <If
        when={searching()}
        else={
          <If when={folder() !== "scheduled"} else={<ScheduledList />}>
            <MessageList
              items={rows()}
              selectedId={props.highlight ? selectedKey() : null}
              onOpen={open(rows)}
              refreshing={listStatus() === "refreshing"}
              onRefresh={() => void refresh()}
              onLoadMore={() => void loadMore()}
              loadingMore={listStatus() === "more"}
              showAccount={mailbox().account === null}
              showRecipients={mailbox().folder === "sent" || mailbox().folder === "drafts"}
              label={t("list.conversations")}
              testID="messages"
              onArchive={props.touchActions && folder() !== "archive" && folder() !== "trash" ? move("archive") : undefined}
              onDelete={props.touchActions && folder() !== "trash" ? move("trash") : undefined}
              picked={isPicked}
              picking={!!props.touchActions && picking()}
              onPick={props.touchActions ? (id) => togglePicked(id) : undefined}
              empty={empty()}
            />
          </If>
        }
      >
        <MessageList
          items={searchResults() ?? []}
          selectedId={props.highlight ? selectedId() : null}
          onOpen={open(() => searchResults() ?? [])}
          showAccount
          highlight={searchQuery()}
          label={t("search.results")}
          testID="results"
          empty={<SearchEmpty />}
        />
      </If>
    </View>
  );
}
