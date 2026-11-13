// Writing a message: a new one, a reply (quoted), a forward, or one taken out of
// the outbox to edit. What is typed is a draft (../drafts.ts): closing the
// composer keeps it (an edited outbox message goes back to the outbox), Discard
// asks first. Send waits out the undo delay with Undo; the split button's menu
// schedules it (../outbox.ts).
import { cleanup, keymap, Picker, platform, signal, Text, View } from "nsh";
import { AlertDialog } from "nsh/base";
import { t } from "nsh/i18n";
import { Mail, type Draft as MailDraft, type Outgoing } from "nsh-mail/native";
import { Button, Dialog, DialogBody, DialogFooter, IconButton, Menu, Page, SelectField, SplitButton, TextField, touch, systemHeader, type MenuAction } from "../ui";
import { PageHeader } from "../components/PageHeader";
import { AttachmentChips } from "../components/Attachments";
import { onPageDrop, type Incoming } from "../lib/attachments";
import { ScheduleDialog } from "../components/ScheduleDialog";
import { NavContext, show } from "../nav";
import { endDraft, holdDraft, isEnded, useDraft } from "../drafts";
import { isAddress, parseRecipient, sendDate, splitRecipients, type ComposeInit } from "../lib/format";
import { wide } from "../layout";
import { queueDraft, schedulePresets, undoDelay } from "../outbox";
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
  const picking = signal(false);
  const dragging = signal(false);
  const editing = props.init.mode === "edit" ? props.init.outgoing : undefined;
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
    if (!draft.edited() && !editing) return nav.close();
    if (!systemHeader) return confirming.set(true);
    const { action } = await AlertDialog.show({
      title: editing ? t("compose.deleteScheduledTitle") : t("compose.discardTitle"),
      description: editing ? t("compose.deleteScheduledBody") : t("compose.discardBody"),
      actions: [
        { id: "keep", label: t("compose.keepEditing"), role: "cancel" },
        { id: "discard", label: t("compose.discard"), role: "destructive" },
      ],
    });
    if (action === "discard") discard();
  };

  const title = () =>
    ({ new: t("compose.new"), reply: t("compose.reply"), replyAll: t("compose.reply"), forward: t("compose.forward"), edit: t("compose.editTitle") })[props.init.mode];

  const message = (): MailDraft => ({
    accountId: from() ?? "",
    to: splitRecipients(to()).map(parseRecipient),
    cc: splitRecipients(cc()).map(parseRecipient),
    bcc: splitRecipients(bcc()).map(parseRecipient),
    subject: subject(),
    text: body(),
    inReplyTo: start.inReplyTo,
    references: start.references,
    attachments: files.draft(),
  });
  /** The message to send, or null with the reason shown. */
  const ready = (): MailDraft | null => {
    const recipients = [...splitRecipients(to()), ...splitRecipients(cc()), ...splitRecipients(bcc())];
    const bad = recipients.find((r) => !isAddress(r));
    if (!from()) return error.set(t("compose.noAccount")), null;
    if (!splitRecipients(to()).length) return error.set(t("compose.needRecipient")), null;
    if (bad) return error.set(t("compose.badAddress", { address: bad })), null;
    error.set(null);
    if (files.pending()) return files.setError(t("compose.stillUploading")), null;
    if (files.items().some((a) => a.failed)) return files.setError(t("compose.removeFailed")), null;
    if (files.overLimit()) return files.setError(files.limitText()), null;
    files.setError(null);
    return message();
  };

  // Undo brings this draft back as it was, in a composer of its own context.
  const reopen = (hold: ReturnType<typeof holdDraft>, back: Outgoing) => {
    hold.restore();
    // A forward's files came back as uploads; the draft still names the originals.
    const mine = new Set(files.draft().flatMap((a) => (a.kind === "upload" ? [a.id] : [])));
    for (const f of back.files) if (!mine.has(f.id)) void Mail.unstage(f.id).catch(() => {});
    show({ name: "compose", init: props.init }, wide());
  };

  /** Sends now (no undo delay), after the undo delay (`at` null), or at `at`. */
  const go = async (at: number | null) => {
    if (sending()) return;
    const d = ready();
    if (!d) return;
    sending.set(true);
    try {
      if (at === null && undoDelay() === 0) {
        await Mail.send(d);
        endDraft(draft, "sent");
        showToast(t("compose.sent"));
      } else {
        let hold: ReturnType<typeof holdDraft> | undefined;
        await queueDraft(d, at, { onUndo: (back) => hold && reopen(hold, back), onKept: () => hold?.release() });
        hold = holdDraft(draft);
      }
      nav.done();
    } catch (e) {
      error.set(errorText(e));
    } finally {
      sending.set(false);
    }
  };
  const send = () => go(null);
  keymap.bind("mod+enter", () => void send(), { when: "always" });

  // An outbox message being edited goes back when the composer closes without Send or Discard.
  if (editing) {
    cleanup(() => {
      if (isEnded(draft)) return;
      endDraft(draft, "sent");
      Mail.queue(message(), { sendAt: Math.max(editing.sendAt, Date.now()) }).catch((e) => showToast(t("compose.notKept", { detail: errorText(e) }), "error"));
    });
  }

  const scheduleActions = (): MenuAction[] => [
    ...schedulePresets().map((p) => ({ id: p.id, label: t(`schedule.${p.id}`), description: sendDate(p.at), icon: "clock" as const, onSelect: () => void go(p.at) })),
    { id: "pick", label: t("schedule.pick"), icon: "calendar", separatorBefore: true, onSelect: () => picking.set(true) },
  ];

  // Pointer devices send from the composer's foot, as mail apps do there; touch screens from its top bar.
  const sendButton = (align: "start" | "end", cls?: string) => (
    <SplitButton
      icon="send"
      label={sending() ? t("compose.sending") : t("compose.send")}
      loading={sending()}
      onPress={() => void send()}
      menuLabel={t("compose.schedule")}
      actions={scheduleActions()}
      menuAlign={align}
      class={cls}
      testID="send"
      menuTestID="send-later"
    />
  );
  const actions = () => (
    <>
      <If when={touch}>
        <IconButton icon="trash" label={t("compose.discard")} onPress={() => void askDiscard()} testID="compose-discard" />
      </If>
      <IconButton icon="paperclip" label={t("compose.attach")} onPress={() => void pick()} testID="compose-attach" />
      <If when={systemHeader} else={<If when={touch}>{sendButton("end", "me-2")}</If>}>
        <Menu label={t("compose.schedule")} actions={scheduleActions()} trigger={<IconButton icon="clock" label={t("compose.schedule")} disabled={sending()} testID="send-later" />} />
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
          <If when={editing}>
            {(o) => (
              <Text class={`px-4 pt-2 text-body-medium tracking-body-medium ${o().error ? "text-error" : "text-on-surface-variant"}`} testID="compose-scheduled">
                {o().error ? t("compose.editFailed", { detail: o().error!.message }) : t("compose.editScheduled", { when: sendDate(o().sendAt) })}
              </Text>
            )}
          </If>
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
          <View class="flex-row items-center gap-3 px-4 py-3">
            {sendButton("start")}
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
      <ScheduleDialog open={picking()} onClose={() => picking.set(false)} onPick={(at) => (picking.set(false), void go(at))} />
      <Dialog
        open={confirming()}
        onClose={() => confirming.set(false)}
        label={editing ? t("compose.deleteScheduledTitle") : t("compose.discardTitle")}
        title={editing ? t("compose.deleteScheduledTitle") : t("compose.discardTitle")}
        size="sm"
        testID="discard-confirm"
      >
        <DialogBody>
          <Text class="text-body-medium tracking-body-medium text-on-surface-variant">{editing ? t("compose.deleteScheduledBody") : t("compose.discardBody")}</Text>
        </DialogBody>
        <DialogFooter>
          <Button variant="text" label={t("compose.keepEditing")} onPress={() => confirming.set(false)} testID="keep-editing" />
          <Button variant="text" label={t("compose.discard")} onPress={discard} testID="confirm-discard" />
        </DialogFooter>
      </Dialog>
    </Page>
  );
}
