// What a search shows: the hint before anything is typed, the loading indicator
// while it runs, the failure with a retry, and nothing found with a way back.
import { View } from "nsh";
import { t } from "nsh/i18n";
import { Button, Empty, LoadingIndicator } from "../ui";
import { clearSearch, runSearch, searchError, searchQuery, searchStatus } from "../store";

export function SearchEmpty() {
  const state = () =>
    !searchQuery().trim() ? "hint" : searchStatus() === "loading" ? "loading" : searchStatus() === "error" ? "error" : "none";
  return (
    <View class="flex-1" testID="search-empty">
      <If when={state() === "hint"}>
        <Empty icon="search" title={t("search.hint")} description={t("search.hintDetail")} />
      </If>
      <If when={state() === "loading"}>
        <View class="flex-1 items-center justify-center p-8">
          <LoadingIndicator label={t("search.running")} contained />
        </View>
      </If>
      <If when={state() === "error"}>
        <Empty
          icon="alert"
          title={t("search.failed")}
          description={searchError() ?? undefined}
          action={<Button variant="tonal" icon="refresh" label={t("actions.retry")} onPress={() => void runSearch(searchQuery())} testID="search-retry" />}
        />
      </If>
      {/* A status region: what the search found (or didn't) is announced as it changes. */}
      <View role="summary" class="flex-1">
        <If when={state() === "none"}>
          <Empty
            icon="search"
            title={t("search.none", { query: searchQuery().trim() })}
            description={t("search.noneHint")}
            action={<Button variant="text" label={t("search.clear")} onPress={clearSearch} testID="search-clear" />}
          />
        </If>
      </View>
    </View>
  );
}
