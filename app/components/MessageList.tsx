// A virtualized list of messages: unread by weight and a dot, flagged by an
// icon in its own lane, the account by a bar of its color in the unified view.
// Pointer devices and wide windows get dense rows with a one-line preview.
// Touch screens swipe a row right to archive it, left to delete it, and
// long-press to select.
import { createAnimatedValue, effect, List, reduceMotion, Text, View, type JSX } from "nsh";
import { Haptics } from "nsh/mobile";
import { t } from "nsh/i18n";
import type { Summary } from "nsh-mail/native";
import { AccountMark, Icon, Row, Spinner, touch } from "../ui";
import { displayName, listDate } from "../lib/format";
import { wide } from "../layout";
import { accountIndex, accounts } from "../store";

export interface MessageListProps {
  items: Summary[];
  selectedId?: string | null;
  onOpen: (id: string) => void;
  refreshing?: boolean;
  onRefresh?: () => void;
  onLoadMore?: () => void;
  loadingMore?: boolean;
  /** Show whose account each message is in (the unified view, search). */
  showAccount?: boolean;
  /** Show the recipients instead of the sender (Sent, Drafts). */
  showRecipients?: boolean;
  /** Search words to mark in subjects and previews. */
  highlight?: string;
  empty?: JSX.Element;
  testID?: string;
  /** Swipe actions (touch screens). */
  onArchive?: (id: string) => void;
  onDelete?: (id: string) => void;
  /** Long-press selection (touch screens): `picked` marks rows, `onPick` toggles one. */
  picked?: (id: string) => boolean;
  picking?: boolean;
  onPick?: (id: string) => void;
}

export function MessageList(props: MessageListProps) {
  const dense = () => !touch || wide();
  const terms = () => (props.highlight ?? "").split(/\s+/).filter(Boolean);
  return (
    <List
      each={props.items}
      keyExtractor={(m) => m.id}
      estimatedItemSize={dense() ? (touch ? 82 : 76) : 104}
      refreshing={props.refreshing ?? false}
      onRefresh={props.onRefresh}
      onLoadMore={props.onLoadMore}
      testID={props.testID}
      empty={props.empty}
      footer={
        <If when={props.loadingMore}>
          <View class="items-center py-3">
            <Spinner label={t("list.loadingMore")} size="sm" />
          </View>
        </If>
      }
      class="flex-1"
    >
      {(m) => (
        <Swipe id={m().id} onArchive={props.picking ? undefined : props.onArchive} onDelete={props.picking ? undefined : props.onDelete}>
          <MessageRow
            message={m()}
            selected={props.selectedId === m().id || !!props.picked?.(m().id)}
            picking={!!props.picking}
            onPress={() => (props.picking ? props.onPick?.(m().id) : props.onOpen(m().id))}
            onLongPress={
              props.onPick
                ? () => {
                    Haptics.impact("medium");
                    props.onPick?.(m().id);
                  }
                : undefined
            }
            showAccount={!!props.showAccount && accounts().length > 1}
            showRecipients={!!props.showRecipients}
            dense={dense()}
            terms={terms()}
          />
        </Swipe>
      )}
    </List>
  );
}

/** How far (of the row's width) a swipe must go to act when let go slowly. */
const SWIPE_AT = 0.35;

/** A row that swipes: right reveals Archive, left Delete. The cell is reused for other
 * messages, so the offset goes back to 0 whenever its message changes. */
function Swipe(props: { id: string; onArchive?: (id: string) => void; onDelete?: (id: string) => void; children?: JSX.Element }) {
  const x = createAnimatedValue(0);
  let width = 0;
  effect(() => {
    void props.id;
    x.set(0);
  });
  const end = (e: { value?: { x?: number }; velocity: { x: number } }) => {
    const at = (e.value?.x ?? 0) + e.velocity.x * 0.15;
    const act = width > 0 && Math.abs(at) > width * SWIPE_AT ? (at > 0 ? props.onArchive : props.onDelete) : undefined;
    if (!act) {
      void x.spring(0, { velocity: e.velocity.x });
      return;
    }
    const id = props.id;
    Haptics.impact("light");
    const away = at > 0 ? width : -width;
    if (reduceMotion()) act(id);
    else void x.timing(away, { duration: 180 }).then(() => act(id));
  };
  return (
    <If when={touch && (props.onArchive || props.onDelete)} else={props.children}>
      <View class="overflow-hidden" onLayout={({ bounds }) => (width = bounds.size.width)}>
        <View class="absolute inset-0 flex-row items-center justify-between bg-success-100 px-6" style={{ opacity: x.interpolate({ inputRange: [0, 1], outputRange: [0, 1] }) }}>
          <Icon name="archive" tone="success-800" />
        </View>
        <View class="absolute inset-0 flex-row items-center justify-end bg-danger-100 px-6" style={{ opacity: x.interpolate({ inputRange: [-1, 0], outputRange: [1, 0] }) }}>
          <Icon name="trash" tone="danger-800" />
        </View>
        <View pan={{ x }} onPanEnd={end} style={{ translate: [x, 0] }}>
          {props.children}
        </View>
      </View>
    </If>
  );
}

const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** `text` with each occurrence of `terms` marked; plain text without terms. */
function Marked(props: { text: string; terms: string[]; class: string }) {
  const parts = () => {
    const pattern = new RegExp(`(${props.terms.map(escapeRegExp).join("|")})`, "giu");
    return props.text
      .split(pattern)
      .map((text, i) => ({ key: i, text, hit: i % 2 === 1 }))
      .filter((p) => p.text);
  };
  return (
    <If when={props.terms.length} else={<Text class={props.class}>{props.text}</Text>}>
      <Text class={props.class}>
        <Each of={parts()} key={(p) => p.key}>
          {(p) => <Text class={p().hit ? "bg-flag/25 font-semibold text-foreground" : ""}>{p().text}</Text>}
        </Each>
      </Text>
    </If>
  );
}

function MessageRow(props: {
  message: Summary;
  selected: boolean;
  picking: boolean;
  onPress: () => void;
  onLongPress?: () => void;
  showAccount: boolean;
  showRecipients: boolean;
  dense: boolean;
  terms: string[];
}) {
  const m = () => props.message;
  const who = () =>
    props.showRecipients ? m().to.map(displayName).join(", ") || t("compose.noRecipients") : displayName(m().from);
  const label = () =>
    [m().seen ? "" : t("list.unread"), who(), m().subject || t("reader.noSubject"), listDate(m().date), m().flagged ? t("list.flagged") : ""]
      .filter(Boolean)
      .join(", ");
  return (
    <Row
      selected={props.selected}
      onPress={props.onPress}
      onLongPress={props.onLongPress}
      accessibilityLabel={label()}
      testID={`message-${m().id}`}
      class={`flex-row border-b border-divider pe-3 ${props.dense ? (touch ? "py-1.5" : "py-2") : "py-3"}`}
    >
      <If when={props.showAccount}>
        <AccountMark index={accountIndex(m().accountId)} shape="bar" />
      </If>
      <If
        when={props.picking}
        else={
          <View class="w-6 items-center pt-1.5">
            <If when={!m().seen}>
              <View class="size-2 rounded-full bg-unread" />
            </If>
          </View>
        }
      >
        <View class="w-10 items-center pt-0.5">
          <View class={`size-5 items-center justify-center rounded-full border-2 ${props.selected ? "border-primary bg-primary" : "border-default-400"}`}>
            <If when={props.selected}>
              <Icon name="check" size="sm" tone="primary-foreground" />
            </If>
          </View>
        </View>
      </If>
      <View class={`min-w-0 flex-1 ${touch && props.dense ? "" : "gap-0.5"}`}>
        <View class="flex-row items-center gap-1.5">
          <Text
            class={`line-clamp-1 flex-1 text-foreground ${touch ? "text-medium" : "text-small"} ${m().seen ? "" : "font-semibold"}`}
          >
            {who()}
          </Text>
          <Text class={`tabular-nums ${touch ? "text-small" : "text-tiny"} ${m().seen ? "text-foreground-500" : "font-medium text-foreground"}`}>
            {listDate(m().date)}
          </Text>
        </View>
        <View class="flex-row items-center gap-1.5">
          <Marked
            text={m().subject || t("reader.noSubject")}
            terms={props.terms}
            class={`line-clamp-1 flex-1 text-foreground ${touch ? "text-medium" : "text-small"} ${m().seen ? "" : "font-medium"}`}
          />
          <If when={m().hasAttachments}>
            <Icon name="paperclip" size="sm" tone="default-500" />
          </If>
          <View class="w-4 items-center">
            <If when={m().flagged}>
              <Icon name="flagFilled" size="sm" tone="flag" />
            </If>
          </View>
        </View>
        <Marked
          text={m().snippet}
          terms={props.terms}
          class={`${props.dense ? "line-clamp-1" : "line-clamp-2"} text-foreground-500 ${touch ? "text-small" : "text-tiny"}`}
        />
      </View>
    </Row>
  );
}
