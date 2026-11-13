// The chosen mailbox's conversations (or messages), or the search's results while a search runs.
import { View } from "nsh";
import { t } from "nsh/i18n";
import { Button, Empty, LoadingIndicator } from "../ui";
import { MessageList } from "./MessageList";
import { ScheduledList } from "./ScheduledList";
import {
  accountName,
  accounts,
  hasMore,
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
  searchQuery,
  searchResults,
  selectedId,
  selectedKey,
  togglePicked,
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

export function MailboxList(props: { onOpen: (row: Row) => void; onAddAccount: () => void; highlight?: boolean; searchable?: boolean; touchActions?: boolean }) {
  // Phones search on a page of their own; the list there stays the mailbox's.
  const searching = () => props.searchable !== false && searchResults() !== null;
  const folder = () => mailbox().folder;
  const open = (list: () => Row[]) => (key: string) => {
    const row = list().find((r) => r.key === key);
    if (row) props.onOpen(row);
  };
  const move = (folder: "archive" | "trash") => (key: string) => {
    const row = rows().find((r) => r.key === key);
    if (row) moveMessages([key], folder, row.thread);
  };
  return (
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
            loadingMore={listStatus() === "more" || (hasMore() && rows().length > 0 && listStatus() !== "loading")}
            showAccount={mailbox().account === null}
            showRecipients={mailbox().folder === "sent" || mailbox().folder === "drafts"}
            testID="messages"
            onArchive={props.touchActions && folder() !== "archive" && folder() !== "trash" ? move("archive") : undefined}
            onDelete={props.touchActions && folder() !== "trash" ? move("trash") : undefined}
            picked={isPicked}
            picking={!!props.touchActions && picking()}
            onPick={props.touchActions ? (id) => togglePicked(id) : undefined}
            empty={
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
                      <Empty icon="inbox" title={t("empty.mailbox")} />
                    </If>
                  }
                >
                  <Empty icon="inbox" title={t("empty.loadFailed")} action={<Button variant="tonal" icon="refresh" label={t("actions.retry")} onPress={() => void reload()} />} />
                </If>
              </If>
            }
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
        testID="results"
        empty={<Empty icon="search" title={t("search.none", { query: searchQuery() })} />}
      />
    </If>
  );
}
