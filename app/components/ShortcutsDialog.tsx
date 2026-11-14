// The paths whose actions have a shortcut, listed by `?` and from Settings:
// the keys as this platform shows them (`keymap.label`), with what they do.
import { keymap, Text, View } from "nsh";
import { t } from "nsh/i18n";
import { Dialog, DialogBody, SectionLabel } from "../ui";

interface Row {
  keys: string[];
  label: string;
}

const mail = (): Row[] => [
  { keys: ["c"], label: t("actions.compose") },
  { keys: ["r"], label: t("actions.reply") },
  { keys: ["/"], label: t("search.label") },
  { keys: ["j"], label: t("actions.nextMessage") },
  { keys: ["k"], label: t("actions.previousMessage") },
  { keys: ["e"], label: t("actions.archive") },
  { keys: ["shift+3"], label: t("actions.delete") },
];

const composing = (): Row[] => [
  { keys: ["mod+enter"], label: t("compose.send") },
  { keys: ["escape"], label: t("shortcuts.closeDialog") },
  { keys: ["shift+/"], label: t("shortcuts.title") },
];

function Key(props: { keys: string[] }) {
  return (
    <View class="flex-row items-center gap-1">
      <Each of={props.keys} key={(k) => k}>
        {(k) => (
          <Text class="rounded-small border border-outline-variant px-2 py-0.5 text-label-medium tabular-nums text-on-surface-variant">{keymap.label(k())}</Text>
        )}
      </Each>
    </View>
  );
}

function List(props: { rows: Row[] }) {
  return (
    <View class="gap-1">
      <Each of={props.rows} key={(r) => r.keys.join("+")}>
        {(row) => (
          <View class="min-h-9 flex-row items-center justify-between gap-4">
            <Text class="flex-1 text-body-large tracking-body-large text-on-surface">{row().label}</Text>
            <Key keys={row().keys} />
          </View>
        )}
      </Each>
    </View>
  );
}

export function ShortcutsDialog(props: { open: boolean; onClose: () => void }) {
  return (
    <Dialog open={props.open} onClose={props.onClose} label={t("shortcuts.title")} title={t("shortcuts.title")} size="sm" testID="shortcuts">
      <DialogBody class="gap-3 pb-6">
        <SectionLabel label={t("shortcuts.mail")} class="px-0 pb-1 pt-0" />
        <List rows={mail()} />
        <SectionLabel label={t("shortcuts.composing")} class="px-0 pb-1 pt-3" />
        <List rows={composing()} />
        <Text class="pt-2 text-body-small tracking-body-small text-on-surface-variant">{t("shortcuts.help")}</Text>
      </DialogBody>
    </Dialog>
  );
}
