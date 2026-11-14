// The feed's filter chips and search bar, the same on the wide layout and on
// phones: every source with its unread count, and All.
import { Scroll, View, type InputRef } from "nsh";
import { t } from "nsh/i18n";
import { Badge, Chip, IconButton, TextField } from "../ui";
import { clearFeedQuery, feedFilter, feedQuery, setFeedFilter, sourceCounts, sources, submitFeedQuery, typeFeedQuery } from "../notify";
import { SourceMark } from "./SourceMark";

/** "N unread": the chip's count, a quiet one, since it is not an alert. */
export function count(count: number) {
  return count > 0 ? <Badge quiet label={count > 999 ? "999+" : String(count)} /> : undefined;
}

export function FeedFilters() {
  const all = () => Object.values(sourceCounts()).reduce((sum, n) => sum + n, 0);
  return (
    <Scroll horizontal class="w-full shrink-0" contentClass="flex-row items-center gap-2 px-4 py-1" testID="notify-filters">
      <Chip
        kind="filter"
        label={t("notify.all")}
        selected={feedFilter() === null}
        onPress={() => setFeedFilter(null)}
        end={count(all())}
        testID="notify-filter-all"
      />
      <Each of={sources()} key={(s) => s.id}>
        {(s) => (
          <Chip
            kind="filter"
            label={s().name}
            selected={feedFilter() === s().id}
            onPress={() => setFeedFilter(s().id)}
            start={<SourceMark source={s()} size="sm" />}
            end={count(sourceCounts()[s().id] ?? 0)}
            testID={`notify-filter-${s().id}`}
          />
        )}
      </Each>
    </Scroll>
  );
}

export function FeedSearch(props: { ref?: (ref: InputRef) => void }) {
  return (
    <View class="px-3 pb-2 pt-1">
      <TextField
        ref={props.ref}
        variant="search"
        value={feedQuery()}
        onChange={typeFeedQuery}
        onSubmit={() => submitFeedQuery(feedQuery())}
        placeholder={t("notify.searchPlaceholder")}
        label={t("notify.search")}
        returnKey="search"
        leadingIcon="search"
        trailing={
          <If when={feedQuery()}>
            <IconButton icon="close" label={t("search.clear")} size="xs" onPress={clearFeedQuery} class="-me-2" testID="notify-search-clear" />
          </If>
        }
        testID="notify-search"
      />
    </View>
  );
}
