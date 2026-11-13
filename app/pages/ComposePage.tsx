// Writing a message: a new one, a reply (quoted) or a forward. What is typed is
// a draft (../drafts.ts): closing the composer keeps it, Discard asks first.
import { keymap, Picker, platform, signal, Text, View } from "nsh";
import { AlertDialog } from "nsh/base";
import { t } from "nsh/i18n";
import { Mail } from "nsh-mail/native";
import { Button, Dialog, DialogBody, DialogFooter, IconButton, Page, SelectField, TextField, touch, systemHeader } from "../ui";
import { PageHeader } from "../components/PageHeader";
import { AttachmentChips } from "../components/Attachments";
import { onPageDrop, type Incoming } from "../lib/attachments";
import { NavContext } from "../nav";
import { endDraft, useDraft } from "../drafts";
import { isAddress, parseRecipient, splitRecipients, type ComposeInit } from "../lib/format";
import { accounts, errorText, mailbox, showToast } from "../store";

export function ComposePage(props: { init: ComposeInit }) {
  const nav = NavContext();
  const draft = useDraft(props.init, accounts(), mailbox().account ?? accounts()[0]?.id ?? null);
  const { start, from, to, cc, bcc, subject, files } = draft;
  const body = draft.text;
  const showCc = signal(!!cc() || !!bcc());
  const sending = signal(false);
  const error = signal<string | null>(null);
  const confirming = signal(false);
  const dragging = signal(false);
  const attach = (list: Incoming[]) => files.add(list);
  onPageDrop((on) => dragging.set(on), attach);
  const pick = async () => {
    try {
      attach((await Picker.pickFiles({ multiple: true })).map((p) => p.file));
    } catch (e) {
      files.setError(errorText(e));
    }
  };

  const discard = () => {
    confirming.set(false);
    endDraft(draft, "discarded");
    nav.close();
  };
  // Only an explicit Discard throws typed text away, and only after asking.
  const askDiscard = async () => {
    if (!draft.edited()) return nav.close();
    if (!systemHeader) return confirming.set(true);
    const { action } = await AlertDialog.show({
      title: t("compose.discardTitle"),
      description: t("compose.discardBody"),
      actions: [
        { id: "keep", label: t("compose.keepEditing"), role: "cancel" },
        { id: "discard", label: t("compose.discard"), role: "destructive" },
      ],
    });
    if (action === "discard") discard();
  };

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
    if (files.pending()) return files.setError(t("compose.stillUploading"));
    if (files.items().some((a) => a.failed)) return files.setError(t("compose.removeFailed"));
    if (files.overLimit()) return files.setError(files.limitText());
    files.setError(null);
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
        attachments: files.draft(),
      });
      endDraft(draft, "sent");
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
    <>
      <If when={touch}>
        <IconButton icon="trash" label={t("compose.discard")} onPress={() => void askDiscard()} testID="compose-discard" />
      </If>
      <IconButton icon="paperclip" label={t("compose.attach")} onPress={() => void pick()} testID="compose-attach" />
      <If
        when={systemHeader}
        else={<Button icon="send" label={sending() ? t("compose.sending") : t("compose.send")} loading={sending()} onPress={() => void send()} class="me-2" testID="send" />}
      >
        <IconButton icon="send" variant="filled" label={sending() ? t("compose.sending") : t("compose.send")} disabled={sending()} onPress={() => void send()} testID="send" />
      </If>
    </>
  );
  const label = "w-16 text-body-large tracking-body-large text-on-surface-variant";
  const line = "min-h-14 flex-row items-center gap-2 border-b border-outline-variant px-4";
  const replying = props.init.mode === "reply" || props.init.mode === "replyAll";
  return (
    <Page title={title()} header={<PageHeader title={title()} actions={actions()} />} headerRight={systemHeader ? actions() : undefined}>
      <View
        class={`min-h-0 flex-1 ${nav.modal ? "bg-surface-container-high" : "bg-surface"}`}
        onFileDragEnter={() => dragging.set(true)}
        onFileDragLeave={() => dragging.set(false)}
        onFileDrop={(e: { uris: string[]; mimeTypes: string[] }) => {
          dragging.set(false);
          attach(e.uris.map((path, i) => ({ kind: "path", path, type: e.mimeTypes[i] ?? "" })));
        }}
      >
        <View>
          <If when={accounts().length > 1}>
            <View class={`${line} py-1`}>
              <Text class={label}>{t("compose.from")}</Text>
              <SelectField
                value={from()}
                items={accounts().map((a) => ({ value: a.id, title: a.name ? `${a.name} <${a.email}>` : a.email }))}
                onChange={(v) => from.set(v)}
                label={t("compose.from")}
                floatingLabel={false}
                class="flex-1"
                testID="compose-from"
              />
            </View>
          </If>
          <View class={line}>
            <Text class={label}>{t("compose.to")}</Text>
            <TextField
              variant="plain"
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
              <Button variant="text" size="xs" label={t("compose.ccBcc")} onPress={() => showCc.set(true)} />
            </If>
          </View>
          <If when={showCc()}>
            <View class={line}>
              <Text class={label}>{t("compose.cc")}</Text>
              <TextField variant="plain" value={cc()} onChange={(v) => cc.set(v)} kind="email" label={t("compose.cc")} class="flex-1" testID="compose-cc" />
            </View>
            <View class={line}>
              <Text class={label}>{t("compose.bcc")}</Text>
              <TextField variant="plain" value={bcc()} onChange={(v) => bcc.set(v)} kind="email" label={t("compose.bcc")} class="flex-1" testID="compose-bcc" />
            </View>
          </If>
          <View class={line}>
            <Text class={label}>{t("compose.subject")}</Text>
            <TextField variant="plain" value={subject()} onChange={(v) => subject.set(v)} label={t("compose.subject")} class="flex-1" testID="compose-subject" />
          </View>
          <If when={error()}>
            <Text class="px-4 pt-2 text-body-medium tracking-body-medium text-error" testID="compose-error">
              {error()}
            </Text>
          </If>
        </View>
        <View class="min-h-0 flex-1 p-4">
          <TextField
            value={body()}
            onChange={(v) => body.set(v)}
            multiline
            label={t("compose.body")}
            autoFocus={replying}
            ref={(input) => replying && setTimeout(() => input.setSelection(0), 0)}
            variant="plain"
            class="flex-1"
            testID="compose-body"
            onPaste={(e) => {
              if (e.files.length) attach(e.files);
              else if (e.uris.length && platform.os !== "web") attach(e.uris.map((path) => ({ kind: "path", path, type: "" })));
            }}
          />
        </View>
        <AttachmentChips files={files} error={files.error()} />
        <If when={systemHeader}>
          <View class="pb-[env(safe-area-inset-bottom)]" />
        </If>
        <If when={!touch}>
          <View class="flex-row items-center justify-end gap-2 px-4 py-3">
            <Text class="flex-1 text-body-small tracking-body-small text-on-surface-variant">{t("compose.sendHint", { keys: keymap.label("mod+enter") })}</Text>
            <Button variant="text" label={t("compose.discard")} onPress={() => void askDiscard()} testID="compose-discard" />
          </View>
        </If>
        <If when={dragging()}>
          <View class="absolute inset-2 items-center justify-center rounded-extra-large border-2 border-dashed border-primary bg-primary-container/90" style={{ pointerEvents: "none" }} testID="compose-drop">
            <Text class="text-title-medium font-medium text-on-primary-container">{t("compose.dropHere")}</Text>
          </View>
        </If>
      </View>
      <Dialog open={confirming()} onClose={() => confirming.set(false)} label={t("compose.discardTitle")} title={t("compose.discardTitle")} size="sm" testID="discard-confirm">
        <DialogBody>
          <Text class="text-body-medium tracking-body-medium text-on-surface-variant">{t("compose.discardBody")}</Text>
        </DialogBody>
        <DialogFooter>
          <Button variant="text" label={t("compose.keepEditing")} onPress={() => confirming.set(false)} testID="keep-editing" />
          <Button variant="text" label={t("compose.discard")} onPress={discard} testID="confirm-discard" />
        </DialogFooter>
      </Dialog>
    </Page>
  );
}
