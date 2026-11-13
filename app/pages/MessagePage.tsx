// A message on its own page (phones and narrow windows).
import { t } from "nsh/i18n";
import { Empty, Page, Spinner, systemHeader } from "../ui";
import { PageHeader } from "../components/PageHeader";
import { PhoneReaderActions, Reader, ReaderActions } from "../components/Reader";
import { NavContext } from "../nav";
import { current } from "../store";
import type { ComposeMode } from "../lib/format";

export function MessagePage(props: { id: string }) {
  const nav = NavContext();
  const message = () => (current()?.id === props.id ? current() : undefined);
  const compose = (mode: ComposeMode) => nav.stack.push({ name: "compose", init: { mode, source: message() } });
  return (
    <Page
      title=""
      header={
        <PageHeader
          title=""
          actions={<If when={message()}>{(m) => <ReaderActions message={m()} onCompose={compose} />}</If>}
        />
      }
      headerRight={<If when={message()}>{(m) => <PhoneReaderActions message={m()} onLeave={nav.close} />}</If>}
    >
      <If
        when={message()}
        else={
          <If when={current.error()} else={<Empty icon="mail" title={t("reader.loading")} action={<Spinner label={t("reader.loading")} />} />}>
            <Empty icon="mail" title={t("reader.failed")} />
          </If>
        }
      >
        {(m) => <Reader message={m()} onCompose={compose} toolbar={!systemHeader} phone={systemHeader} onLeave={nav.close} />}
      </If>
    </Page>
  );
}
