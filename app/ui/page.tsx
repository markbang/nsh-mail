// A page of a stack. iOS and Android draw its header natively, and Android
// colors that header and the page from the activity's theme, which is the
// light one whatever the system's mode: the page and its header take the
// theme's tokens here, so every page follows the appearance.
import { View, type JSX } from "nsh";
import { NavPage } from "nsh/base";
import { material } from "./material";

export interface PageProps {
  /** The system header's title (iOS, Android). */
  title: string;
  /** The app's own header on the desktops and the web. */
  header?: JSX.Element;
  headerLeft?: JSX.Element;
  headerRight?: JSX.Element;
  /** Android: the page's own top app bar (`m3.AppBar`) in place of the system header. */
  appBar?: JSX.Element;
  /** A tab's first page: the tab bar stays under it. */
  root?: boolean;
  popOnBack?: boolean;
  onBackPress?: () => void;
  testID?: string;
  children?: JSX.Element;
}

export function Page(props: PageProps) {
  return (
    <NavPage
      title={props.title}
      header={props.header}
      headerLeft={props.headerLeft && <View class="flex-row items-center">{props.headerLeft}</View>}
      headerRight={props.headerRight && <View class="flex-row items-center">{props.headerRight}</View>}
      headerClass="bg-surface text-on-surface tint-on-surface"
      headerVisible={!(material && props.appBar)}
      hidesBottomBarWhenPushed={!props.root}
      popOnBack={props.popOnBack ?? true}
      onBackPress={props.onBackPress}
      testID={props.testID}
      class="bg-surface"
    >
      <If when={material && props.appBar} else={props.children}>
        <View class="flex-1 bg-surface">
          {props.appBar}
          {props.children}
        </View>
      </If>
    </NavPage>
  );
}
