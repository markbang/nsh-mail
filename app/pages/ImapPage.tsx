// An IMAP account: the address first, then its servers (found from the
// address, editable) and an app password.
import { openUrl, signal, Text, View } from "nsh";
import { t } from "nsh/i18n";
import { Mail, type Endpoint, type Security, type ServerSettings } from "nsh-mail/native";
import { Button, Icon, Page, SectionLabel, SelectField, TextField, touch } from "../ui";
import { PageHeader, PageScroll } from "../components/PageHeader";
import { NavContext } from "../nav";
import { isAddress, providerName } from "../lib/format";
import { errorText, showToast } from "../store";

const guess = (email: string): ServerSettings => {
  const domain = email.split("@")[1] ?? "";
  return {
    username: email,
    imap: { host: domain ? `imap.${domain}` : "", port: 993, security: "tls" },
    smtp: { host: domain ? `smtp.${domain}` : "", port: 465, security: "tls" },
    auth: { kind: "password" },
    appPassword: true,
  };
};

export function ImapPage() {
  const nav = NavContext();
  const step = signal<"address" | "servers">("address");
  const email = signal("");
  const name = signal("");
  const password = signal("");
  const settings = signal<ServerSettings>(guess(""));
  const known = signal(false);
  const busy = signal(false);
  const error = signal<string | null>(null);

  const set = (server: "imap" | "smtp", change: Partial<Endpoint>) =>
    settings.update((s) => ({ ...s, [server]: { ...s[server], ...change } }));
  const port = (text: string) => Number.parseInt(text.replace(/\D/g, ""), 10) || 0;
  const security = [
    { value: "tls", title: t("imap.tls") },
    { value: "starttls", title: t("imap.starttls") },
  ];

  const detect = async () => {
    if (!isAddress(email())) return error.set(t("imap.badEmail"));
    error.set(null);
    busy.set(true);
    try {
      const found = await Mail.detect(email().trim());
      known.set(!!found);
      settings.set(found ? { ...found, username: found.username || email().trim() } : guess(email().trim()));
      step.set("servers");
    } catch (e) {
      error.set(errorText(e));
    } finally {
      busy.set(false);
    }
  };

  const add = async () => {
    const s = settings();
    if (!s.imap.host.trim() || !s.smtp.host.trim()) return error.set(t("imap.needServers"));
    if (!password()) return error.set(t("imap.needPassword"));
    error.set(null);
    busy.set(true);
    try {
      const account = await Mail.addImap({
        email: email().trim(),
        name: name().trim() || null,
        username: s.username.trim() || null,
        password: password(),
        imap: s.imap,
        smtp: s.smtp,
        provider: s.provider ?? null,
      });
      showToast(t("accounts.added", { email: account.email }));
      nav.done();
    } catch (e) {
      error.set(errorText(e));
    } finally {
      busy.set(false);
    }
  };

  const note = "text-body-medium tracking-body-medium";
  const provider = () => providerName(settings().provider, settings().displayName ?? "");
  return (
    <Page title={t("accounts.other")} header={<PageHeader title={t("accounts.other")} />}>
      <PageScroll contentClass={touch ? "gap-4 p-4" : "gap-4 p-6"} testID="imap">
        <TextField
            value={email()}
            onChange={(v) => {
              email.set(v);
              if (step() === "servers") step.set("address");
            }}
            onSubmit={() => void detect()}
            kind="email"
            placeholder="you@example.com"
            label={t("imap.email")}
            returnKey="next"
            autoFocus
            testID="imap-email" />
        <TextField value={name()} onChange={(v) => name.set(v)} label={t("imap.name")} testID="imap-name" supporting={t("imap.nameHint")} />

        <If when={step() === "servers"}>
          <View class="gap-4">
            <Text class={`text-on-surface-variant ${note}`} testID="imap-detected">
              {known() ? t("imap.found", { provider: provider() }) : t("imap.notFound")}
            </Text>
            <If when={settings().auth.kind === "oauth"}>
              <Text class={`text-on-surface ${note}`} testID="imap-oauth">
                {t("imap.useOAuth", { provider: provider() })}
              </Text>
            </If>
            <SectionLabel label={t("imap.incoming")} class="px-0 pb-0" />
            <View class="flex-row gap-2">
              <TextField class="flex-1" value={settings().imap.host} onChange={(v) => set("imap", { host: v.trim() })} kind="url" label={t("imap.imapHost")} testID="imap-host" />
              <TextField class="w-20" value={String(settings().imap.port || "")} onChange={(v) => set("imap", { port: port(v) })} kind="number" label={t("imap.imapPort")} testID="imap-port" />
            </View>
            <SelectField value={settings().imap.security} items={security} onChange={(v) => set("imap", { security: v as Security })} label={t("imap.security")} />
            <SectionLabel label={t("imap.outgoing")} class="px-0 pb-0" />
            <View class="flex-row gap-2">
              <TextField class="flex-1" value={settings().smtp.host} onChange={(v) => set("smtp", { host: v.trim() })} kind="url" label={t("imap.smtpHost")} testID="smtp-host" />
              <TextField class="w-20" value={String(settings().smtp.port || "")} onChange={(v) => set("smtp", { port: port(v) })} kind="number" label={t("imap.smtpPort")} testID="smtp-port" />
            </View>
            <SelectField value={settings().smtp.security} items={security} onChange={(v) => set("smtp", { security: v as Security })} label={t("imap.security")} />
            <TextField value={settings().username} onChange={(v) => settings.update((s) => ({ ...s, username: v.trim() }))} kind="email" label={t("imap.username")} />
            <TextField value={password()} onChange={(v) => password.set(v)} masked label={t("imap.password")} onSubmit={() => void add()} returnKey="done" testID="imap-password" />
            <View class="flex-row items-start gap-3 rounded-large bg-surface-container-high p-4">
              <Icon name="key" size="sm" tone="on-surface-variant" />
              <View class="flex-1 gap-2">
                <Text class={`text-on-surface-variant ${note}`}>{t("imap.appPasswordHint")}</Text>
                <If when={settings().helpUrl}>
                  {(url) => (
                    <Button
                      size="xs"
                      variant="tonal"
                      icon="external"
                      label={t("imap.appPasswordLink", { provider: provider() })}
                      onPress={() => void openUrl(url())}
                      class="self-start"
                    />
                  )}
                </If>
              </View>
            </View>
          </View>
        </If>

        <If when={error()}>
          <Text class={`text-error ${note}`} testID="imap-error">
            {error()}
          </Text>
        </If>

        <If
          when={step() === "servers"}
          else={<Button label={t("imap.continue")} loading={busy()} onPress={() => void detect()} class="self-start" testID="imap-continue" />}
        >
          <Button label={busy() ? t("imap.connecting") : t("imap.add")} loading={busy()} onPress={() => void add()} class="self-start" testID="imap-add" />
        </If>
      </PageScroll>
    </Page>
  );
}
