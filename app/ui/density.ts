// Desktop and web share compact pointer sizes; phones and tablets get touch sizes.
import { platform } from "nsh";

/** True on touch screens (iOS, Android, a browser in the phone idiom). */
export const touch = !platform.desktop;

/** iOS and Android draw a page's header themselves (NavPage `title`, `headerLeft`, `headerRight`). */
export const systemHeader = platform.os === "ios" || platform.os === "android";
