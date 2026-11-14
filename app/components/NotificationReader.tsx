// One notification, whole: its source, sender and time, the tags, the body,
// and its actions (read, archive, open the link, open the mail it came from).
import { openUrl, Text, View } from "nsh";
import { t } from "nsh/i18n";
import type { Notification, Source } from "nsh-mail/native";
import { Button, Divider, IconButton } from "../ui";
import { PageScroll } from "./PageHeader";
import { fullDate } from "../lib/format";
import { SourceMark, Tags } from "./SourceMark";

export function NotificationReader(props: {
  note: Notification;
  source?: Source;
  onToggleRead: () => void;
  onArchive: () => void;
  onOpenMail?: () => void;
}) {
  const n = () => props.note;
  const actions = () => (
    <View class="flex-row flex-wrap items-center gap-2">
      <If when={n().link}>{(link) => (
        <Button variant="tonal" icon="external" label={t("notify.openLink")} onPress={() => void openUrl(link()).catch(() => {})} testID="notify-open-link" />
      )}</If>
      <If when={props.onOpenMail}>
        <Button variant="text" icon="mail" label={t("notify.openMail")} onPress={() => props.onOpenMail?.()} testID="notify-open-mail" />
      </If>
      <View class="flex-1" />
      <IconButton
        icon={n().read ? "mail" : "mailOpen"}
        label={n().read ? t("actions.markUnread") : t("actions.markRead")}
        onPress={() => props.onToggleRead()}
        testID="notify-read"
      />
      <IconButton icon="archive" label={t("actions.archive")} onPress={() => props.onArchive()} testID="notify-archive" />
    </View>
  );
  const content = () => (
    <View selectable class="gap-4 p-5 pt-4">
      <View class="flex-row items-center gap-2">
        <If when={props.source}>{(s) => <SourceMark source={s()} size="sm" />}</If>
        <Text class="text-label-large font-medium tracking-label-large text-on-surface-variant" testID="notify-source">
          {props.source?.name ?? ""}
        </Text>
        <View class="flex-1" />
        <Text class="text-label-medium tabular-nums tracking-label-medium text-on-surface-variant">{fullDate(n().receivedAt)}</Text>
      </View>
      <Text role="header" class="text-headline-small tracking-headline-small text-on-surface" testID="notify-title">
        {n().title}
      </Text>
      <Text class="text-body-medium tracking-body-medium text-on-surface-variant">{n().sender}</Text>
      <If when={n().priority >= 7}>
        <View class="flex-row items-center gap-1.5">
          <View accessibilityElementsHidden class="size-2 rounded-full bg-error" />
          <Text class="text-label-medium tracking-label-medium text-error">{t("notify.highPriority")}</Text>
        </View>
      </If>
      <If when={n().tags.length}>
        <Tags tags={n().tags} />
      </If>
      {actions()}
      <Divider />
      <Text class="text-body-large tracking-body-large text-on-surface" testID="notify-body">
        {n().body}
      </Text>
    </View>
  );
  return (
    <View class="flex-1 overflow-hidden" testID="notify-reader">
      <PageScroll contentClass="" testID="notify-scroll">
        {content()}
      </PageScroll>
    </View>
  );
}
