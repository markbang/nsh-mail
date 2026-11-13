// A page's header on desktops and the web (phones show the system's, from
// the page's title): back or close, the title, the page's actions.
import { cleanup, Scroll, Text, untracked, View, type JSX } from "nsh";
import { t } from "nsh/i18n";
import { IconButton, touch } from "../ui";
import { NavContext } from "../nav";

/** The header's height, which a dialog's page adds to its content's. */
export const headerHeight = touch ? 56 : 48;

/** A page's scrolling content under its header: in a dialog it reports its height, so the dialog fits it. */
export function PageScroll(props: { contentClass: string; testID?: string; children?: JSX.Element }) {
  const nav = NavContext();
  const fit = nav.fit;
  const index = untracked(() => nav.stack.routes().length) - 1;
  if (fit) cleanup(() => fit(index, undefined));
  // The dialog's border (1pt above and below) is inside its height.
  const report = (height: number) => fit?.(index, Math.ceil(height) + headerHeight + 2);
  return (
    <Scroll class="flex-1 bg-background" testID={props.testID}>
      <View class={props.contentClass} onLayout={fit ? ({ bounds }) => report(bounds.size.height) : undefined}>
        {props.children}
      </View>
    </Scroll>
  );
}

export function PageHeader(props: { title: string; actions?: JSX.Element; leading?: JSX.Element; testID?: string }) {
  const nav = NavContext();
  const depth = () => nav.stack.routes().length;
  return (
    <View class="flex-row items-center gap-1 border-b border-border bg-background px-2" style={{ height: headerHeight }}>
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
