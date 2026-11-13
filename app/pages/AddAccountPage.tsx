// Adding an account: Gmail and Outlook sign in with OAuth, anything else
// with IMAP and an app password.
import { signal, Text, View } from "nsh";
import { NavPage } from "nsh/base";
import { t } from "nsh/i18n";
import { Mail, type OAuthProvider } from "nsh-mail/native";
import { Button, touch } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { NavContext } from "../nav";
import { errorText, showToast } from "../store";
import google from "../assets/google.svg";
import microsoft from "../assets/microsoft.svg";

export function AddAccountPage() {
  const nav = NavContext();
  const busy = signal<OAuthProvider | null>(null);
  const oauth = async (provider: OAuthProvider) => {
    busy.set(provider);
    try {
      const account = await Mail.addOAuth(provider);
      showToast(t("accounts.added", { email: account.email }));
      nav.done();
    } catch (e) {
      showToast(errorText(e), "error");
    } finally {
      busy.set(null);
    }
  };
  return (
    <NavPage title={t("accounts.add")} header={<PageHeader title={t("accounts.add")} />}>
      <PageScroll contentClass={touch ? "gap-6 p-4" : "gap-6 p-6"}>
        <View class="gap-1">
          <Text class={`font-semibold text-foreground ${touch ? "text-xl" : "text-base"}`}>{t("accounts.addTitle")}</Text>
          <Text class={`text-muted-foreground ${touch ? "text-sm" : "text-xs"}`}>{t("accounts.addHint")}</Text>
        </View>
        <View class="gap-2">
          <Button size="lg" mark={google} label={t("accounts.google")} loading={busy() === "google"} disabled={!!busy()} onPress={() => void oauth("google")} testID="add-google" />
          <Button size="lg" mark={microsoft} label={t("accounts.microsoft")} loading={busy() === "microsoft"} disabled={!!busy()} onPress={() => void oauth("microsoft")} testID="add-microsoft" />
          <Button size="lg" icon="mail" label={t("accounts.other")} disabled={!!busy()} onPress={() => nav.stack.push({ name: "imap" })} testID="add-imap" />
        </View>
      </PageScroll>
    </NavPage>
  );
}
