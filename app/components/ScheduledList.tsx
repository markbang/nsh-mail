// The Scheduled mailbox: messages waiting in the outbox for their time, and
// those that could not be sent. A row opens the message to edit (closing the
// composer puts it back); Cancel send takes it out, with Undo.
import { Scroll, Text, View } from "nsh";
import { t } from "nsh/i18n";
import { Mail, type Outgoing } from "nsh-mail/native";
import { AccountMark, Empty, Icon, IconButton, LoadingIndicator, Row, touch } from "../ui";
import { displayName, sendDate } from "../lib/format";
import { wide } from "../layout";
import { show } from "../nav";
import { cancelScheduled, loadOutbox, outboxLoaded, waiting } from "../outbox";
import { accountIndex, errorText, mailbox, mailStatus, showToast } from "../store";

async function edit(o: Outgoing) {
  try {
    const back = await Mail.cancel(o.id);
    show({ name: "compose", init: { mode: "edit", outgoing: back } }, wide());
  } catch (e) {
    showToast(errorText(e), "error");
    void loadOutbox();
  }
}

function ScheduledRow(props: { outgoing: Outgoing; showAccount: boolean }) {
  const o = () => props.outgoing;
  const to = () => o().draft.to.map(displayName).join(", ");
  const subject = () => o().draft.subject || t("reader.noSubject");
  const preview = () => o().draft.text.replace(/\s+/g, " ").trim();
  const when = () => sendDate(o().sendAt);
  return (
    <Row
      onPress={() => void edit(o())}
      accessibilityLabel={[to(), subject(), o().error ? t("outbox.notSent", { detail: o().error!.message }) : t("outbox.sends", { when: when() })].join(", ")}
      class={`mb-0.5 flex-row items-center pe-2 ${touch ? "py-2" : "py-2.5"}`}
      testID="scheduled-row"
    >
      <If when={props.showAccount}>
        <AccountMark index={accountIndex(o().draft.accountId)} shape="bar" />
      </If>
      <View class="w-6 items-center self-start pt-1">
        <If when={o().error}>
          <Icon name="alert" size="sm" tone="error" />
        </If>
      </View>
      <View class="min-w-0 flex-1 gap-0.5">
        <View class="flex-row items-center gap-1.5">
          <Text class="line-clamp-1 flex-1 text-body-large tracking-body-large text-on-surface">{to()}</Text>
          <Text class={`text-label-medium tracking-label-medium tabular-nums ${o().error ? "text-error" : "text-primary"}`}>{when()}</Text>
        </View>
        <Text class="line-clamp-1 text-body-medium tracking-body-medium text-on-surface">{subject()}</Text>
        <If
          when={o().error}
          else={
            <If when={preview()}>
              <Text class="line-clamp-1 text-body-medium tracking-body-medium text-on-surface-variant">{preview()}</Text>
            </If>
          }
        >
          {(e) => <Text class="line-clamp-2 text-body-medium tracking-body-medium text-error">{t("outbox.notSent", { detail: e().message })}</Text>}
        </If>
      </View>
      <IconButton icon="close" label={t("outbox.cancel")} onPress={() => void cancelScheduled(o().id)} testID="scheduled-cancel" />
    </Row>
  );
}

export function ScheduledList() {
  void loadOutbox();
  const items = () => waiting(mailbox().account);
  return (
    <If
      when={items().length}
      else={
        <If
          when={outboxLoaded()}
          else={
            <View class="flex-1 items-center justify-center p-8">
              <LoadingIndicator label={t("reader.loading")} contained />
            </View>
          }
        >
          <Empty icon="clock" title={t("outbox.empty")} description={mailStatus().mode === "local" ? t("outbox.localHint") : t("outbox.emptyHint")} />
        </If>
      }
    >
      <Scroll class="flex-1" contentClass={`gap-0.5 ${touch ? "pb-24" : "px-2 pb-2"}`} testID="scheduled">
        <Each of={items()} key={(o) => o.id}>
          {(o) => <ScheduledRow outgoing={o()} showAccount={mailbox().account === null} />}
        </Each>
        <If when={mailStatus().mode === "local"}>
          <Text class="px-4 py-3 text-body-small tracking-body-small text-on-surface-variant">{t("outbox.localHint")}</Text>
        </If>
      </Scroll>
    </If>
  );
}
