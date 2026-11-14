// One notification on a phone: its source, sender, tags, body and actions.
import { t } from "nsh/i18n";
import { Empty, Page } from "../ui";
import { NotificationReader } from "../components/NotificationReader";
import { PageHeader } from "../components/PageHeader";
import { phoneStack, selectTab, TAB } from "../nav";
import { archiveNotifications, markNotifications, notifications, sourceById } from "../notify";
import { openMessage } from "../store";

export function NotificationPage(props: { id: string }) {
  const note = () => notifications().find((n) => n.id === props.id);
  const source = () => sourceById(note()?.sourceId);
  const openMail = () => {
    openMessage(props.id);
    selectTab(TAB.mail);
    phoneStack.setRoutes([{ name: "inbox" }, { name: "message", id: props.id }]);
  };
  return (
    <Page title={source()?.name ?? t("notify.title")} header={<PageHeader title={source()?.name ?? t("notify.title")} />}>
      <If
        when={note()}
        else={
          <Empty icon="bell" title={t("notify.gone")} description={t("notify.goneHint")} />
        }
      >
        {(n) => (
          <NotificationReader
            note={n()}
            source={source()}
            onToggleRead={() => void markNotifications([n().id], !n().read)}
            onArchive={() => {
              archiveNotifications([n().id]);
              phoneStack.pop();
            }}
            onOpenMail={n().threadId ? openMail : undefined}
          />
        )}
      </If>
    </Page>
  );
}
