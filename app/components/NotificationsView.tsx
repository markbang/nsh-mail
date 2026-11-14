// The wide layout's second view (the rail's first destination): every
// notification in one list on the start side, the open one beside it.
import { Text, View } from "nsh";
import { t } from "nsh/i18n";
import { Empty, IconButton, LoadingIndicator, Pane, Panes } from "../ui";
import { FeedEmpty } from "./FeedEmpty";
import { FeedFilters, FeedSearch } from "./FeedFilters";
import { NotificationList } from "./NotificationList";
import { NotificationReader } from "./NotificationReader";
import {
  archiveNotifications,
  feedStatus,
  loadMoreFeed,
  markNotifications,
  notifications,
  openNotification,
  openNotificationAt,
  reloadFeed,
  setFeedFilter,
  showMail,
  sourceById,
} from "../notify";
import { openMailbox, openMessage } from "../store";

export function NotificationsView(props: { onAddSource: () => void; onOpenSources: () => void }) {
  const open = () => openNotification();
  const openInMail = (id: string) => {
    showMail();
    openMailbox({ account: null, folder: "inbox" });
    openMessage(id);
  };
  return (
    <Panes label={t("layout.resize")} class="py-3 pe-3">
      <Pane size={420} minSize={340} maxSize={620} fixed first>
        <View class="min-w-0 flex-1 overflow-hidden rounded-large bg-surface" testID="notifications">
          <View class="flex-row items-center gap-2 pe-2 ps-5 pt-3">
            <Text role="header" class="line-clamp-1 flex-1 text-title-large tracking-title-large text-on-surface">
              {t("notify.allTitle")}
            </Text>
            <If
              when={feedStatus() === "refreshing"}
              else={<IconButton icon="refresh" label={t("actions.refresh")} onPress={() => void reloadFeed("refreshing")} testID="notify-refresh" />}
            >
              <LoadingIndicator label={t("sync.running")} size={40} testID="notify-refreshing" />
            </If>
            <IconButton icon="settings" label={t("sources.title")} onPress={() => props.onOpenSources()} testID="notify-sources" />
          </View>
          <FeedFilters />
          <FeedSearch />
          <NotificationList
            items={notifications()}
            selectedId={open()?.id ?? null}
            onOpen={openNotificationAt}
            refreshing={feedStatus() === "refreshing"}
            onRefresh={() => void reloadFeed("refreshing")}
            onLoadMore={() => void loadMoreFeed()}
            empty={<FeedEmpty onAddSource={props.onAddSource} onClearFilters={() => setFeedFilter(null)} />}
            testID="notify-list"
          />
        </View>
      </Pane>
      <Pane minSize={360} last>
        <View class="flex-1 overflow-hidden rounded-large bg-surface" testID="notify-pane">
          <If when={open()} else={<Empty icon="bell" title={t("notify.noSelection")} description={t("notify.noSelectionHint")} />}>
            {(n) => (
              <NotificationReader
                note={n()}
                source={sourceById(n().sourceId)}
                onToggleRead={() => void markNotifications([n().id], !n().read)}
                onArchive={() => archiveNotifications([n().id])}
                onOpenMail={n().threadId ? () => openInMail(n().id) : undefined}
              />
            )}
          </If>
        </View>
      </Pane>
    </Panes>
  );
}
