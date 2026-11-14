// Search on its own page (phones and narrow windows).
import { cleanup, Text, View } from "nsh";
import { t } from "nsh/i18n";
import { AppBar, Page, systemHeader } from "../ui";
import { resultCount } from "../components/Mailbox";
import { MessageList } from "../components/MessageList";
import { PageHeader } from "../components/PageHeader";
import { SearchEmpty } from "../components/SearchEmpty";
import { SearchField } from "../components/SearchField";
import { NavContext } from "../nav";
import { clearSearch, openRow, searchActive, searchQuery, searchResults } from "../store";

export function SearchPage() {
  const nav = NavContext();
  cleanup(clearSearch);
  return (
    <Page title={t("search.label")} header={<PageHeader title={t("search.label")} />} root={systemHeader} appBar={<AppBar title={t("search.label")} />}>
      <View class="flex-1 bg-surface">
        <View class="px-4 pb-2 pt-2">
          <SearchField autoFocus />
        </View>
        <If
          when={searchActive()}
          else={
            <View class="flex-1">
              <SearchEmpty />
            </View>
          }
        >
          <View class="flex-1">
            <If when={searchResults()?.length}>
              {(count) => (
                <Text role="summary" class="px-4 pb-2 text-label-large tabular-nums text-on-surface-variant" testID="result-count">
                  {resultCount(count())}
                </Text>
              )}
            </If>
            <MessageList
              items={searchResults() ?? []}
              onOpen={(id) => {
                const row = (searchResults() ?? []).find((r) => r.key === id);
                if (!row) return;
                openRow(row);
                nav.stack.push({ name: "message", id });
              }}
              showAccount
              highlight={searchQuery()}
              label={t("search.results")}
              testID="results"
              empty={<SearchEmpty />}
            />
          </View>
        </If>
      </View>
    </Page>
  );
}
