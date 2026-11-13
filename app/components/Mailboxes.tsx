// The mailbox list: every inbox together, each account's inbox, and the
// other folders of the chosen account (or of all of them).
import { Scroll, Text, View } from "nsh";
import { t } from "nsh/i18n";
import type { Folder } from "nsh-mail/native";
import { AccountMark, Badge, Button, IconButton, Listbox, NavItem, SectionLabel, Spinner, touch, type IconName } from "../ui";
import { accountName, accounts, mailbox, mailStatus, openMailbox, refresh, unreadCount } from "../store";

const folders: { folder: Folder; icon: IconName }[] = [
  { folder: "sent", icon: "send" },
  { folder: "drafts", icon: "drafts" },
  { folder: "archive", icon: "archive" },
  { folder: "trash", icon: "trash" },
];

export function Mailboxes(props: { onCompose?: () => void; onSettings?: () => void; onChosen?: () => void }) {
  const choose = (account: string | null, folder: Folder) => {
    openMailbox({ account, folder });
    props.onChosen?.();
  };
  const is = (account: string | null, folder: Folder) => mailbox().account === account && mailbox().folder === folder;
  const count = (account: string | null) => {
    const n = unreadCount(account);
    return n ? <Badge label={String(n)} color="primary" variant="flat" /> : undefined;
  };
  return (
    <View class={`flex-1 ${props.onSettings ? "bg-sidebar" : "bg-background"}`}>
      <If when={props.onCompose}>
        <View class="px-3 pb-1 pt-3">
          <Button color="primary" icon="compose" label={t("actions.compose")} onPress={() => props.onCompose?.()} fullWidth testID="compose" />
        </View>
      </If>
      <Scroll class="flex-1" contentClass="p-2">
        <Listbox label={t("mailbox.choose")} class="p-0">
        <NavItem
          icon="inboxes"
          label={t("mailbox.allInboxes")}
          end={count(null)}
          selected={is(null, "inbox")}
          onPress={() => choose(null, "inbox")}
          testID="mailbox-all"
        />
        <Each of={accounts()} key={(a) => a.id}>
          {(account, index) => (
            <NavItem
              lead={<AccountMark index={index()} size="md" />}
              label={accountName(account().id) ?? account().email}
              tooltip={account().email}
              end={count(account().id)}
              selected={is(account().id, "inbox")}
              onPress={() => choose(account().id, "inbox")}
            />
          )}
        </Each>
        <SectionLabel label={t("mailbox.folders")} />
        <Each of={folders} key={(f) => f.folder}>
          {(f) => (
            <NavItem
              icon={f().icon}
              label={t(`folder.${f().folder}`)}
              selected={is(mailbox().account, f().folder)}
              onPress={() => choose(mailbox().account, f().folder)}
            />
          )}
        </Each>
        </Listbox>
      </Scroll>
      <View class={`flex-row items-center gap-1 border-t border-divider px-2 ${touch ? "h-14" : "h-11"}`}>
        <View class="flex-1 flex-row items-center gap-2 px-1">
          <If when={mailStatus().syncing} else={<Text class="line-clamp-1 text-tiny text-foreground-500">{mailStatus().error ? t("sync.failed") : t(`mode.${mailStatus().mode}`)}</Text>}>
            <Spinner label={t("sync.running")} size="sm" />
            <Text class="text-tiny text-foreground-500">{t("sync.running")}</Text>
          </If>
        </View>
        <IconButton icon="refresh" label={t("actions.refresh")} size="sm" onPress={() => void refresh()} />
        <If when={props.onSettings}>
          <IconButton icon="settings" label={t("settings.title")} size="sm" onPress={() => props.onSettings?.()} testID="open-settings" />
        </If>
      </View>
    </View>
  );
}
