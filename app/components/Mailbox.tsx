// The chosen mailbox's messages, or the search's results while a search runs.
import { t } from "nsh/i18n";
import { Button, Empty } from "../ui";
import { MessageList } from "./MessageList";
import { accounts, hasMore, listStatus, loadMore, mailbox, messages, refresh, searchQuery, searchResults, selectedId } from "../store";

/** The chosen mailbox's name. */
export function mailboxTitle() {
  const box = mailbox();
  return box.folder === "inbox"
    ? box.account
      ? (accounts().find((a) => a.id === box.account)?.email ?? t("folder.inbox"))
      : t("mailbox.allInboxes")
    : t(`folder.${box.folder}`);
}

export function MailboxList(props: { onOpen: (id: string) => void; onAddAccount: () => void; highlight?: boolean }) {
  const searching = () => searchResults() !== null;
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
          empty={
            <If when={listStatus() !== "loading"}>
              <If
                when={accounts().length}
                else={
                  <Empty
                    icon="inbox"
                    title={t("empty.noAccounts")}
                    description={t("empty.noAccountsHint")}
                    action={<Button variant="primary" icon="plus" label={t("accounts.add")} onPress={props.onAddAccount} />}
                  />
                }
              >
                <Empty icon="inbox" title={t("empty.mailbox")} />
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
        testID="results"
        empty={<Empty icon="search" title={t("search.none", { query: searchQuery() })} />}
      />
    </If>
  );
}
