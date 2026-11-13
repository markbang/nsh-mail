// The wide layout: mailboxes, the message list and the reader side by side.
import { Text, View, type InputRef } from "nsh";
import { t } from "nsh/i18n";
import { Empty, IconButton, Pane, Panes, Spinner } from "../ui";
import { MailboxList, mailboxTitle, resultCount } from "./Mailbox";
import { Mailboxes } from "./Mailboxes";
import { Reader, ReaderActions } from "./Reader";
import { SearchField } from "./SearchField";
import { current, listStatus, openMessage, refresh, searchQuery, searchResults, selectedId } from "../store";
import type { ComposeMode } from "../lib/format";

export interface WorkspaceProps {
  onCompose: (mode: ComposeMode) => void;
  onSettings: () => void;
  onAddAccount: () => void;
  searchRef: (ref: InputRef) => void;
}

export function Workspace(props: WorkspaceProps) {
  const open = () => (current()?.id === selectedId() ? current() : undefined);
  return (
    <Panes label={t("layout.resize")}>
      <Pane size={232} minSize={184} maxSize={320} fixed>
        <Mailboxes onCompose={() => props.onCompose("new")} onSettings={props.onSettings} />
      </Pane>
      <Pane size={400} minSize={300} maxSize={600} fixed>
        <View class="flex-1 bg-background">
          <View class="h-12 flex-row items-center gap-2 border-b border-divider pe-2 ps-4">
            <If
              when={searchResults()}
              else={
                <Text role="header" class="line-clamp-1 flex-1 text-small font-semibold text-foreground">
                  {mailboxTitle()}
                </Text>
              }
            >
              {(found) => (
                <>
                  <Text role="header" class="line-clamp-1 flex-1 text-small font-semibold text-foreground" testID="results-title">
                    {t("search.title", { query: searchQuery().trim() })}
                  </Text>
                  <Text class="text-tiny tabular-nums text-foreground-500">{resultCount(found().length)}</Text>
                </>
              )}
            </If>
            <If when={listStatus() === "refreshing"}>
              <Spinner label={t("sync.running")} size="sm" />
            </If>
            <IconButton icon="refresh" label={t("actions.refresh")} size="sm" onPress={() => void refresh()} />
          </View>
          <View class="border-b border-divider px-3 py-2">
            <SearchField ref={props.searchRef} />
          </View>
          <MailboxList onOpen={(id) => openMessage(id)} onAddAccount={props.onAddAccount} highlight />
        </View>
      </Pane>
      <Pane minSize={360}>
        <View class="flex-1 bg-background">
          <View class="h-12 flex-row items-center justify-end border-b border-divider px-2">
            <If when={open()}>{(m) => <ReaderActions message={m()} onCompose={props.onCompose} />}</If>
          </View>
          <If
            when={open()}
            else={
              <If when={selectedId() && !current.error()} else={<Empty icon="mailOpen" title={t("reader.none")} description={t("reader.noneHint")} />}>
                <View class="flex-1 items-center justify-center">
                  <Spinner label={t("reader.loading")} />
                </View>
              </If>
            }
          >
            {(m) => (
              <Each of={[m()]} key={(x) => x.id}>
                {(message) => <Reader message={message()} onCompose={props.onCompose} toolbar />}
              </Each>
            )}
          </If>
        </View>
      </Pane>
    </Panes>
  );
}
