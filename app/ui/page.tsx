// A page of a stack. iOS and Android draw its header natively, and Android
// colors that header and the page from the activity's theme, which is the
// light one whatever the system's mode: the page and its header take the
// theme's tokens here, so every page follows the appearance.
import { View, type JSX } from "nsh";
import { NavPage } from "nsh/base";

export interface PageProps {
  /** The system header's title (iOS, Android). */
  title: string;
  /** The app's own header on the desktops and the web. */
  header?: JSX.Element;
  headerLeft?: JSX.Element;
  headerRight?: JSX.Element;
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
      hidesBottomBarWhenPushed={!props.root}
      popOnBack={props.popOnBack ?? true}
      onBackPress={props.onBackPress}
      testID={props.testID}
      class="bg-surface"
    >
      {props.children}
    </NavPage>
  );
}
