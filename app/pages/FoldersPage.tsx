// The Mailboxes tab (iOS and Android phones): every inbox, each account's, and
// the folders; choosing one shows it in the Mail tab.
import { t } from "nsh/i18n";
import { Page } from "../ui";
import { Mailboxes } from "../components/Mailboxes";
import { PageHeader } from "../components/PageHeader";
import { selectTab, tabs } from "../nav";

export function FoldersPage() {
  return (
    <Page title={t("tabs.folders")} header={<PageHeader title={t("tabs.folders")} />} root>
      <Mailboxes
        onChosen={() => {
          tabs[0].stack.popToRoot();
          selectTab(0);
        }}
      />
    </Page>
  );
}
