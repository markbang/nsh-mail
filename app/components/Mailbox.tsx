// The chosen mailbox's messages, or the search's results while a search runs.
import { View } from "nsh";
import { t } from "nsh/i18n";
import { Button, Empty, Spinner } from "../ui";
import { MessageList } from "./MessageList";
import {
  accountName,
  accounts,
  hasMore,
  isPicked,
  listStatus,
  loadMore,
  mailbox,
  messages,
  moveMessages,
  picking,
  refresh,
  reload,
  SEARCH_LIMIT,
  searchQuery,
  searchResults,
  selectedId,
  togglePicked,
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

export function MailboxList(props: { onOpen: (id: string) => void; onAddAccount: () => void; highlight?: boolean; searchable?: boolean; touchActions?: boolean }) {
  // Phones search on a page of their own; the list there stays the mailbox's.
  const searching = () => props.searchable !== false && searchResults() !== null;
  const folder = () => mailbox().folder;
  return (
    <If
      when={searching()}
      else={
        <MessageList
          items={messages()}
          selectedId={props.highlight ? selectedId() : null}
          onOpen={props.onOpen}
          refreshing={listStatus() === "refreshing"}
          onRefresh={() => void refresh()}
          onLoadMore={() => void loadMore()}
          loadingMore={listStatus() === "more" || (hasMore() && messages().length > 0 && listStatus() !== "loading")}
          showAccount={mailbox().account === null}
          showRecipients={mailbox().folder === "sent" || mailbox().folder === "drafts"}
          testID="messages"
          onArchive={props.touchActions && folder() !== "archive" && folder() !== "trash" ? (id) => moveMessages([id], "archive") : undefined}
          onDelete={props.touchActions && folder() !== "trash" ? (id) => moveMessages([id], "trash") : undefined}
          picked={isPicked}
          picking={!!props.touchActions && picking()}
          onPick={props.touchActions ? (id) => togglePicked(id) : undefined}
          empty={
            <If
              when={listStatus() !== "loading"}
              else={
                <View class="flex-1 items-center justify-center p-8">
                  <Spinner label={t("reader.loading")} />
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
                        action={<Button color="primary" icon="plus" label={t("accounts.add")} onPress={props.onAddAccount} />}
                      />
                    }
                  >
                    <Empty icon="inbox" title={t("empty.mailbox")} />
                  </If>
                }
              >
                <Empty icon="inbox" title={t("empty.loadFailed")} action={<Button variant="flat" icon="refresh" label={t("actions.retry")} onPress={() => void reload()} />} />
              </If>
            </If>
          }
        />
      }
    >
      <MessageList
        items={searchResults() ?? []}
        selectedId={props.highlight ? selectedId() : null}
        onOpen={props.onOpen}
        showAccount
        highlight={searchQuery()}
        testID="results"
        empty={<Empty icon="search" title={t("search.none", { query: searchQuery() })} />}
      />
    </If>
  );
}
