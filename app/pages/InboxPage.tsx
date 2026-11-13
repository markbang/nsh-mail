// The phone's first page: the chosen mailbox. iOS and Android reach search,
// the mailboxes and compose from the tab bar and its button; a narrow browser
// from the header, the mailbox list in a sheet.
import { signal, View } from "nsh";
import { t } from "nsh/i18n";
import { IconButton, Page, Sheet, systemHeader } from "../ui";
import { MailboxList, mailboxTitle } from "../components/Mailbox";
import { Mailboxes } from "../components/Mailboxes";
import { PageHeader } from "../components/PageHeader";
import { NavContext } from "../nav";
import { openMessage } from "../store";

export function InboxPage() {
  const nav = NavContext();
  const sheet = signal(false);
  const open = (id: string) => {
    openMessage(id);
    nav.stack.push({ name: "message", id });
  };
  const menu = () => <IconButton icon="menu" label={t("mailbox.choose")} onPress={() => sheet.set(true)} testID="mailboxes" />;
  const actions = () => (
    <>
      <IconButton icon="search" label={t("search.label")} onPress={() => nav.stack.push({ name: "search" })} testID="open-search" />
      <IconButton icon="compose" label={t("actions.compose")} onPress={() => nav.stack.push({ name: "compose", init: { mode: "new" } })} testID="compose" />
    </>
  );
  return (
    <Page title={mailboxTitle()} header={<PageHeader title={mailboxTitle()} leading={menu()} actions={actions()} />} root>
      <View class="flex-1 bg-background">
        <MailboxList onOpen={open} onAddAccount={() => nav.stack.push({ name: "addAccount" })} searchable={false} />
      </View>
      <If when={!systemHeader}>
        <Sheet open={sheet()} onClose={() => sheet.set(false)} side="left" label={t("mailbox.choose")}>
          <Mailboxes
            onSettings={() => {
              sheet.set(false);
              nav.stack.push({ name: "settings" });
            }}
            onChosen={() => sheet.set(false)}
          />
        </Sheet>
      </If>
    </Page>
  );
}
