// The first screen: where mail comes from. A browser cannot reach mail
// servers, so on the web only a server (cloud mode) is offered.
import { Notifications, platform, Scroll, signal, Text, UrlScheme, View } from "nsh";
import { t } from "nsh/i18n";
import { Mail } from "nsh-mail/native";
import { Button, Field, Icon, Row, Segmented, TextField, touch, type IconName } from "../ui";
import { errorText, sessionChanged } from "../store";

const web = platform.os === "web";

/** The web client is served by its server: the page's origin. */
function pageOrigin() {
  try {
    return new URL(UrlScheme.url() ?? "").origin;
  } catch {
    return "";
  }
}

function Choice(props: { icon: IconName; title: string; detail: string; selected: boolean; onPress: () => void; testID: string }) {
  return (
    <Row
      selected={props.selected}
      onPress={props.onPress}
      testID={props.testID}
      class={`flex-row items-start gap-3 rounded-large border-2 p-4 ${props.selected ? "border-primary" : "border-default-200"}`}
    >
      <Icon name={props.icon} tone={props.selected ? "primary" : "default-500"} />
      <View class="flex-1 gap-1">
        <Text class={`font-medium text-foreground ${touch ? "text-medium" : "text-small"}`}>{props.title}</Text>
        <Text class={`text-foreground-500 ${touch ? "text-small" : "text-tiny"}`}>{props.detail}</Text>
      </View>
    </Row>
  );
}

export function Onboarding() {
  const mode = signal<"local" | "cloud">(web ? "cloud" : "local");
  const signUp = signal(false);
  const server = signal(web ? pageOrigin() : "");
  const email = signal("");
  const password = signal("");
  const busy = signal(false);
  const error = signal<string | null>(null);

  const finish = async () => {
    // Asked here, as the user acts: browsers refuse a prompt nobody caused.
    void Notifications.requestPermission().catch(() => undefined);
    await sessionChanged();
  };
  const go = async () => {
    error.set(null);
    busy.set(true);
    try {
      if (mode() === "local") await Mail.useLocal();
      else await Mail.signIn(server(), email().trim(), password(), signUp());
      await finish();
    } catch (e) {
      error.set(errorText(e));
    } finally {
      busy.set(false);
    }
  };

  return (
    <Scroll class="flex-1 bg-background" contentClass="grow items-center justify-center p-6 pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]">
      <View class="w-full max-w-[420px] gap-6 py-8" testID="onboarding">
        <View class="gap-2">
          <Icon name="inboxes" size="lg" tone="primary" />
          <Text role="header" class={`font-semibold text-foreground ${touch ? "text-2xl" : "text-xl"}`}>
            {t("onboarding.title")}
          </Text>
          <Text class={`text-foreground-500 ${touch ? "text-medium" : "text-small"}`}>{web ? t("onboarding.subtitleWeb") : t("onboarding.subtitle")}</Text>
        </View>

        <If when={!web} else={<Text class={`text-foreground-500 ${touch ? "text-small" : "text-tiny"}`}>{t("onboarding.webOnlyCloud")}</Text>}>
          <View class="gap-2">
            <Choice
              icon="device"
              title={t("onboarding.local")}
              detail={t("onboarding.localDetail")}
              selected={mode() === "local"}
              onPress={() => mode.set("local")}
              testID="mode-local"
            />
            <Choice
              icon="cloud"
              title={t("onboarding.cloud")}
              detail={t("onboarding.cloudDetail")}
              selected={mode() === "cloud"}
              onPress={() => mode.set("cloud")}
              testID="mode-cloud"
            />
          </View>
        </If>

        <If when={mode() === "cloud"}>
          <View class="gap-4">
            <Segmented
              value={signUp() ? "up" : "in"}
              options={[
                { value: "in", label: t("onboarding.signIn") },
                { value: "up", label: t("onboarding.signUp") },
              ]}
              onChange={(v) => signUp.set(v === "up")}
            />
            <Field label={t("onboarding.server")} description={t("onboarding.serverHint")}>
              <TextField value={server()} onChange={(v) => server.set(v)} kind="url" placeholder="https://mail.example.com" label={t("onboarding.server")} testID="server" />
            </Field>
            <Field label={t("onboarding.email")}>
              <TextField value={email()} onChange={(v) => email.set(v)} kind="email" label={t("onboarding.email")} testID="cloud-email" />
            </Field>
            <Field label={t("onboarding.password")} description={signUp() ? t("onboarding.passwordHint") : undefined}>
              <TextField value={password()} onChange={(v) => password.set(v)} masked label={t("onboarding.password")} onSubmit={() => void go()} returnKey="go" testID="cloud-password" />
            </Field>
          </View>
        </If>

        <If when={error()}>
          <Text class={`text-danger ${touch ? "text-small" : "text-tiny"}`} testID="onboarding-error">
            {error()}
          </Text>
        </If>

        <Button
          color="primary"
          size="lg"
          fullWidth
          label={mode() === "local" ? t("onboarding.useLocal") : signUp() ? t("onboarding.createAccount") : t("onboarding.signIn")}
          loading={busy()}
          onPress={() => void go()}
          testID="onboarding-go"
        />
      </View>
    </Scroll>
  );
}
