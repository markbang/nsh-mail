// Android phones get Material 3 Expressive's own components (ui/m3.tsx) where the
// shared screens keep their pointer-first ones; the tokens are everyone's (ui/tokens.ts).
import { platform } from "nsh";
import { springTo, transition as springTransition } from "./tokens";

export const material = platform.os === "android";

/** M3E's springs for `AnimatedValue.spring` (ExpressiveMotionTokens). */
export const springs = {
  fastSpatial: springTo("spatial", "fast"),
  spatial: springTo("spatial", "default"),
  slowSpatial: springTo("spatial", "slow"),
  fastEffects: springTo("effects", "fast"),
  effects: springTo("effects", "default"),
} as const;

/** The same springs as class transitions (Material's web curves for them). */
export const transition = {
  fastSpatial: `${springTransition("spatial", "fast", "transition-all")}`,
  spatial: `${springTransition("spatial", "default", "transition-all")}`,
  fastEffects: `${springTransition("effects", "fast", "transition-colors")}`,
  effects: `${springTransition("effects", "default")}`,
} as const;

/** A pressed button's shape morph: from its resting corners to Material's pressed `small`, on the fast spatial spring. */
export const pressMorph = `${transition.fastSpatial} active:rounded-small`;
