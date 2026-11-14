// The phone's Notifications tab: every source's notifications in one feed,
// filter chips and search over it, and the open one on a page of its own.
import { View } from "nsh";
import { t } from "nsh/i18n";
import { AppBar, IconButton, Menu, Page } from "../ui";
import { FeedEmpty } from "../components/FeedEmpty";
import { FeedFilters, FeedSearch } from "../components/FeedFilters";
import { NotificationList } from "../components/NotificationList";
import { PageHeader } from "../components/PageHeader";
import { NavContext } from "../nav";
import { feedStatus, loadMoreFeed, notifications, openNotificationAt, reloadFeed, setFeedFilter } from "../notify";

export function NotificationsPage() {
  const nav = NavContext();
  const open = (id: string) => {
    openNotificationAt(id);
    nav.stack.push({ name: "notification", id });
  };
  const more = () => (
    <Menu
      label={t("notify.actions")}
      trigger={<IconButton icon="more" label={t("notify.actions")} testID="notify-more" />}
      actions={[
        { id: "refresh", label: t("actions.refresh"), icon: "refresh", onSelect: () => void reloadFeed("refreshing") },
        { id: "sources", label: t("sources.title"), icon: "bell", onSelect: () => nav.stack.push({ name: "sources" }) },
        { id: "keys", label: t("keys.title"), icon: "key", onSelect: () => nav.stack.push({ name: "apiKeys" }) },
        { id: "rules", label: t("rules.title"), icon: "settings", onSelect: () => nav.stack.push({ name: "notifyRules" }) },
      ]}
    />
  );
  const list = () => (
    <NotificationList
      items={notifications()}
      onOpen={open}
      refreshing={feedStatus() === "refreshing"}
      onRefresh={() => void reloadFeed("refreshing")}
      onLoadMore={() => void loadMoreFeed()}
      empty={<FeedEmpty onAddSource={() => nav.stack.push({ name: "addSource" })} onClearFilters={() => setFeedFilter(null)} />}
      testID="notify-list"
    />
  );
  return (
    <Page
      title={t("notify.title")}
      header={<PageHeader title={t("notify.title")} actions={more()} />}
      headerRight={more()}
      appBar={<AppBar title={t("notify.title")} actions={more()} />}
      root
    >
      <View class="flex-1 bg-surface" testID="notifications">
        <FeedFilters />
        <FeedSearch />
        {list()}
      </View>
    </Page>
  );
}
