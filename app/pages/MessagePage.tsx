// A message on its own page (phones and narrow windows).
import { t } from "nsh/i18n";
import { Button, Empty, LoadingIndicator, Page, systemHeader } from "../ui";
import { PageHeader } from "../components/PageHeader";
import { PhoneReaderActions, Reader } from "../components/Reader";
import { NavContext } from "../nav";
import { conversation, current, shownMessage } from "../store";
import type { ComposeMode } from "../lib/format";

// `id` is the message first opened; in a conversation the reader may expand another.
export function MessagePage(props: { id: string }) {
  const nav = NavContext();
  const message = () => shownMessage();
  const compose = (mode: ComposeMode) => nav.stack.push({ name: "compose", init: { mode, source: message() } });
  return (
    <Page
      title=""
      header={<PageHeader title="" />}
      headerRight={<If when={message()}>{(m) => <PhoneReaderActions message={m()} conversation={conversation()} onLeave={nav.close} />}</If>}
    >
      <If
        when={message()}
        else={
          <If
            when={current.error()}
            else={<Empty icon="mail" title={t("reader.loading")} action={<LoadingIndicator label={t("reader.loading")} contained />} />}
          >
            <Empty
              icon="alert"
              title={t("reader.failed")}
              description={t("reader.failedHint")}
              action={<Button variant="tonal" icon="refresh" label={t("actions.retry")} onPress={() => current.refetch()} testID="reader-retry" />}
            />
          </If>
        }
      >
        {(m) => <Reader message={m()} conversation={conversation()} onCompose={compose} toolbar={!systemHeader} phone={systemHeader} onLeave={nav.close} />}
      </If>
    </Page>
  );
}
