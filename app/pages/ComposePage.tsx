// Writing a message: a new one, a reply (quoted) or a forward.
import { keymap, signal, Text, View } from "nsh";
import { NavPage } from "nsh/base";
import { t } from "nsh/i18n";
import { Mail } from "nsh-mail/native";
import { Button, SelectField, TextField, touch, systemHeader } from "../ui";
import { PageHeader } from "../components/PageHeader";
import { NavContext } from "../nav";
import { draftFor, isAddress, parseRecipient, splitRecipients, type ComposeInit } from "../lib/format";
import { accounts, errorText, mailbox, showToast } from "../store";

export function ComposePage(props: { init: ComposeInit }) {
  const nav = NavContext();
  const start = draftFor(props.init, accounts().map((a) => a.email));
  const from = signal(start.accountId ?? mailbox().account ?? accounts()[0]?.id ?? null);
  const to = signal(start.to);
  const cc = signal(start.cc);
  const bcc = signal("");
  const showCc = signal(!!start.cc);
  const subject = signal(start.subject);
  const body = signal(start.text);
  const sending = signal(false);
  const error = signal<string | null>(null);

  const title = () =>
    ({ new: t("compose.new"), reply: t("compose.reply"), replyAll: t("compose.reply"), forward: t("compose.forward") })[props.init.mode];

  const send = async () => {
    if (sending()) return;
    const recipients = [...splitRecipients(to()), ...splitRecipients(cc()), ...splitRecipients(bcc())];
    const bad = recipients.find((r) => !isAddress(r));
    const account = from();
    if (!account) return error.set(t("compose.noAccount"));
    if (!splitRecipients(to()).length) return error.set(t("compose.needRecipient"));
    if (bad) return error.set(t("compose.badAddress", { address: bad }));
    error.set(null);
    sending.set(true);
    try {
      await Mail.send({
        accountId: account,
        to: splitRecipients(to()).map(parseRecipient),
        cc: splitRecipients(cc()).map(parseRecipient),
        bcc: splitRecipients(bcc()).map(parseRecipient),
        subject: subject(),
        text: body(),
        inReplyTo: start.inReplyTo,
        references: start.references,
      });
      showToast(t("compose.sent"));
      nav.done();
    } catch (e) {
      error.set(errorText(e));
    } finally {
      sending.set(false);
    }
  };
  keymap.bind("mod+enter", () => void send(), { when: "always" });

  const actions = () => (
    <Button variant="primary" icon="send" label={sending() ? t("compose.sending") : t("compose.send")} loading={sending()} onPress={() => void send()} testID="send" />
  );
  const label = touch ? "w-16 text-sm" : "w-14 text-xs";
  const replying = props.init.mode === "reply" || props.init.mode === "replyAll";
  return (
    <NavPage title={title()} header={<PageHeader title={title()} actions={actions()} />} headerRight={systemHeader ? actions() : undefined}>
      <View class="min-h-0 flex-1 bg-background">
        <View class={`gap-2 border-b border-border ${touch ? "p-4" : "px-4 py-3"}`}>
          <If when={accounts().length > 1}>
            <View class="flex-row items-center gap-2">
              <Text class={`${label} text-muted-foreground`}>{t("compose.from")}</Text>
              <SelectField
                value={from()}
                items={accounts().map((a) => ({ value: a.id, title: a.name ? `${a.name} <${a.email}>` : a.email }))}
                onChange={(v) => from.set(v)}
                label={t("compose.from")}
                class="flex-1"
                testID="compose-from"
              />
            </View>
          </If>
          <View class="flex-row items-center gap-2">
            <Text class={`${label} text-muted-foreground`}>{t("compose.to")}</Text>
            <TextField
              value={to()}
              onChange={(v) => to.set(v)}
              kind="email"
              label={t("compose.to")}
              autoFocus={props.init.mode === "new" || props.init.mode === "forward"}
              invalid={!!error()}
              class="flex-1"
              testID="compose-to"
            />
            <If when={!showCc()}>
              <Button variant="ghost" size="sm" label={t("compose.ccBcc")} onPress={() => showCc.set(true)} />
            </If>
          </View>
          <If when={showCc()}>
            <View class="flex-row items-center gap-2">
              <Text class={`${label} text-muted-foreground`}>{t("compose.cc")}</Text>
              <TextField value={cc()} onChange={(v) => cc.set(v)} kind="email" label={t("compose.cc")} class="flex-1" testID="compose-cc" />
            </View>
            <View class="flex-row items-center gap-2">
              <Text class={`${label} text-muted-foreground`}>{t("compose.bcc")}</Text>
              <TextField value={bcc()} onChange={(v) => bcc.set(v)} kind="email" label={t("compose.bcc")} class="flex-1" testID="compose-bcc" />
            </View>
          </If>
          <View class="flex-row items-center gap-2">
            <Text class={`${label} text-muted-foreground`}>{t("compose.subject")}</Text>
            <TextField value={subject()} onChange={(v) => subject.set(v)} label={t("compose.subject")} class="flex-1" testID="compose-subject" />
          </View>
          <If when={error()}>
            <Text class={`text-destructive ${touch ? "text-sm" : "text-xs"}`} testID="compose-error">
              {error()}
            </Text>
          </If>
        </View>
        <View class={`min-h-0 flex-1 ${touch ? "p-4" : "p-3"}`}>
          <TextField
            value={body()}
            onChange={(v) => body.set(v)}
            multiline
            label={t("compose.body")}
            autoFocus={replying}
            ref={(input) => replying && setTimeout(() => input.setSelection(0), 0)}
            class="flex-1 border-0"
            testID="compose-body"
          />
        </View>
        <If when={!touch}>
          <View class="flex-row items-center justify-end gap-2 border-t border-border px-4 py-2">
            <Text class="flex-1 text-xs text-muted-foreground">{t("compose.sendHint", { keys: keymap.label("mod+enter") })}</Text>
            <Button variant="ghost" label={t("compose.discard")} onPress={nav.close} />
          </View>
        </If>
      </View>
    </NavPage>
  );
}
