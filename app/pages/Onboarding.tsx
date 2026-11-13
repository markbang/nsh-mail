// The first screen: where mail comes from. A browser cannot reach mail
// servers, so on the web only a server (cloud mode) is offered.
import { Notifications, platform, Scroll, signal, Text, UrlScheme, View } from "nsh";
import { t } from "nsh/i18n";
import { Mail } from "nsh-mail/native";
import { Button, Choice, Icon, Row, TextField, type IconName } from "../ui";
import { errorText, sessionChanged } from "../store";

const web = platform.os === "web";

/** Our hosted server, prefilled for cloud mode on native platforms; the user may change it. */
const DEFAULT_SERVER = "https://mail-api.bangwu.me";

/** The web client is served by its server: the page's origin. */
function pageOrigin() {
  try {
    return new URL(UrlScheme.url() ?? "").origin;
  } catch {
    return "";
  }
}

function Mode(props: { icon: IconName; title: string; detail: string; selected: boolean; onPress: () => void; testID: string }) {
  return (
    <Row
      selected={props.selected}
      onPress={props.onPress}
      testID={props.testID}
      class={`flex-row items-start gap-4 !rounded-large p-4 ${props.selected ? "" : "border border-outline-variant"}`}
    >
      <Icon name={props.icon} tone={props.selected ? "on-secondary-container" : "on-surface-variant"} />
      <View class="flex-1 gap-1">
        <Text class={`text-title-medium font-medium tracking-title-medium ${props.selected ? "text-on-secondary-container" : "text-on-surface"}`}>{props.title}</Text>
        <Text class={`text-body-medium tracking-body-medium ${props.selected ? "text-on-secondary-container" : "text-on-surface-variant"}`}>{props.detail}</Text>
      </View>
    </Row>
  );
}

export function Onboarding() {
  const mode = signal<"local" | "cloud">(web ? "cloud" : "local");
  const signUp = signal(false);
  const server = signal(web ? pageOrigin() : DEFAULT_SERVER);
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
    <Scroll class="flex-1 bg-surface" contentClass="grow items-center justify-center p-6 pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]">
      <View class="w-full max-w-[420px] gap-6 py-8" testID="onboarding">
        <View class="gap-2">
          <View class="mb-2 size-16 items-center justify-center rounded-extra-large bg-primary-container">
            <Icon name="inboxes" size="lg" tone="on-primary-container" />
          </View>
          <Text role="header" class="text-headline-medium font-medium text-on-surface">
            {t("onboarding.title")}
          </Text>
          <Text class="text-body-large tracking-body-large text-on-surface-variant">{web ? t("onboarding.subtitleWeb") : t("onboarding.subtitle")}</Text>
        </View>

        <If when={!web} else={<Text class="text-body-medium tracking-body-medium text-on-surface-variant">{t("onboarding.webOnlyCloud")}</Text>}>
          <View class="gap-2">
            <Mode
              icon="device"
              title={t("onboarding.local")}
              detail={t("onboarding.localDetail")}
              selected={mode() === "local"}
              onPress={() => mode.set("local")}
              testID="mode-local"
            />
            <Mode
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
            <Choice
              label={t("onboarding.signIn")}
              fullWidth
              value={signUp() ? "up" : "in"}
              options={[
                { value: "in", label: t("onboarding.signIn") },
                { value: "up", label: t("onboarding.signUp") },
              ]}
              onChange={(v) => signUp.set(v === "up")}
            />
            <TextField value={server()} onChange={(v) => server.set(v)} kind="url" placeholder="https://mail.example.com" label={t("onboarding.server")} testID="server" supporting={t("onboarding.serverHint")} />
            <TextField value={email()} onChange={(v) => email.set(v)} kind="email" label={t("onboarding.email")} testID="cloud-email" />
            <TextField value={password()} onChange={(v) => password.set(v)} masked label={t("onboarding.password")} onSubmit={() => void go()} returnKey="go" testID="cloud-password" supporting={signUp() ? t("onboarding.passwordHint") : undefined} />
          </View>
        </If>

        <If when={error()}>
          <Text class="text-body-medium tracking-body-medium text-error" testID="onboarding-error">
            {error()}
          </Text>
        </If>

        <Button
          size="md"
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
