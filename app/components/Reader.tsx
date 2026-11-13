// One message: who and when, the actions on it, the body (sanitized HTML
// as native text, or plain text) and its attachments. Remote images stay
// blocked until the reader asks for them.
import { createMotion, Image, openUrl, reduceMotion, Scroll, signal, Text, View } from "nsh";
import { TextView } from "nsh/base";
import { t } from "nsh/i18n";
import { Mail, type Attachment, type Message } from "nsh-mail/native";
import { AccountMark, Avatar, Button, FileTile, Icon, IconButton, Divider, touch } from "../ui";
import { displayName, fileSize, fullDate, type ComposeMode } from "../lib/format";
import { accountById, accountIndex, accounts, errorText, setFlags, showToast } from "../store";

export interface ReaderProps {
  message: Message;
  onCompose: (mode: ComposeMode) => void;
  /** Wide layouts put the actions in the reader's own toolbar. */
  toolbar?: boolean;
}

// Per message: the reader chose to load its images.
const imagesShown = signal<Record<string, boolean>>({});

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
    </View>
  );
}

export function Reader(props: ReaderProps) {
  const m = () => props.message;
  const enter = createMotion({ enter: reduceMotion() ? undefined : "fadeIn", duration: 150 });
  const account = () => accountById(m().accountId);
  const recipients = () => [...m().to, ...m().cc].map(displayName).join(", ");
  const showImages = () => !!imagesShown()[m().id];
  const blocked = () => m().remoteImages.length > 0;
  // The engine takes remote images' `src` out (in order, into `remoteImages`); loading puts them back.
  const html = () => {
    const source = m().html;
    if (!source || !showImages()) return source;
    let next = 0;
    return source.replace(/<img\b(?![^>]*\ssrc=)/gi, (tag) => {
      const url = m().remoteImages[next++];
      return url ? `${tag} src="${url.replace(/"/g, "&quot;")}"` : tag;
    });
  };
  return (
    <Scroll class="flex-1" style={enter.style} testID="reader">
      <View selectable class={touch ? "gap-4 px-4 pb-8 pt-4" : "gap-4 px-6 pb-10 pt-5"}>
      <View class="gap-3">
        <Text role="header" class={`font-semibold text-foreground ${touch ? "text-xl" : "text-large"}`}>
          {m().subject || t("reader.noSubject")}
        </Text>
        <View class="flex-row items-start gap-3">
          <Avatar name={displayName(m().from)} />
          <View class="min-w-0 flex-1 gap-0.5">
            <View class="flex-row flex-wrap items-baseline gap-x-2">
              <Text class={`font-medium text-foreground ${touch ? "text-medium" : "text-small"}`}>{displayName(m().from)}</Text>
              <Text class={`text-foreground-500 ${touch ? "text-small" : "text-tiny"}`}>{m().from?.email ?? ""}</Text>
            </View>
            <Text class={`line-clamp-2 text-foreground-500 ${touch ? "text-small" : "text-tiny"}`}>
              {t("reader.toList", { names: recipients() || t("compose.noRecipients") })}
            </Text>
            <View class="flex-row items-center gap-1.5">
              <If when={accounts().length > 1 && account()}>
                <AccountMark index={accountIndex(m().accountId)} />
                <Text class="text-tiny text-foreground-500">{account()?.email}</Text>
                <Text class="text-tiny text-foreground-500">·</Text>
              </If>
              <Text class="text-tiny tabular-nums text-foreground-500">{fullDate(m().date)}</Text>
            </View>
          </View>
        </View>
        <If when={!props.toolbar}>
          <ReaderActions message={m()} onCompose={props.onCompose} />
        </If>
      </View>
      <Divider />
      <If when={blocked() && !showImages()}>
        <View class="flex-row items-center gap-3 rounded-medium bg-default-100 px-3 py-2">
          <Icon name="image" size="sm" tone="default-500" />
          <Text class={`flex-1 text-foreground-500 ${touch ? "text-small" : "text-tiny"}`}>{t("reader.imagesBlocked")}</Text>
          <Button
            size="sm"
            variant="flat"
            label={t("reader.loadImages")}
            onPress={() => imagesShown.update((s) => ({ ...s, [m().id]: true }))}
          />
        </View>
      </If>
      <If
        when={html()}
        else={
          <Text class={`text-foreground ${touch ? "text-medium leading-6" : "text-small leading-6"}`} testID="body-text">
            {m().text ?? ""}
          </Text>
        }
      >
        {(html) => (
          <TextView
            html={html()}
            selectable
            links="report"
            onLinkPress={({ url }) => {
              // Links to other sites and apps; anchors (`#…`) and relative links lead nowhere in mail.
              if (/^[a-z][a-z0-9+.-]*:/i.test(url)) void openUrl(url);
            }}
            renderImage={(image) =>
              image.src ? (
                <Image src={image.src} accessibilityLabel={image.alt} class="max-w-full rounded-medium" style={{ width: "100%", aspectRatio: 16 / 9 }} />
              ) : (
                <View class="flex-row items-center gap-2 rounded-medium border border-divider px-3 py-2">
                  <Icon name="image" size="sm" tone="default-500" />
                  <Text class="text-tiny text-foreground-500">{image.alt || t("reader.blockedImage")}</Text>
                </View>
              )
            }
            class={`gap-3 text-foreground ${touch ? "text-medium" : "text-small"}`}
            classes={{ link: "text-primary", blockquote: "border-l-2 border-divider pl-3 text-foreground-500" }}
            testID="body-html"
          />
        )}
      </If>
      <If when={m().attachments.length}>
        <View class="gap-2">
          <Text class="text-tiny font-medium text-foreground-500">{t("reader.attachments")}</Text>
          <View class="flex-row flex-wrap items-end gap-2">
            <Each of={m().attachments.map((attachment) => ({ id: m().id, attachment }))} key={(x) => `${x.id}:${x.attachment.index}`}>
              {(x) => <AttachmentTile messageId={x().id} attachment={x().attachment} />}
            </Each>
          </View>
        </View>
      </If>
      </View>
    </Scroll>
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
