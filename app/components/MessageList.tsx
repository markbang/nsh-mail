// A virtualized list of messages: unread by weight and a dot, flagged by an
// icon in its own lane, the account by a bar of its color in the unified view.
// Pointer devices and wide windows get dense rows with a one-line preview.
// Touch screens swipe a row right to archive it, left to delete it, and
// long-press to select.
import { createAnimatedValue, effect, List, reduceMotion, signal, Text, View, type JSX } from "nsh";
import { Haptics } from "nsh/mobile";
import { t } from "nsh/i18n";
import type { Summary } from "nsh-mail/native";
import { AccountMark, Icon, initials, LoadingIndicator, material, Row, Spinner, springs, touch, transition } from "../ui";
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

/** Android's pull to refresh: Material's contained loading indicator, growing with the pull. */
const PULL = 80;

export function MessageList(props: MessageListProps) {
  const dense = () => !touch || wide();
  const terms = () => (props.highlight ?? "").split(/\s+/).filter(Boolean);
  const m3 = () => material && !wide();
  const pull = createAnimatedValue(0);
  return (
    <View class="flex-1">
    <List
      refreshIndicator={m3() && props.onRefresh ? { height: PULL } : undefined}
      pullValue={m3() ? pull : undefined}
      contentStyle={m3() ? { paddingTop: 4, paddingBottom: 96 } : undefined}
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
          <If when={m3()} else={<MessageRow
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
          />}>
            <MaterialRow
              message={m()}
              selected={props.selectedId === m().id || !!props.picked?.(m().id)}
              picked={!!props.picked?.(m().id)}
              onPress={() => (props.picking ? props.onPick?.(m().id) : props.onOpen(m().id))}
              onPick={
                props.onPick
                  ? () => {
                      Haptics.impact("medium");
                      props.onPick?.(m().id);
                    }
                  : undefined
              }
              showAccount={!!props.showAccount && accounts().length > 1}
              showRecipients={!!props.showRecipients}
              terms={terms()}
            />
          </If>
        </Swipe>
      )}
    </List>
    <If when={m3() && props.onRefresh}>
      <View
        class="absolute inset-x-0 top-0 items-center justify-center"
        style={{
          height: PULL,
          pointerEvents: "none",
          opacity: pull.interpolate({ inputRange: [0, 24], outputRange: [0, 1] }),
          transform: [{ translateY: pull.interpolate({ inputRange: [0, PULL], outputRange: [-PULL / 2, 0] }) }],
        }}
      >
        <LoadingIndicator
          label={t("list.refreshing")}
          contained
          progress={props.refreshing ? -1 : pull.interpolate({ inputRange: [0, PULL], outputRange: [0, 1] })}
          testID="pull-indicator"
        />
      </View>
    </If>
    </View>
  );
}

/** How far (of the row's width) a swipe must go to act when let go slowly. */
const SWIPE_AT = 0.35;

/** A row that swipes: right reveals Archive, left Delete. The cell is reused for other
 * messages, so the offset goes back to 0 whenever its message changes. */
function Swipe(props: { id: string; onArchive?: (id: string) => void; onDelete?: (id: string) => void; children?: JSX.Element }) {
  const x = createAnimatedValue(0);
  let width = 0;
  const span = signal(360);
  const dragging = signal(false);
  effect(() => {
    void props.id;
    x.set(0);
  });
  const end = (e: { value?: { x?: number }; velocity: { x: number } }) => {
    dragging.set(false);
    const at = (e.value?.x ?? 0) + e.velocity.x * 0.15;
    const act = width > 0 && Math.abs(at) > width * SWIPE_AT ? (at > 0 ? props.onArchive : props.onDelete) : undefined;
    if (!act) {
      void x.spring(0, { ...(material ? springs.spatial : {}), velocity: e.velocity.x });
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
      <If when={!material} else={materialSwipe()}>
      <View class="overflow-hidden" onLayout={({ bounds }) => (width = bounds.size.width)}>
        <View class="absolute inset-0 flex-row items-center justify-between bg-tertiary-container px-6" style={{ opacity: x.interpolate({ inputRange: [0, 1], outputRange: [0, 1] }) }}>
          <Icon name="archive" tone="on-tertiary-container" />
        </View>
        <View class="absolute inset-0 flex-row items-center justify-end bg-error-container px-6" style={{ opacity: x.interpolate({ inputRange: [-1, 0], outputRange: [1, 0] }) }}>
          <Icon name="trash" tone="on-error-container" />
        </View>
        <View pan={{ x }} onPanEnd={end} style={{ translate: [x, 0] }}>
          {props.children}
        </View>
      </View>
      </If>
    </If>
  );

  // Material 3 Expressive: the row lifts into a rounded card as it moves, over its action in a tonal
  // container; the action's icon grows on the spatial spring's curve as the swipe nears the point where it acts.
  function materialSwipe() {
    const grow = (sign: 1 | -1) =>
      x.interpolate({
        inputRange: sign > 0 ? [0, span() * SWIPE_AT, span() * SWIPE_AT + 1] : [-span() * SWIPE_AT - 1, -span() * SWIPE_AT, 0],
        outputRange: sign > 0 ? [0.6, 1, 1.25] : [1.25, 1, 0.6],
      });
    return (
      <View
        class="mx-2 overflow-hidden rounded-large"
        onLayout={({ bounds }) => {
          width = bounds.size.width;
          span.set(bounds.size.width);
        }}
      >
        <View class="absolute inset-0 flex-row items-center bg-tertiary-container px-6" style={{ opacity: x.interpolate({ inputRange: [0, 8], outputRange: [0, 1] }) }}>
          <View style={{ transform: [{ scale: grow(1) }] }}>
            <Icon name="archive" class="tint-on-tertiary-container" />
          </View>
        </View>
        <View class="absolute inset-0 flex-row items-center justify-end bg-error-container px-6" style={{ opacity: x.interpolate({ inputRange: [-8, 0], outputRange: [1, 0] }) }}>
          <View style={{ transform: [{ scale: grow(-1) }] }}>
            <Icon name="trash" class="tint-on-error-container" />
          </View>
        </View>
        <View
          pan={{ x }}
          onPanStart={() => dragging.set(true)}
          onPanEnd={end}
          class={`overflow-hidden ${transition.fastSpatial} ${dragging() ? "rounded-extra-large" : "rounded-large"}`}
          style={{ translate: [x, 0] }}
        >
          {props.children}
        </View>
      </View>
    );
  }
}

const tones = [
  "bg-primary-container text-on-primary-container",
  "bg-secondary-container text-on-secondary-container",
  "bg-tertiary-container text-on-tertiary-container",
];
// FNV-1a: one sender, one tone.
const toneOf = (key: string) => tones[[...key].reduce((h, c) => Math.imul(h ^ c.charCodeAt(0), 16777619) >>> 0, 2166136261) % tones.length];

/** Android's row: the sender's initials in a tonal circle that morphs into a checked rounded square when
 * picked (a tap on it picks, as in Gmail), unread in emphasized type, a picked row in the secondary container. */
function MaterialRow(props: {
  message: Summary;
  selected: boolean;
  picked: boolean;
  onPress: () => void;
  onPick?: () => void;
  showAccount: boolean;
  showRecipients: boolean;
  terms: string[];
}) {
  const m = () => props.message;
  const who = () => (props.showRecipients ? m().to.map(displayName).join(", ") || t("compose.noRecipients") : displayName(m().from));
  const label = () =>
    [m().seen ? "" : t("list.unread"), who(), m().subject || t("reader.noSubject"), listDate(m().date), m().flagged ? t("list.flagged") : ""]
      .filter(Boolean)
      .join(", ");
  return (
    <Row
      selected={props.selected}
      onPress={props.onPress}
      onLongPress={props.onPick}
      accessibilityLabel={label()}
      testID={`message-${m().id}`}
      class={`flex-row gap-3 rounded-large py-3 ps-3 pe-4 ${transition.fastSpatial}`}
    >
      <View
        role="checkbox"
        accessibilityLabel={t("actions.select")}
        accessibilityState={{ checked: props.picked }}
        onPress={props.onPick ? () => props.onPick?.() : undefined}
        hitSlop={4}
        class={`size-10 items-center justify-center ${transition.fastSpatial} ${props.picked ? "rounded-medium bg-primary" : `rounded-full ${toneOf(who())}`}`}
      >
        <If when={props.picked} else={<Text class="text-title-small font-medium">{initials(who())}</Text>}>
          <Icon name="check" class="tint-on-primary" />
        </If>
      </View>
      <View class="min-w-0 flex-1">
        <View class="flex-row items-center gap-1.5">
          <If when={props.showAccount}>
            <AccountMark index={accountIndex(m().accountId)} />
          </If>
          <Text class={`line-clamp-1 flex-1 text-body-large ${m().seen ? "text-on-surface-variant" : "font-bold text-on-surface"}`}>{who()}</Text>
          <Text class={`text-label-medium tabular-nums ${m().seen ? "font-medium text-on-surface-variant" : "font-bold text-primary"}`}>{listDate(m().date)}</Text>
        </View>
        <View class="flex-row items-center gap-1.5">
          <Marked
            text={m().subject || t("reader.noSubject")}
            terms={props.terms}
            class={`line-clamp-1 flex-1 text-body-medium ${m().seen ? "text-on-surface" : "font-bold text-on-surface"}`}
          />
          <If when={m().hasAttachments}>
            <Icon name="paperclip" size="sm" class="tint-on-surface-variant" />
          </If>
          <If when={m().flagged}>
            <Icon name="flagFilled" size="sm" tone="flag" />
          </If>
        </View>
        <Marked text={m().snippet} terms={props.terms} class="line-clamp-1 text-body-medium text-on-surface-variant" />
      </View>
    </Row>
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
          {(p) => <Text class={p().hit ? "bg-flag/25 font-semibold text-on-surface" : ""}>{p().text}</Text>}
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
      class={`flex-row border-b border-outline-variant pe-3 ${props.dense ? (touch ? "py-1.5" : "py-2") : "py-3"}`}
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
          <View class={`size-5 items-center justify-center rounded-full border-2 ${props.selected ? "border-primary bg-primary" : "border-outline"}`}>
            <If when={props.selected}>
              <Icon name="check" size="sm" tone="on-primary" />
            </If>
          </View>
        </View>
      </If>
      <View class={`min-w-0 flex-1 ${touch && props.dense ? "" : "gap-0.5"}`}>
        <View class="flex-row items-center gap-1.5">
          <Text
            class={`line-clamp-1 flex-1 text-on-surface ${touch ? "text-body-large" : "text-body-medium"} ${m().seen ? "" : "font-semibold"}`}
          >
            {who()}
          </Text>
          <Text class={`tabular-nums ${touch ? "text-body-medium" : "text-body-small"} ${m().seen ? "text-on-surface-variant" : "font-medium text-on-surface"}`}>
            {listDate(m().date)}
          </Text>
        </View>
        <View class="flex-row items-center gap-1.5">
          <Marked
            text={m().subject || t("reader.noSubject")}
            terms={props.terms}
            class={`line-clamp-1 flex-1 text-on-surface ${touch ? "text-body-large" : "text-body-medium"} ${m().seen ? "" : "font-medium"}`}
          />
          <If when={m().hasAttachments}>
            <Icon name="paperclip" size="sm" tone="on-surface-variant" />
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
          class={`${props.dense ? "line-clamp-1" : "line-clamp-2"} text-on-surface-variant ${touch ? "text-body-medium" : "text-body-small"}`}
        />
      </View>
    </Row>
  );
}
