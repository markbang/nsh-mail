// A page's header on desktops and the web (phones show the system's, from
// the page's title): back or close, the title, the page's actions.
import { Text, View, type JSX } from "nsh";
import { t } from "nsh/i18n";
import { IconButton, touch } from "../ui";
import { NavContext } from "../nav";

export function PageHeader(props: { title: string; actions?: JSX.Element; leading?: JSX.Element; testID?: string }) {
  const nav = NavContext();
  const depth = () => nav.stack.routes().length;
  return (
    <View class={`flex-row items-center gap-1 border-b border-border bg-background px-2 ${touch ? "h-14" : "h-12"}`}>
      <If
        when={props.leading}
        else={
          <If when={depth() > 1 || nav.modal}>
            <IconButton
              icon={depth() > 1 ? "back" : "close"}
              label={depth() > 1 ? t("common.back") : t("common.close")}
              onPress={nav.close}
              testID="header-back"
            />
          </If>
        }
      >
        {props.leading}
      </If>
      <Text role="header" class={`line-clamp-1 flex-1 px-2 font-semibold text-foreground ${touch ? "text-lg" : "text-sm"}`}>
        {props.title}
      </Text>
      <View class="flex-row items-center gap-1">{props.actions}</View>
    </View>
  );
}
