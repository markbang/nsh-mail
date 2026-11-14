// An account's signature: plain text with a little formatting, shown as it
// will look, and whether new messages and replies get it.
import { signal, Text, untracked, View } from "nsh";
import { TextView } from "nsh/base";
import { t } from "nsh/i18n";
import { Button, Page, SectionLabel, Switch, TextField, touch } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { NavContext } from "../nav";
import { renderSignature } from "../lib/signature";
import { accountById, errorText, setSignature, showToast } from "../store";

export function SignaturePage(props: { accountId: string }) {
  const nav = NavContext();
  const account = () => accountById(props.accountId);
  const kept = untracked(() => account()?.signature) ?? { text: "", newMessages: true, replies: true };
  const text = signal(kept.text);
  const newMessages = signal(kept.newMessages);
  const replies = signal(kept.replies);
  const saving = signal(false);
  const error = signal<string | null>(null);

  const save = async () => {
    saving.set(true);
    error.set(null);
    try {
      await setSignature(props.accountId, { text: text().trim() ? text() : "", newMessages: newMessages(), replies: replies() });
      showToast(t("signature.saved"));
      nav.close();
    } catch (e) {
      error.set(errorText(e));
    } finally {
      saving.set(false);
    }
  };

  return (
    <Page title={t("signature.title")} header={<PageHeader title={t("signature.title")} />}>
      <PageScroll contentClass={touch ? "gap-4 p-4" : "gap-4 p-6"} testID="signature">
        <Text class="text-body-medium tracking-body-medium text-on-surface-variant">{account()?.email ?? ""}</Text>
        <TextField
          value={text()}
          onChange={(v) => text.set(v)}
          multiline
          label={t("signature.text")}
          placeholder={t("signature.placeholder")}
          supporting={t("signature.hint")}
          autoFocus={!touch}
          class="min-h-32"
          testID="signature-text"
        />
        <If when={text().trim()}>
          <View class="gap-1">
            <SectionLabel label={t("signature.preview")} class="px-0 pb-0" />
            <View class="gap-1 rounded-large bg-surface-container px-4 py-3">
              <Text class="text-body-large tracking-body-large text-on-surface-variant">--</Text>
              <TextView html={renderSignature(text()).html} links="open" class="text-body-large text-on-surface" testID="signature-preview" />
            </View>
          </View>
        </If>
        <View class="gap-3 py-1">
          <Switch checked={newMessages()} onChange={(on) => newMessages.set(on)} label={t("signature.newMessages")} testID="signature-new" />
          <Switch checked={replies()} onChange={(on) => replies.set(on)} label={t("signature.replies")} description={t("signature.repliesDetail")} testID="signature-replies" />
        </View>
        <If when={error()}>
          <Text class="text-body-medium tracking-body-medium text-error" testID="signature-error">
            {error()}
          </Text>
        </If>
        <Button label={t("signature.save")} loading={saving()} onPress={() => void save()} class="self-start" testID="signature-save" />
      </PageScroll>
    </Page>
  );
}
