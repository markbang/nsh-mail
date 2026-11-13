// Search on its own page (phones and narrow windows).
import { cleanup, Text, View } from "nsh";
import { t } from "nsh/i18n";
import { AppBar, Empty, Page, systemHeader } from "../ui";
import { resultCount } from "../components/Mailbox";
import { MessageList } from "../components/MessageList";
import { PageHeader } from "../components/PageHeader";
import { SearchField } from "../components/SearchField";
import { NavContext } from "../nav";
import { clearSearch, openMessage, searchQuery, searchResults } from "../store";

export function SearchPage() {
  const nav = NavContext();
  cleanup(clearSearch);
  return (
    <Page title={t("search.label")} header={<PageHeader title={t("search.label")} />} root={systemHeader} appBar={<AppBar title={t("search.label")} />}>
      <View class="flex-1 bg-surface">
        <View class="gap-2 px-4 pb-2 pt-2">
          <SearchField autoFocus />
          <If when={searchResults()}>
            {(found) => <Text class="px-4 text-label-large tabular-nums text-on-surface-variant">{resultCount(found().length)}</Text>}
          </If>
        </View>
        <If when={searchResults()} else={<Empty icon="search" title={t("search.hint")} />}>
          {(found) => (
            <MessageList
              items={found()}
              onOpen={(id) => {
                openMessage(id);
                nav.stack.push({ name: "message", id });
              }}
              showAccount
              highlight={searchQuery()}
              testID="results"
              empty={<Empty icon="search" title={t("search.none", { query: searchQuery() })} />}
            />
          )}
        </If>
      </View>
    </Page>
  );
}
