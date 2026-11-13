// M3 progress indicators, linear and circular: 4pt strokes, a 4pt gap between
// indicator and track, a stop dot at the end. They draw on a Canvas whose
// numbers are animated nodes, so they move natively; reduced motion leaves
// them resting. (The loading indicator is in m3.tsx.)
import { Animated, Canvas, createAnimatedValue, reduceMotion, token, View, type AnimatedNode } from "nsh";

const TAU = Math.PI * 2;
const STROKE = 4;
const GAP = 4;

/** A linear progress indicator: `value` 0…1, or none while the amount is unknown. */
export function LinearProgress(props: { label: string; value?: number; class?: string; testID?: string }) {
  const head = createAnimatedValue(0);
  const tail = createAnimatedValue(0);
  const indeterminate = () => props.value === undefined;
  if (indeterminate() && !reduceMotion()) {
    // One bar chases across: its head leads on the emphasized curve, its tail follows.
    Animated.loop(
      Animated.parallel([
        Animated.timing(head, { toValue: 1, duration: 1100, easing: [0.2, 0, 0, 1] }),
        Animated.timing(tail, { toValue: 1, duration: 1100, delay: 250, easing: [0.3, 0, 0.8, 0.15] }),
      ]),
      { resetBeforeIteration: true },
    ).start();
  }
  const active = () => token("primary");
  const track = () => token("secondary-container");
  return (
    <View role="progressbar" accessibilityLabel={props.label} accessibilityValue={props.value === undefined ? undefined : { min: 0, max: 100, now: Math.round(props.value * 100) }} class={`h-1 ${props.class ?? ""}`} testID={props.testID}>
      <Canvas
        accessibilityElementsHidden
        style={{ flex: 1 }}
        draw={(cx, { width }) => {
          const y = STROKE / 2;
          const line = (from: number | AnimatedNode, to: number | AnimatedNode, paint: string) => {
            cx.strokeStyle = paint;
            cx.lineWidth = STROKE;
            cx.lineCap = "round";
            cx.beginPath();
            cx.moveTo(from, y);
            cx.lineTo(to, y);
            cx.stroke();
          };
          const span = width - STROKE;
          if (!indeterminate()) {
            const v = Math.min(1, Math.max(0, props.value ?? 0));
            const end = y + span * v;
            if (v > 0) line(y, end, active());
            if (v < 1) line(Math.min(width - y, end + (v > 0 ? GAP + STROKE : 0)), width - y, track());
            // The stop dot marks where the track ends.
            cx.fillStyle = active();
            cx.beginPath();
            cx.arc(width - y, y, STROKE / 2, 0, TAU);
            cx.fill();
            return;
          }
          line(y, width - y, track());
          line(tail.mul(span).add(y), head.mul(span).add(y), active());
        }}
      />
    </View>
  );
}

/** A circular progress indicator, 40pt: `value` 0…1, or none while the amount is unknown. */
export function CircularProgress(props: { label: string; value?: number; size?: number; testID?: string }) {
  const size = () => props.size ?? 40;
  const spin = createAnimatedValue(0);
  const sweep = createAnimatedValue(0.1);
  if (props.value === undefined && !reduceMotion()) {
    Animated.loop(Animated.timing(spin, { toValue: 1, duration: 1400, easing: "linear" }), { resetBeforeIteration: true }).start();
    Animated.loop(
      Animated.sequence([
        Animated.timing(sweep, { toValue: 0.75, duration: 700, easing: [0.2, 0, 0, 1] }),
        Animated.timing(sweep, { toValue: 0.1, duration: 700, easing: [0.2, 0, 0, 1] }),
      ]),
    ).start();
  }
  return (
    <View role="progressbar" accessibilityLabel={props.label} testID={props.testID} style={{ width: size(), height: size() }}>
      <Canvas
        accessibilityElementsHidden
        style={{ width: size(), height: size() }}
        draw={(cx, { width }) => {
          const c = width / 2;
          const r = c - STROKE / 2;
          cx.lineWidth = STROKE;
          cx.lineCap = "round";
          const top = -Math.PI / 2;
          if (props.value !== undefined) {
            const v = Math.min(1, Math.max(0, props.value));
            const gap = (GAP + STROKE) / r;
            cx.strokeStyle = token("secondary-container");
            cx.beginPath();
            cx.arc(c, c, r, top + TAU * v + (v > 0 ? gap : 0), top + TAU - (v > 0 ? gap : 0));
            cx.stroke();
            if (v > 0) {
              cx.strokeStyle = token("primary");
              cx.beginPath();
              cx.arc(c, c, r, top, top + TAU * v);
              cx.stroke();
            }
            return;
          }
          const start = spin.mul(TAU).add(top);
          cx.strokeStyle = token("primary");
          cx.beginPath();
          cx.arc(c, c, r, start, start.add(sweep.mul(TAU)));
          cx.stroke();
        }}
      />
    </View>
  );
}
