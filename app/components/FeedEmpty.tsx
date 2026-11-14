// What the feed shows with nothing in it, and the next step: no sources yet,
// a filter or search that found nothing, or a load that failed.
import { View } from "nsh";
import { t } from "nsh/i18n";
import { Button, Empty, LoadingIndicator } from "../ui";
import { feedError, feedFilter, feedQuery, feedStatus, clearFeedQuery, reloadFeed, sources } from "../notify";

export function FeedEmpty(props: { onAddSource: () => void; onClearFilters: () => void }) {
  const filtered = () => !!feedQuery().trim() || !!feedFilter();
  const clear = () => {
    clearFeedQuery();
    props.onClearFilters();
  };
  return (
    <If
      when={feedStatus() !== "loading"}
      else={
        <View class="flex-1 items-center justify-center p-8">
          <LoadingIndicator label={t("notify.loading")} contained />
        </View>
      }
    >
      <If
        when={feedStatus() === "error"}
        else={
          <If
            when={sources().length}
            else={
              <Empty
                icon="bell"
                title={t("notify.noSources")}
                description={t("notify.noSourcesHint")}
                action={<Button icon="plus" label={t("sources.add")} onPress={props.onAddSource} testID="empty-add-source" />}
              />
            }
          >
            <If
              when={filtered()}
              else={
                <Empty
                  icon="bell"
                  title={t("notify.empty")}
                  description={t("notify.emptyHint")}
                  action={<Button icon="plus" label={t("sources.add")} onPress={props.onAddSource} testID="empty-add-source" />}
                />
              }
            >
              <Empty
                icon="filter"
                title={t("notify.nothing")}
                description={t("notify.nothingHint")}
                action={<Button variant="tonal" icon="close" label={t("notify.clearFilters")} onPress={clear} testID="empty-clear-filters" />}
              />
            </If>
          </If>
        }
      >
        <Empty
          icon="alert"
          title={t("notify.failed")}
          description={feedError() ?? t("notify.failedHint")}
          action={<Button variant="tonal" icon="refresh" label={t("actions.retry")} onPress={() => void reloadFeed()} testID="feed-retry" />}
        />
      </If>
    </If>
  );
}
