// One message: who and when, the actions on it, the body (its HTML in a
// web view, or its text) and its attachments. Remote images stay blocked
// until the reader asks for them.
import { createMotion, reduceMotion, Scroll, signal, Text, View } from "nsh";
import { t } from "nsh/i18n";
import { Mail, type Attachment, type Message } from "nsh-mail/native";
import { AccountMark, Avatar, Button, FileTile, FloatingToolbar, Icon, IconButton, Divider, material, Menu, ToolbarButton, touch } from "../ui";
import { displayName, fileSize, fullDate, type ComposeMode } from "../lib/format";
import { accountById, accountIndex, accounts, errorText, moveMessages, setFlags, showToast } from "../store";
import { MailView, webViewAvailable } from "./MailView";

export interface ReaderProps {
  message: Message;
  onCompose: (mode: ComposeMode) => void;
  /** Wide layouts put the actions in the reader's own toolbar. */
  toolbar?: boolean;
  /** Phones: the actions are in the system header and at the bottom (Android's floating toolbar). */
  phone?: boolean;
  /** Leaves the page (a phone, after archiving or deleting). */
  onLeave?: () => void;
}

// Per message: the reader chose to load its images.
const imagesShown = signal<Record<string, boolean>>({});

/** Archive or delete the message: `before` runs first (a phone leaves the page). */
function move(m: Message, folder: "archive" | "trash", before?: () => void) {
  before?.();
  moveMessages([m.id], folder);
}

/** The phone's header actions: archive, delete, unread, and the rest under More. */
export function PhoneReaderActions(props: { message: Message; onLeave: () => void }) {
  const m = () => props.message;
  return (
    <>
      {/* Android has them in the floating toolbar. */}
      <If when={!material && m().folder !== "archive" && m().folder !== "trash"}>
        <IconButton icon="archive" label={t("actions.archive")} onPress={() => move(m(), "archive", props.onLeave)} testID="archive" />
      </If>
      <If when={!material && m().folder !== "trash"}>
        <IconButton icon="trash" label={t("actions.delete")} onPress={() => move(m(), "trash", props.onLeave)} testID="delete" />
      </If>
      <IconButton
        icon="mail"
        label={t("actions.markUnread")}
        onPress={() => {
          void setFlags(m().id, { seen: false });
          props.onLeave();
        }}
        testID="toggle-seen"
      />
      <Menu
        label={t("actions.more")}
        trigger={<IconButton icon="more" label={t("actions.more")} testID="reader-more" />}
        actions={[
          {
            id: "flag",
            label: m().flagged ? t("actions.unflag") : t("actions.flag"),
            icon: m().flagged ? "flagFilled" : "flag",
            onSelect: () => void setFlags(m().id, { flagged: !m().flagged }),
          },
        ]}
      />
    </>
  );
}

export function ReaderActions(props: { message: Message; onCompose: (mode: ComposeMode) => void }) {
  const m = () => props.message;
  return (
    <View class="flex-row items-center gap-1">
      <IconButton size={touch ? "md" : "sm"} icon="reply" label={t("actions.reply")} shortcut="R" onPress={() => props.onCompose("reply")} testID="reply" />
      <IconButton size={touch ? "md" : "sm"} icon="replyAll" label={t("actions.replyAll")} onPress={() => props.onCompose("replyAll")} testID="reply-all" />
      <IconButton size={touch ? "md" : "sm"} icon="forward" label={t("actions.forward")} onPress={() => props.onCompose("forward")} testID="forward" />
      <Divider orientation="vertical" class="mx-1 !h-5" />
      <IconButton
        size={touch ? "md" : "sm"}
        icon={m().seen ? "mail" : "mailOpen"}
        label={m().seen ? t("actions.markUnread") : t("actions.markRead")}
        onPress={() => void setFlags(m().id, { seen: !m().seen })}
        testID="toggle-seen"
      />
      <IconButton
        size={touch ? "md" : "sm"}
        icon={m().flagged ? "flagFilled" : "flag"}
        tone={m().flagged ? "flag" : undefined}
        selected={m().flagged}
        label={m().flagged ? t("actions.unflag") : t("actions.flag")}
        onPress={() => void setFlags(m().id, { flagged: !m().flagged })}
        testID="toggle-flag"
      />
      <If when={m().folder !== "archive" && m().folder !== "trash"}>
        <IconButton size={touch ? "md" : "sm"} icon="archive" label={t("actions.archive")} shortcut="E" onPress={() => move(m(), "archive")} testID="archive" />
      </If>
      <If when={m().folder !== "trash"}>
        <IconButton size={touch ? "md" : "sm"} icon="trash" label={t("actions.delete")} shortcut="#" onPress={() => move(m(), "trash")} testID="delete" />
      </If>
    </View>
  );
}

/** Android: M3E's floating toolbar, vibrant, over the end of the message: reply first. */
function ReaderToolbar(props: { message: Message; onCompose: (mode: ComposeMode) => void; onLeave?: () => void }) {
  const m = () => props.message;
  return (
    <View class="absolute inset-x-0 bottom-0 items-center pb-[env(safe-area-inset-bottom)]" style={{ pointerEvents: "box-none" }}>
      <View class="mb-4">
        <FloatingToolbar label={t("reader.actions")} vibrant testID="reader-toolbar">
          <ToolbarButton icon="reply" label={t("actions.reply")} emphasized vibrant onPress={() => props.onCompose("reply")} testID="reply" />
          <ToolbarButton icon="replyAll" label={t("actions.replyAll")} vibrant onPress={() => props.onCompose("replyAll")} testID="reply-all" />
          <ToolbarButton icon="forward" label={t("actions.forward")} vibrant onPress={() => props.onCompose("forward")} testID="forward" />
          <If when={m().folder !== "archive" && m().folder !== "trash"}>
            <ToolbarButton icon="archive" label={t("actions.archive")} vibrant onPress={() => move(m(), "archive", props.onLeave)} testID="archive" />
          </If>
          <If when={m().folder !== "trash"}>
            <ToolbarButton icon="trash" label={t("actions.delete")} vibrant onPress={() => move(m(), "trash", props.onLeave)} testID="delete" />
          </If>
        </FloatingToolbar>
      </View>
    </View>
  );
}

/** Replying on a phone: a bar at the bottom of the message, over the gesture area. */
function ReplyBar(props: { onCompose: (mode: ComposeMode) => void }) {
  return (
    <View class="border-t border-outline-variant bg-surface pb-[env(safe-area-inset-bottom)]">
      <View class="flex-row gap-2 px-4 py-3">
        <Button variant="bordered" radius="full" icon="reply" label={t("actions.reply")} onPress={() => props.onCompose("reply")} class="flex-1" testID="reply" />
        <Button variant="bordered" radius="full" icon="replyAll" label={t("actions.replyAll")} onPress={() => props.onCompose("replyAll")} class="flex-1" testID="reply-all" />
        <Button variant="bordered" radius="full" icon="forward" label={t("actions.forward")} onPress={() => props.onCompose("forward")} class="flex-1" testID="forward" />
      </View>
    </View>
  );
}


export function Reader(props: ReaderProps) {
  const m = () => props.message;
  const enter = createMotion({ enter: reduceMotion() ? undefined : "fadeIn", duration: 150 });
  const account = () => accountById(m().accountId);
  const recipients = () => [...m().to, ...m().cc].map(displayName).join(", ");
  const showImages = () => !!imagesShown()[m().id];
  const html = () => (webViewAvailable() ? m().html : null);
  const pad = touch ? "px-4" : "px-6";
  const header = () => (
    <View class="gap-3">
      <View class="flex-row items-start gap-2">
        <Text role="header" class={`flex-1 ${material ? "text-headline-small font-medium text-on-surface" : `font-semibold text-on-surface ${touch ? "text-xl" : "text-title-large"}`}`}>
          {m().subject || t("reader.noSubject")}
        </Text>
        <If when={props.phone}>
          <IconButton
            icon={m().flagged ? "flagFilled" : "flag"}
            tone={m().flagged ? "flag" : "on-surface-variant"}
            selected={m().flagged}
            label={m().flagged ? t("actions.unflag") : t("actions.flag")}
            onPress={() => void setFlags(m().id, { flagged: !m().flagged })}
            class="-me-2 -mt-1.5"
            testID="toggle-flag"
          />
        </If>
      </View>
      <View class="flex-row items-start gap-3">
        <Avatar name={displayName(m().from)} />
        <View class="min-w-0 flex-1 gap-0.5">
          <View class="flex-row flex-wrap items-baseline gap-x-2">
            <Text class={`font-medium text-on-surface ${touch ? "text-body-large" : "text-body-medium"}`}>{displayName(m().from)}</Text>
            <Text class={`text-on-surface-variant ${touch ? "text-body-medium" : "text-body-small"}`}>{m().from?.email ?? ""}</Text>
          </View>
          <Text class={`line-clamp-2 text-on-surface-variant ${touch ? "text-body-medium" : "text-body-small"}`}>
            {t("reader.toList", { names: recipients() || t("compose.noRecipients") })}
          </Text>
          <View class="flex-row items-center gap-1.5">
            <If when={accounts().length > 1 && account()}>
              <AccountMark index={accountIndex(m().accountId)} />
              <Text class="text-body-small text-on-surface-variant">{account()?.email}</Text>
              <Text class="text-body-small text-on-surface-variant">·</Text>
            </If>
            <Text class="text-body-small tabular-nums text-on-surface-variant">{fullDate(m().date)}</Text>
          </View>
        </View>
      </View>
      <If when={!props.toolbar && !props.phone}>
        <ReaderActions message={m()} onCompose={props.onCompose} />
      </If>
    </View>
  );
  const attachments = () => m().attachments.map((attachment) => ({ id: m().id, attachment }));
  const tiles = () => (
    <Each of={attachments()} key={(x) => `${x.id}:${x.attachment.index}`}>
      {(x) => <AttachmentTile messageId={x().id} attachment={x().attachment} />}
    </Each>
  );
  // Android's floating toolbar covers the end of the page: room is kept for it.
  const end = material && props.phone ? "pb-28" : touch ? "pb-8" : "pb-10";
  // A mail's HTML scrolls in its own view, under the header and over the attachments: one scrollbar.
  return (
    <View class="flex-1">
    <If
      when={html()}
      else={
        <Scroll class="flex-1" style={enter.style} testID="reader">
          <View selectable class={`gap-4 ${pad} ${end} ${touch ? "pt-4" : "pt-5"}`}>
            {header()}
            <Divider />
            <Text class={`text-on-surface ${touch ? "text-body-large leading-6" : "text-body-medium leading-6"}`} testID="body-text">
              {m().text ?? ""}
            </Text>
            <If when={m().attachments.length}>
              <View class="gap-2">
                <Text class="text-body-small font-medium text-on-surface-variant">{t("reader.attachments")}</Text>
                <View class="flex-row flex-wrap items-end gap-2">{tiles()}</View>
              </View>
            </If>
          </View>
        </Scroll>
      }
    >
      {(source) => (
        <View class={`flex-1 gap-3 ${material && props.phone ? "pb-24" : touch ? "pb-4" : "pb-5"} ${touch ? "pt-4" : "pt-5"}`} style={enter.style} testID="reader">
          <View selectable class={`gap-4 ${pad}`}>
            {header()}
            <If when={m().remoteImages.length > 0 && !showImages()}>
              <View class="flex-row items-center gap-3 rounded-medium bg-surface-container-high px-3 py-2">
                <Icon name="image" size="sm" tone="on-surface-variant" />
                <Text class={`flex-1 text-on-surface-variant ${touch ? "text-body-medium" : "text-body-small"}`}>{t("reader.imagesBlocked")}</Text>
                <Button
                  size="sm"
                  variant="flat"
                  label={t("reader.loadImages")}
                  onPress={() => imagesShown.update((s) => ({ ...s, [m().id]: true }))}
                />
              </View>
            </If>
          </View>
          <View class={`min-h-48 flex-1 overflow-hidden border border-outline-variant bg-white ${material ? "rounded-large" : "rounded-medium"} ${touch ? "mx-2" : "mx-6"}`}>
            <MailView html={source()} remote={showImages()} onUnavailable={() => webViewAvailable.set(false)} class="flex-1" />
          </View>
          <If when={m().attachments.length}>
            <Scroll horizontal class="shrink-0" contentClass={`flex-row items-end gap-2 ${pad}`} testID="attachments">
              {tiles()}
            </Scroll>
          </If>
        </View>
      )}
    </If>
    <If when={props.phone && material}>
      <ReaderToolbar message={m()} onCompose={props.onCompose} onLeave={props.onLeave} />
    </If>
    <If when={props.phone && !material}>
      <ReplyBar onCompose={props.onCompose} />
    </If>
    </View>
  );
}

// Keyed by message and index: one tile per attachment, its preview fetched once.
function AttachmentTile(props: { messageId: string; attachment: Attachment }) {
  const a = () => props.attachment;
  const busy = signal(false);
  const thumbnail = signal<string | null>(null);
  if (a().mimeType.startsWith("image/")) {
    Mail.attachmentPreview(props.messageId, a()).then(
      (uri) => thumbnail.set(uri),
      () => {},
    );
  }
  const open = async () => {
    busy.set(true);
    try {
      const saved = await Mail.openAttachment(props.messageId, a());
      if (saved) showToast(t("reader.savedTo", { path: saved }));
    } catch (e) {
      showToast(errorText(e), "error");
    } finally {
      busy.set(false);
    }
  };
  return (
    <FileTile
      name={a().filename ?? t("reader.untitledAttachment")}
      detail={fileSize(a().size)}
      thumbnail={thumbnail()}
      busy={busy()}
      busyLabel={t("reader.openingAttachment")}
      onPress={() => void open()}
      testID={`attachment-${a().index}`}
    />
  );
}
