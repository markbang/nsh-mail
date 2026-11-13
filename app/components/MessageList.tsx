// A virtualized list of messages: unread by weight and a dot, flagged by an
// icon in its own lane, the account by a bar of its color in the unified view.
// Pointer devices and wide windows get dense rows with a one-line preview.
import { List, Text, View, type JSX } from "nsh";
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
}

export function MessageList(props: MessageListProps) {
  const dense = () => !touch || wide();
  const terms = () => (props.highlight ?? "").split(/\s+/).filter(Boolean);
  return (
    <List
      each={props.items}
      keyExtractor={(m) => m.id}
      estimatedItemSize={dense() ? (touch ? 76 : 70) : 104}
      refreshing={props.refreshing ?? false}
      onRefresh={props.onRefresh}
      onLoadMore={props.onLoadMore}
      testID={props.testID}
      empty={props.empty}
      footer={
        <If when={props.loadingMore}>
          <View class="items-center py-3">
            <Spinner label={t("list.loadingMore")} />
          </View>
        </If>
      }
      class="flex-1"
    >
      {(m) => (
        <MessageRow
          message={m()}
          selected={props.selectedId === m().id}
          onPress={() => props.onOpen(m().id)}
          showAccount={!!props.showAccount && accounts().length > 1}
          showRecipients={!!props.showRecipients}
          dense={dense()}
          terms={terms()}
        />
      )}
    </List>
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
  onPress: () => void;
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
      accessibilityLabel={label()}
      testID={`message-${m().id}`}
      class={`flex-row border-b border-border pe-3 ${props.dense ? "py-2" : "py-3"}`}
    >
      <If when={props.showAccount}>
        <AccountMark index={accountIndex(m().accountId)} shape="bar" />
      </If>
      <View class="w-6 items-center pt-1.5">
        <If when={!m().seen}>
          <View class="size-2 rounded-full bg-unread" />
        </If>
      </View>
      <View class="min-w-0 flex-1 gap-0.5">
        <View class="flex-row items-center gap-1.5">
          <Text
            class={`line-clamp-1 flex-1 text-foreground ${touch ? "text-base" : "text-sm"} ${m().seen ? "" : "font-semibold"}`}
          >
            {who()}
          </Text>
          <Text class={`tabular-nums ${touch ? "text-sm" : "text-xs"} ${m().seen ? "text-muted-foreground" : "font-medium text-foreground"}`}>
            {listDate(m().date)}
          </Text>
        </View>
        <View class="flex-row items-center gap-1.5">
          <Marked
            text={m().subject || t("reader.noSubject")}
            terms={props.terms}
            class={`line-clamp-1 flex-1 text-foreground ${touch ? "text-base" : "text-sm"} ${m().seen ? "" : "font-medium"}`}
          />
          <If when={m().hasAttachments}>
            <Icon name="paperclip" size="sm" tone="muted-foreground" />
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
          class={`${props.dense ? "line-clamp-1" : "line-clamp-2"} text-muted-foreground ${touch ? "text-sm" : "text-xs"}`}
        />
      </View>
    </Row>
  );
}
