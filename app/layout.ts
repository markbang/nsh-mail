// Which shell shows: three panes from this width up, a stack below it.
import { windowSize } from "nsh";

export const WIDE = 840;
export const wide = () => windowSize().width >= WIDE;
