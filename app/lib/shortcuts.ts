// Whether the shortcuts list is open: `?` opens it anywhere, and Settings has a
// row for it, since a phone with no hardware keyboard has no `?` to press.
import { signal } from "nsh";

export const shortcutsOpen = signal(false);
