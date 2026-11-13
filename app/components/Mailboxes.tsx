// The mailboxes as navigation: every inbox together, each account's inbox,
// and the other folders of the chosen account (or of all of them). Wide
// windows show them in the navigation rail; phones in the Mailboxes tab.
import { Text, View, type JSX } from "nsh";
import { t } from "nsh/i18n";
import type { Folder } from "nsh-mail/native";
import { AccountMark, IconButton, LoadingIndicator, NavigationRail, type IconName, type RailItem } from "../ui";
import { accountName, accounts, mailbox, mailStatus, openMailbox, refresh, unreadCount } from "../store";

const folders: { folder: Folder; icon: IconName }[] = [
  { folder: "sent", icon: "send" },
  { folder: "drafts", icon: "drafts" },
  { folder: "archive", icon: "archive" },
  { folder: "trash", icon: "trash" },
];

const keyOf = (account: string | null, folder: Folder) => `${account ?? "*"}/${folder}`;

function items(): RailItem[] {
  return [
    { key: keyOf(null, "inbox"), label: t("mailbox.allInboxes"), icon: "inboxes", badge: unreadCount(null), testID: "mailbox-all" },
    ...accounts().map((a, i) => ({
      key: keyOf(a.id, "inbox"),
      label: accountName(a.id) ?? a.email,
      tooltip: a.email,
      lead: <AccountMark index={i} size="md" />,
      badge: unreadCount(a.id),
    })),
    ...folders.map((f, i) => ({
      key: keyOf(mailbox().account, f.folder),
      label: t(`folder.${f.folder}`),
      icon: f.icon,
      section: i === 0 ? t("mailbox.folders") : undefined,
    })),
  ];
}

function choose(key: string) {
  const [account, folder] = key.split("/");
  openMailbox({ account: account === "*" ? null : account, folder: folder as Folder });
}

/** Sync status, refresh and settings: the rail's footer. */
function Status(props: { expanded: boolean; onSettings?: () => void }) {
  return (
    <View class={`items-center gap-1 ${props.expanded ? "flex-row ps-4" : ""}`}>
      <If when={props.expanded}>
        <View class="min-w-0 flex-1 flex-row items-center gap-2">
          <If when={mailStatus().syncing} else={<Text class="line-clamp-1 text-label-medium tracking-label-medium text-on-surface-variant">{mailStatus().error ? t("sync.failed") : t(`mode.${mailStatus().mode}`)}</Text>}>
            <LoadingIndicator label={t("sync.running")} size={24} />
            <Text class="line-clamp-1 text-label-medium tracking-label-medium text-on-surface-variant">{t("sync.running")}</Text>
          </If>
        </View>
      </If>
      <If when={!props.expanded && mailStatus().syncing} else={<IconButton icon="refresh" label={t("actions.refresh")} onPress={() => void refresh()} testID="refresh-all" />}>
        <LoadingIndicator label={t("sync.running")} size={40} />
      </If>
      <If when={props.onSettings}>
        <IconButton icon="settings" label={t("settings.title")} onPress={() => props.onSettings?.()} testID="open-settings" />
      </If>
    </View>
  );
}

/** The navigation rail of wide windows: the FAB on top, the mailboxes, the status at the foot. */
export function MailboxRail(props: { expanded: boolean; fab: JSX.Element; onSettings: () => void }) {
  return (
    <NavigationRail
      label={t("mailbox.choose")}
      expanded={props.expanded}
      items={items()}
      selected={keyOf(mailbox().account, mailbox().folder)}
      onSelect={choose}
      header={props.fab}
      footer={<Status expanded={props.expanded} onSettings={props.onSettings} />}
      testID="mailbox-rail"
    />
  );
}

/** The mailboxes as a page's list (phones), with the status at the foot. */
export function Mailboxes(props: { onChosen?: () => void; onSettings?: () => void }) {
  return (
    <NavigationRail
      label={t("mailbox.choose")}
      expanded
      items={items()}
      selected={keyOf(mailbox().account, mailbox().folder)}
      onSelect={(key) => {
        choose(key);
        props.onChosen?.();
      }}
      footer={<Status expanded onSettings={props.onSettings} />}
    />
  );
}
