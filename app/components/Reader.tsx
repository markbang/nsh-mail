// One message, or a conversation: its older messages collapsed above the
// one expanded (the latest when it opens) and its newer ones below; a tap on
// a collapsed message expands it. For the expanded message: who and when,
// the body (its HTML in a web view, or its text) and its attachments. Remote
// images stay blocked until the reader asks for them. Archive, delete, read
// and flag act on the whole conversation; reply and forward on the message.
import { createMotion, reduceMotion, Scroll, signal, Text, View } from "nsh";
import { t } from "nsh/i18n";
import { Mail, type Attachment, type Message, type Summary } from "nsh-mail/native";
import { AccountMark, Avatar, Button, Divider, FileTile, FloatingToolbar, Icon, IconButton, LoadingIndicator, material, Menu, Row, ToolbarButton, touch } from "../ui";
import { displayName, fileSize, fullDate, listDate, type ComposeMode } from "../lib/format";
import {
  accountById,
  accountIndex,
  accounts,
  conversationsEnabled,
  errorText,
  isLeaving,
  moveMessages,
  openMessage,
  ownAddresses,
  selectedId,
  setFlags,
  setThreadFlags,
  showToast,
} from "../store";
import { MailView, webViewAvailable } from "./MailView";
import { wide } from "../layout";

export interface ReaderProps {
  message: Message;
  /** Its conversation, oldest first, while conversations are on. */
  conversation?: Summary[];
  onCompose: (mode: ComposeMode) => void;
  /** The actions float in a toolbar at the reader's bottom (wide windows, narrow browsers). */
  toolbar?: boolean;
  /** Phones: the actions are in the system header and at the bottom (Android's floating toolbar). */
  phone?: boolean;
  /** Leaves the page (a phone, after archiving or deleting). */
  onLeave?: () => void;
}

// Per message: the reader chose to load its images.
const imagesShown = signal<Record<string, boolean>>({});
// Per conversation: every older message listed, not only the first and the last.
const allShown = signal<Record<string, boolean>>({});

/** What the actions act on: the conversation shown, or the message alone. */
export interface Target {
  message: Message;
  /** The conversation's messages on screen (more than one), oldest first. */
  thread: Summary[] | null;
}

/** The messages of `conversation` the reader lists around `m`: none when it is alone. What is in
 * Trash stays out unless the reader is in Trash; what is being deleted leaves at once. */
export function threadOf(m: Message, conversation: Summary[] | undefined): Summary[] | null {
  if (!conversation?.some((s) => s.id === m.id)) return null;
  const shown = conversation.filter((s) => s.id === m.id || ((s.folder !== "trash" || m.folder === "trash") && !isLeaving(s.id)));
  return shown.length > 1 ? shown : null;
}

const seen = (x: Target) => (x.thread ? x.thread.every((s) => s.seen) : x.message.seen);
const flagged = (x: Target) => (x.thread ? x.thread.some((s) => s.flagged) : x.message.flagged);

/** Archive or delete the conversation (or message): `before` runs first (a phone leaves the page). */
function move(x: Target, folder: "archive" | "trash", before?: () => void) {
  before?.();
  // The list's row is the conversation even when it holds this message alone.
  if (conversationsEnabled()) moveMessages([x.message.threadId], folder, true);
  else moveMessages([x.message.id], folder);
}

function mark(x: Target, flags: { seen?: boolean; flagged?: boolean }) {
  if (x.thread) void setThreadFlags(x.message.threadId, flags);
  else void setFlags(x.message.id, flags);
}

/** The phone's header actions: archive, delete, unread, and the rest under More. */
export function PhoneReaderActions(props: { message: Message; conversation?: Summary[]; onLeave: () => void }) {
  const x = (): Target => ({ message: props.message, thread: threadOf(props.message, props.conversation) });
  const m = () => props.message;
  return (
    <>
      {/* Android has them in the floating toolbar. */}
      <If when={!material && m().folder !== "archive" && m().folder !== "trash"}>
        <IconButton icon="archive" label={t("actions.archive")} onPress={() => move(x(), "archive", props.onLeave)} testID="archive" />
      </If>
      <If when={!material && m().folder !== "trash"}>
        <IconButton icon="trash" label={t("actions.delete")} onPress={() => move(x(), "trash", props.onLeave)} testID="delete" />
      </If>
      <IconButton
        icon={seen(x()) ? "mail" : "mailOpen"}
        label={seen(x()) ? t("actions.markUnread") : t("actions.markRead")}
        onPress={() => {
          mark(x(), { seen: !seen(x()) });
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
            label: flagged(x()) ? t("actions.unflag") : t("actions.flag"),
            icon: flagged(x()) ? "flagFilled" : "flag",
            onSelect: () => mark(x(), { flagged: !flagged(x()) }),
          },
        ]}
      />
    </>
  );
}

/** M3E's floating toolbar, vibrant, over the end of the message: reply first, then
 * forward, archive and delete; wide windows add read and flag, which phones keep in the header. */
function ReaderToolbar(props: { target: Target; onCompose: (mode: ComposeMode) => void; onLeave?: () => void; wide?: boolean }) {
  const x = () => props.target;
  const m = () => props.target.message;
  return (
    <View class="absolute inset-x-0 bottom-0 items-center pb-[env(safe-area-inset-bottom)]" style={{ pointerEvents: "box-none" }}>
      <View class="mb-4">
        <FloatingToolbar label={t("reader.actions")} vibrant testID="reader-toolbar">
          <ToolbarButton icon="reply" label={t("actions.reply")} emphasized vibrant onPress={() => props.onCompose("reply")} testID="reply" />
          <ToolbarButton icon="replyAll" label={t("actions.replyAll")} vibrant onPress={() => props.onCompose("replyAll")} testID="reply-all" />
          <ToolbarButton icon="forward" label={t("actions.forward")} vibrant onPress={() => props.onCompose("forward")} testID="forward" />
          <If when={m().folder !== "archive" && m().folder !== "trash"}>
            <ToolbarButton icon="archive" label={t("actions.archive")} vibrant onPress={() => move(x(), "archive", props.onLeave)} testID="archive" />
          </If>
          <If when={m().folder !== "trash"}>
            <ToolbarButton icon="trash" label={t("actions.delete")} vibrant onPress={() => move(x(), "trash", props.onLeave)} testID="delete" />
          </If>
          <If when={props.wide}>
            <ToolbarButton
              icon={seen(x()) ? "mail" : "mailOpen"}
              label={seen(x()) ? t("actions.markUnread") : t("actions.markRead")}
              vibrant
              onPress={() => mark(x(), { seen: !seen(x()) })}
              testID="toggle-seen"
            />
            <ToolbarButton
              icon={flagged(x()) ? "flagFilled" : "flag"}
              label={flagged(x()) ? t("actions.unflag") : t("actions.flag")}
              vibrant
              onPress={() => mark(x(), { flagged: !flagged(x()) })}
              testID="toggle-flag"
            />
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
        <Button variant="outlined" icon="reply" label={t("actions.reply")} onPress={() => props.onCompose("reply")} class="flex-1" testID="reply" />
        <Button variant="outlined" icon="replyAll" label={t("actions.replyAll")} onPress={() => props.onCompose("replyAll")} class="flex-1" testID="reply-all" />
        <Button variant="outlined" icon="forward" label={t("actions.forward")} onPress={() => props.onCompose("forward")} class="flex-1" testID="forward" />
      </View>
    </View>
  );
}


/** A message of the conversation that is not expanded: who, when and how it starts; a tap expands it. */
function Collapsed(props: { message: Summary; loading: boolean; pad: string }) {
  const m = () => props.message;
  const who = () => (ownAddresses().has(m().from?.email.toLowerCase() ?? "") ? t("list.me") : displayName(m().from));
  const note = () => (m().folder === "drafts" ? t("thread.draft") : m().folder === "trash" ? t("thread.inTrash") : "");
  return (
    <Row
      onPress={() => openMessage(m().id)}
      accessibilityLabel={[m().seen ? "" : t("list.unread"), who(), note(), listDate(m().date), m().snippet].filter(Boolean).join(", ")}
      testID={`thread-message-${m().id}`}
      class={`flex-row items-center gap-3 py-2 ${props.pad}`}
    >
      <Avatar name={displayName(m().from)} size="sm" />
      <View class="min-w-0 flex-1">
        <View class="flex-row items-center gap-2">
          <Text class={`line-clamp-1 shrink text-body-large tracking-body-large text-on-surface ${m().seen ? "" : "font-bold"}`}>{who()}</Text>
          <If when={note()}>
            <Text class="text-label-medium text-error">{note()}</Text>
          </If>
          <View class="flex-1" />
          <If when={m().hasAttachments}>
            <Icon name="paperclip" size="sm" tone="on-surface-variant" />
          </If>
          <If when={m().flagged}>
            <Icon name="flagFilled" size="sm" tone="flag" />
          </If>
          <Text class={`text-label-medium tabular-nums ${m().seen ? "text-on-surface-variant" : "font-bold text-primary"}`}>{listDate(m().date)}</Text>
        </View>
        <Text class="line-clamp-1 text-body-medium tracking-body-medium text-on-surface-variant">{m().snippet}</Text>
      </View>
      <If when={props.loading}>
        <LoadingIndicator label={t("reader.loading")} size={24} />
      </If>
    </Row>
  );
}

/** Collapsed messages, as many as fit: past three, the first, how many more, and the last. */
function CollapsedList(props: { messages: Summary[]; threadId: string; pad: string; testID: string }) {
  const all = () => !!allShown()[props.threadId] || props.messages.length <= 3;
  const loading = (s: Summary) => selectedId() === s.id;
  const row = (s: () => Summary) => <Collapsed message={s()} loading={loading(s())} pad={props.pad} />;
  return (
    <View testID={props.testID}>
      <If
        when={all()}
        else={
          <>
            {row(() => props.messages[0])}
            <View class={`flex-row items-center gap-3 py-1 ${props.pad}`}>
              <Divider class="flex-1" />
              <Button
                size="xs"
                variant="tonal"
                label={t("thread.more", { count: props.messages.length - 2 })}
                onPress={() => allShown.update((a) => ({ ...a, [props.threadId]: true }))}
                testID="thread-more"
              />
              <Divider class="flex-1" />
            </View>
            {row(() => props.messages[props.messages.length - 1])}
          </>
        }
      >
        <Each of={props.messages} key={(s) => s.id}>
          {(s) => row(s)}
        </Each>
      </If>
    </View>
  );
}

export function Reader(props: ReaderProps) {
  const m = () => props.message;
  const target = (): Target => ({ message: m(), thread: threadOf(m(), props.conversation) });
  const at = () => target().thread?.findIndex((s) => s.id === m().id) ?? -1;
  const older = () => target().thread?.slice(0, at()) ?? [];
  const newer = () => target().thread?.slice(at() + 1) ?? [];
  const enter = createMotion({ enter: reduceMotion() ? undefined : "fadeIn", duration: 150 });
  const account = () => accountById(m().accountId);
  const recipients = () => [...m().to, ...m().cc].map(displayName).join(", ");
  const showImages = () => !!imagesShown()[m().id];
  const html = () => (webViewAvailable() ? m().html : null);
  const pad = touch ? "px-4" : "px-6";
  // Rows reach the reader's edges, so their state layer does; their content lines up with the text.
  const rowPad = touch ? "px-4" : "px-6";
  const subject = () => (target().thread?.[0].subject ?? m().subject) || t("reader.noSubject");
  // Delete one message of a conversation: the reader moves to its neighbor first, so the conversation stays.
  const deleteOne = () => {
    const thread = target().thread ?? [];
    const next = thread[at() + 1] ?? thread[at() - 1];
    if (next) openMessage(next.id);
    moveMessages([m().id], "trash");
  };
  const messageMenu = () => (
    <Menu
      label={t("thread.messageActions")}
      trigger={<IconButton icon="more" size="sm" label={t("thread.messageActions")} class="-me-2" testID="message-more" />}
      actions={[
        { id: "reply", label: t("actions.reply"), icon: "reply", onSelect: () => props.onCompose("reply") },
        { id: "replyAll", label: t("actions.replyAll"), icon: "replyAll", onSelect: () => props.onCompose("replyAll") },
        { id: "forward", label: t("actions.forward"), icon: "forward", onSelect: () => props.onCompose("forward") },
        { id: "unread", label: t("actions.markUnread"), icon: "mail", onSelect: () => void setFlags(m().id, { seen: false }) },
        ...(m().folder === "trash" ? [] : [{ id: "delete", label: t("thread.deleteMessage"), icon: "trash" as const, destructive: true, onSelect: deleteOne }]),
      ]}
    />
  );
  const header = () => (
    <View class="gap-3">
      <View class="flex-row items-start gap-2">
        <Text role="header" class="flex-1 text-headline-small text-on-surface">
          {subject()}
        </Text>
        <If when={props.phone}>
          <IconButton
            icon={flagged(target()) ? "flagFilled" : "flag"}
            tone={flagged(target()) ? "flag" : "on-surface-variant"}
            selected={flagged(target())}
            label={flagged(target()) ? t("actions.unflag") : t("actions.flag")}
            onPress={() => mark(target(), { flagged: !flagged(target()) })}
            class="-me-2 -mt-1.5"
            testID="toggle-flag"
          />
        </If>
      </View>
      <If when={older().length}>
        <View class={touch ? "-mx-4" : "-mx-6"}>
          <CollapsedList messages={older()} threadId={m().threadId} pad={rowPad} testID="thread-older" />
        </View>
      </If>
      <View class="flex-row items-start gap-3">
        <Avatar name={displayName(m().from)} />
        <View class="min-w-0 flex-1 gap-0.5">
          <View class="flex-row flex-wrap items-baseline gap-x-2">
            <Text class="text-title-medium font-medium tracking-title-medium text-on-surface">{displayName(m().from)}</Text>
            <Text class="text-body-medium tracking-body-medium text-on-surface-variant">{m().from?.email ?? ""}</Text>
          </View>
          <Text class="line-clamp-2 text-body-medium tracking-body-medium text-on-surface-variant">
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
        <If when={target().thread}>{messageMenu()}</If>
      </View>
    </View>
  );
  const newerList = () => (
    <If when={newer().length}>
      <CollapsedList messages={newer()} threadId={m().threadId} pad={rowPad} testID="thread-newer" />
    </If>
  );
  const attachments = () => m().attachments.map((attachment) => ({ id: m().id, attachment }));
  const tiles = () => (
    <Each of={attachments()} key={(x) => `${x.id}:${x.attachment.index}`}>
      {(x) => <AttachmentTile messageId={x().id} attachment={x().attachment} />}
    </Each>
  );
  // Android's floating toolbar covers the end of the page: room is kept for it.
  const floating = () => !!props.toolbar || (material && !!props.phone);
  const end = () => (floating() ? "pb-28" : "pb-8");
  // A mail's HTML scrolls in its own view, under the header and over the attachments: one scrollbar.
  return (
    <View class="flex-1">
    <If
      when={html()}
      else={
        <Scroll class="flex-1" style={enter.style} testID="reader">
          <View selectable class={`gap-4 ${pad} ${end()} ${touch ? "pt-4" : "pt-5"}`}>
            {header()}
            <Divider />
            <Text class="text-body-large tracking-body-large text-on-surface" testID="body-text">
              {m().text ?? ""}
            </Text>
            <If when={m().attachments.length}>
              <View class="gap-2">
                <Text class="text-title-small font-medium tracking-title-small text-on-surface-variant">{t("reader.attachments")}</Text>
                <View class="flex-row flex-wrap items-end gap-2">{tiles()}</View>
              </View>
            </If>
            <If when={newer().length}>
              <View class={touch ? "-mx-4" : "-mx-6"}>
                <Divider />
                {newerList()}
              </View>
            </If>
          </View>
        </Scroll>
      }
    >
      {(source) => (
        <View class={`flex-1 gap-3 ${floating() ? "pb-24" : "pb-4"} ${touch ? "pt-4" : "pt-5"}`} style={enter.style} testID="reader">
          <View selectable class={`gap-4 ${pad}`}>
            {header()}
            <If when={m().remoteImages.length > 0 && !showImages()}>
              <View class="flex-row items-center gap-3 rounded-large bg-surface-container-high py-2 pe-2 ps-4">
                <Icon name="image" size="sm" tone="on-surface-variant" />
                <Text class="flex-1 text-body-medium tracking-body-medium text-on-surface-variant">{t("reader.imagesBlocked")}</Text>
                <Button
                  size="xs"
                  variant="tonal"
                  label={t("reader.loadImages")}
                  onPress={() => imagesShown.update((s) => ({ ...s, [m().id]: true }))}
                />
              </View>
            </If>
          </View>
          {/* The mail card stays white in both appearances on purpose: mail is designed on white and its colors are not inverted. */}
          <View class={`min-h-48 flex-1 overflow-hidden rounded-large border border-outline-variant bg-white ${touch || !wide() ? "mx-2" : "mx-6"}`}>
            <MailView html={source()} remote={showImages()} onUnavailable={() => webViewAvailable.set(false)} class="flex-1" />
          </View>
          <If when={m().attachments.length}>
            <Scroll horizontal class="shrink-0" contentClass={`flex-row items-end gap-2 ${pad}`} testID="attachments">
              {tiles()}
            </Scroll>
          </If>
          {newerList()}
        </View>
      )}
    </If>
    <If when={floating()}>
      <ReaderToolbar target={target()} onCompose={props.onCompose} onLeave={props.onLeave} wide={!props.phone} />
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
