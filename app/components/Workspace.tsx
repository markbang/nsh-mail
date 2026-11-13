// The wide layout (M3's expanded and large window classes): the navigation
// rail, then the message list and the reader as two panes on the surface
// container, the list fixed and the reader flexible. The rail expands from
// 1200pt wide.
import { Text, View, windowSize, type InputRef } from "nsh";
import { t } from "nsh/i18n";
import { Empty, Fab, IconButton, LoadingIndicator, Pane, Panes } from "../ui";
import { MailboxList, mailboxTitle, resultCount } from "./Mailbox";
import { MailboxRail } from "./Mailboxes";
import { Reader } from "./Reader";
import { SearchField } from "./SearchField";
import { conversation, current, listStatus, openRow, refresh, searchQuery, searchResults, selectedId, shownMessage } from "../store";
import type { ComposeMode } from "../lib/format";

export interface WorkspaceProps {
  onCompose: (mode: ComposeMode) => void;
  onSettings: () => void;
  onAddAccount: () => void;
  searchRef: (ref: InputRef) => void;
}

export const RAIL_EXPANDED = 1200;

export function Workspace(props: WorkspaceProps) {
  const open = () => shownMessage();
  const expanded = () => windowSize().width >= RAIL_EXPANDED;
  const fab = () => (
    <If when={expanded()} else={<Fab icon="compose" label={t("actions.compose")} onPress={() => props.onCompose("new")} testID="compose" />}>
      <Fab icon="compose" label={t("actions.compose")} extended onPress={() => props.onCompose("new")} class="self-start" testID="compose" />
    </If>
  );
  return (
    <View class="flex-1 flex-row bg-surface-container">
      <View class={expanded() ? "w-[256px]" : "w-24"}>
        <MailboxRail expanded={expanded()} fab={fab()} onSettings={props.onSettings} />
      </View>
      <Panes label={t("layout.resize")} class="py-3 pe-3">
        <Pane size={400} minSize={320} maxSize={560} fixed first>
          <View class="flex-1 overflow-hidden rounded-large bg-surface">
            <View class="h-16 flex-row items-center gap-2 pe-2 ps-5">
              <If
                when={searchResults()}
                else={
                  <Text role="header" class="line-clamp-1 flex-1 text-title-large tracking-title-large text-on-surface">
                    {mailboxTitle()}
                  </Text>
                }
              >
                {(found) => (
                  <>
                    <Text role="header" class="line-clamp-1 flex-1 text-title-large tracking-title-large text-on-surface" testID="results-title">
                      {t("search.title", { query: searchQuery().trim() })}
                    </Text>
                    <Text class="text-label-large tabular-nums text-on-surface-variant">{resultCount(found().length)}</Text>
                  </>
                )}
              </If>
              <If when={listStatus() === "refreshing"} else={<IconButton icon="refresh" label={t("actions.refresh")} onPress={() => void refresh()} />}>
                <LoadingIndicator label={t("sync.running")} size={40} testID="refreshing" />
              </If>
            </View>
            <View class="px-3 pb-2">
              <SearchField ref={props.searchRef} />
            </View>
            <MailboxList onOpen={openRow} onAddAccount={props.onAddAccount} highlight />
          </View>
        </Pane>
        <Pane minSize={360} last>
          <View class="flex-1 overflow-hidden rounded-large bg-surface">
            <If
              when={open()}
              else={
                <If when={selectedId() && !current.error()} else={<Empty icon="mailOpen" title={t("reader.none")} description={t("reader.noneHint")} />}>
                  <View class="flex-1 items-center justify-center">
                    <LoadingIndicator label={t("reader.loading")} contained />
                  </View>
                </If>
              }
            >
              {(m) => (
                <Each of={[m()]} key={(x) => x.id}>
                  {(message) => <Reader message={message()} conversation={conversation()} onCompose={props.onCompose} toolbar />}
                </Each>
              )}
            </If>
          </View>
        </Pane>
      </Panes>
    </View>
  );
}
