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

// The heights the dialog's pages measured, by their place in its stack.
const fits = signal<(number | undefined)[]>([]);
/** The height the dialog's top page needs: until it has measured, the page under it's; none for the composer, which takes the dialog's own. */
export const modalFit = () => {
  const heights = fits();
  for (let i = modalStack.routes().length - 1; i >= 0; i--) if (heights[i] !== undefined) return heights[i];
  return undefined;
};

export interface Navigator {
  stack: NavStackState<Route>;
  /** Leaves the screen: pops it, or closes the dialog at its root. */
  close: () => void;
  /** Leaves the whole flow: back to the inbox, or the dialog closed. */
  done: () => void;
  /** Screens in a dialog draw a close button instead of back at their root. */
  modal: boolean;
  /** A dialog's page reports the height it needs (`undefined` when it leaves); the dialog follows, up to the window. */
  fit?: (index: number, height: number | undefined) => void;
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
  fit: (index, height) =>
    fits.update((heights) => {
      const next = [...heights];
      next[index] = height;
      return next;
    }),
};

/** Opens `route` as the wide layout or the phone layout shows it. */
export function show(route: Route, wide: boolean) {
  if (!wide) {
    phoneStack.push(route);
    return;
  }
  fits.set([]);
  modalStack.setRoutes([route]);
  modalOpen.set(true);
}

export function closeModal() {
  modalOpen.set(false);
}
