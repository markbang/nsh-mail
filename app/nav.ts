// Navigation. Phones (and narrow windows) push every screen on one stack;
// wide windows keep the three panes and open compose, settings and adding
// an account in a dialog with a stack of its own.
import { context, signal } from "nsh";
import { createNavStack, type NavStackState } from "nsh/base";
import type { ComposeInit } from "./lib/format";

export type Route =
  | { name: "inbox" }
  | { name: "message"; id: string }
  | { name: "compose"; init: ComposeInit }
  | { name: "search" }
  | { name: "settings" }
  | { name: "addAccount" }
  | { name: "imap" };

export const phoneStack = createNavStack<Route>([{ name: "inbox" }]);
export const modalStack = createNavStack<Route>([{ name: "settings" }]);

const modalOpen = signal(false);
export const isModalOpen = () => modalOpen();

export interface Navigator {
  stack: NavStackState<Route>;
  /** Leaves the screen: pops it, or closes the dialog at its root. */
  close: () => void;
  /** Leaves the whole flow: back to the inbox, or the dialog closed. */
  done: () => void;
  /** Screens in a dialog draw a close button instead of back at their root. */
  modal: boolean;
}

export const NavContext = context<Navigator>({
  stack: phoneStack,
  close: () => phoneStack.pop(),
  done: () => phoneStack.popToRoot(),
  modal: false,
});

export const phoneNavigator: Navigator = {
  stack: phoneStack,
  close: () => phoneStack.pop(),
  done: () => phoneStack.popToRoot(),
  modal: false,
};

export const modalNavigator: Navigator = {
  stack: modalStack,
  close: () => (modalStack.routes().length > 1 ? modalStack.pop() : modalOpen.set(false)),
  done: () => modalOpen.set(false),
  modal: true,
};

/** Opens `route` as the wide layout or the phone layout shows it. */
export function show(route: Route, wide: boolean) {
  if (!wide) {
    phoneStack.push(route);
    return;
  }
  modalStack.setRoutes([route]);
  modalOpen.set(true);
}

export function closeModal() {
  modalOpen.set(false);
}
