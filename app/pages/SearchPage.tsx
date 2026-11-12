// Search on its own page (phones and narrow windows).
import { cleanup, View } from "nsh";
import { NavPage } from "nsh/base";
import { t } from "nsh/i18n";
import { Empty } from "../ui";
import { MessageList } from "../components/MessageList";
import { PageHeader } from "../components/PageHeader";
import { SearchField } from "../components/SearchField";
import { NavContext } from "../nav";
import { clearSearch, openMessage, searchQuery, searchResults } from "../store";

export function SearchPage() {
  const nav = NavContext();
  cleanup(clearSearch);
  return (
    <NavPage title={t("search.label")} header={<PageHeader title={t("search.label")} />}>
      <View class="flex-1 bg-background">
        <View class="border-b border-border p-3">
          <SearchField autoFocus />
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
              testID="results"
              empty={<Empty icon="search" title={t("search.none", { query: searchQuery() })} />}
            />
          )}
        </If>
      </View>
    </NavPage>
  );
}
