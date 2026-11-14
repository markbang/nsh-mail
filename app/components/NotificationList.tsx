// The feed's list: one row per notification under the day it arrived on,
// grouped by day, the source's icon and colour leading it, unread in
// emphasized type with a dot, a high priority's in the error colour, tags
// after the snippet. Touch screens swipe a row right to archive it, left to
// mark it read, and long-press it for its actions.
import { List, signal, Text, View, type JSX } from "nsh";
import { Haptics } from "nsh/mobile";
import { t } from "nsh/i18n";
import type { Notification } from "nsh-mail/native";
import { Icon, ListItem, material, Row, Sheet, touch } from "../ui";
import { dayLabel, dayOf, relativeDate } from "../lib/format";
import { wide } from "../layout";
import { archiveNotifications, markNotifications, sourceById } from "../notify";
import { PriorityDot, SourceMark, Tags } from "./SourceMark";
import { Swipe } from "./Swipe";

/** A heading for a day, or a notification under it. */
interface FeedRow {
  key: string;
  /** A day heading's local midnight. */
  day?: number;
  note?: Notification;
}

/** The feed's rows: a heading for each day, its notifications under it. */
function rows(items: Notification[]): FeedRow[] {
  const out: FeedRow[] = [];
  let day: number | null = null;
  for (const note of items) {
    const at = dayOf(note.receivedAt);
    if (at !== day) {
      day = at;
      out.push({ key: `day-${at}`, day: at });
    }
    out.push({ key: note.id, note });
  }
  return out;
}

export function NotificationList(props: {
  items: Notification[];
  selectedId?: string | null;
  onOpen: (id: string) => void;
  loadingMore?: boolean;
  refreshing?: boolean;
  onRefresh?: () => void;
  onLoadMore?: () => void;
  empty: JSX.Element;
  testID?: string;
}) {
  const dense = () => !touch || wide();
  const actions = signal<Notification | null>(null);
  const find = (id: string) => props.items.find((n) => n.id === id);
  const archive = (id: string) => {
    const n = find(id);
    if (n) archiveNotifications([n.id]);
  };
  const toggleRead = (id: string) => {
    const n = find(id);
    if (n) void markNotifications([n.id], !n.read);
  };
  const act = (fn: (n: Notification) => void) => {
    const n = actions();
    actions.set(null);
    if (n) fn(n);
  };
  return (
    <View class="flex-1">
      <List
        role="list"
        accessibilityLabel={t("notify.list")}
        each={rows(props.items)}
        keyExtractor={(row) => row.key}
        estimatedItemSize={dense() ? 104 : 132}
        refreshing={props.refreshing ?? false}
        onRefresh={props.onRefresh}
        onLoadMore={props.onLoadMore}
        testID={props.testID}
        empty={props.empty}
        class="flex-1"
        contentClass={material && !wide() ? undefined : "px-2 pb-2"}
      >
        {(row) => (
          <If
            when={row().note}
            else={
              <View class="px-4 pb-1 pt-4">
                <Text role="header" class="text-label-large font-medium tracking-label-large text-on-surface-variant">
                  {dayLabel(row().day ?? 0)}
                </Text>
              </View>
            }
          >
            {(note) => (
              <Swipe
                id={row().key}
                onArchive={archive}
                onDelete={toggleRead}
                leftIcon={note().read ? "mail" : "mailOpen"}
                leftTone="secondary"
              >
                <NotificationRow
                  note={note()}
                  selected={props.selectedId === row().key}
                  dense={dense()}
                  onPress={() => props.onOpen(row().key)}
                  onLongPress={
                    touch
                      ? () => {
                          Haptics.impact("medium");
                          actions.set(note());
                        }
                      : undefined
                  }
                />
              </Swipe>
            )}
          </If>
        )}
      </List>
      <Sheet open={!!actions()} onClose={() => actions.set(null)} side="bottom" label={t("notify.actions")}>
        <View class="gap-1 p-3">
          <Text role="header" class="line-clamp-2 px-4 pb-2 text-title-medium text-on-surface">
            {actions()?.title ?? ""}
          </Text>
          <ListItem
            icon={actions()?.read ? "mail" : "mailOpen"}
            headline={actions()?.read ? t("actions.markUnread") : t("actions.markRead")}
            onPress={() => act((n) => void markNotifications([n.id], !n.read))}
            testID="notify-action-read"
          />
          <ListItem icon="archive" headline={t("actions.archive")} onPress={() => act((n) => archiveNotifications([n.id]))} testID="notify-action-archive" />
        </View>
      </Sheet>
    </View>
  );
}

function NotificationRow(props: { note: Notification; selected: boolean; dense: boolean; onPress: () => void; onLongPress?: () => void }) {
  const n = () => props.note;
  const source = () => sourceById(n().sourceId);
  const label = () =>
    [n().read ? "" : t("list.unread"), source()?.name ?? "", n().sender, n().title, n().priority >= 7 ? t("notify.highPriority") : "", relativeDate(n().receivedAt)]
      .filter(Boolean)
      .join(", ");
  return (
    <Row
      selected={props.selected}
      onPress={props.onPress}
      onLongPress={props.onLongPress}
      accessibilityLabel={label()}
      testID={`notify-${n().id}`}
      class="mb-0.5 flex-row gap-3 py-3 ps-3 pe-4"
    >
      <If when={source()}>{(s) => <SourceMark source={s()} size={props.dense ? "sm" : "md"} />}</If>
      <View class="min-w-0 flex-1 gap-0.5">
        <View class="flex-row items-center gap-1.5">
          <Text class={`line-clamp-1 shrink text-label-large tracking-label-large ${n().read ? "text-on-surface-variant" : "font-medium text-on-surface"}`}>
            {n().sender}
          </Text>
          <Text class="line-clamp-1 text-label-medium tracking-label-medium text-on-surface-variant">
            {source()?.name === n().sender ? "" : (source()?.name ?? "")}
          </Text>
          <View class="flex-1" />
          <Text class={`text-label-medium tabular-nums tracking-label-medium ${n().read ? "text-on-surface-variant" : "font-medium text-primary"}`}>
            {relativeDate(n().receivedAt)}
          </Text>
          <If when={!n().read}>
            <View accessibilityElementsHidden class="size-2 rounded-full bg-unread" />
          </If>
        </View>
        <View class="flex-row items-center gap-1.5">
          <PriorityDot priority={n().priority} />
          <Text class={`line-clamp-1 flex-1 text-body-large ${n().read ? "tracking-body-large text-on-surface" : "font-bold tracking-body-large-emphasized text-on-surface"}`}>
            {n().title}
          </Text>
        </View>
        <Text class={`${props.dense ? "line-clamp-1" : "line-clamp-2"} text-body-medium tracking-body-medium text-on-surface-variant`}>
          {n().body.replace(/\s+/g, " ")}
        </Text>
        <Tags tags={n().tags} />
      </View>
      <If when={n().link}>
        <Icon name="external" size="sm" tone="on-surface-variant" />
      </If>
    </Row>
  );
}
