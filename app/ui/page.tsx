// A page of a stack. iOS and Android draw its header natively, and Android
// colors that header and the page from the activity's theme, which is the
// light one whatever the system's mode: the page and its header take the
// theme's tokens here, so every page follows the appearance.
import type { JSX } from "nsh";
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
      headerLeft={props.headerLeft}
      headerRight={props.headerRight}
      headerClass="bg-background text-foreground tint-foreground"
      hidesBottomBarWhenPushed={!props.root}
      popOnBack={props.popOnBack ?? true}
      onBackPress={props.onBackPress}
      testID={props.testID}
      class="bg-background"
    >
      {props.children}
    </NavPage>
  );
}
