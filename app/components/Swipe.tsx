// A list row that swipes: right reveals the first action (archive), left the
// other (delete, or mark read). M3E: the row lifts into a rounded card as it
// moves, over its action in a tonal container; the action's icon grows on the
// spatial spring's curve as the swipe nears the point where it acts.
import { createAnimatedValue, effect, reduceMotion, signal, View, type JSX } from "nsh";
import { Haptics } from "nsh/mobile";
import { Icon, material, springs, touch, transition, type IconName } from "../ui";

/** How far (of the row's width) a swipe must go to act when let go slowly. */
const SWIPE_AT = 0.35;

const tones = {
  error: { container: "bg-error-container", ink: "tint-on-error-container" },
  secondary: { container: "bg-secondary-container", ink: "tint-on-secondary-container" },
} as const;

export interface SwipeProps {
  /** The row's id: the offset goes back to 0 whenever the cell shows another row. */
  id: string;
  onArchive?: (id: string) => void;
  onDelete?: (id: string) => void;
  archiveIcon?: IconName;
  leftIcon?: IconName;
  /** The left action's tone: red for delete, the secondary container for read. */
  leftTone?: keyof typeof tones;
  children?: JSX.Element;
}

export function Swipe(props: SwipeProps) {
  const x = createAnimatedValue(0);
  let width = 0;
  const span = signal(360);
  const dragging = signal(false);
  const left = () => tones[props.leftTone ?? "error"];
  effect(() => {
    void props.id;
    x.set(0);
  });
  const end = (e: { value?: { x?: number }; velocity: { x: number } }) => {
    dragging.set(false);
    const at = (e.value?.x ?? 0) + e.velocity.x * 0.15;
    const act = width > 0 && Math.abs(at) > width * SWIPE_AT ? (at > 0 ? props.onArchive : props.onDelete) : undefined;
    if (!act) {
      void x.spring(0, { ...(material ? springs.spatial : {}), velocity: e.velocity.x });
      return;
    }
    const id = props.id;
    Haptics.impact("light");
    const away = at > 0 ? width : -width;
    if (reduceMotion()) act(id);
    else void x.timing(away, { duration: 180 }).then(() => act(id));
  };
  return (
    <If when={touch && (props.onArchive || props.onDelete)} else={props.children}>
      <If when={!material} else={materialSwipe()}>
        <View class="overflow-hidden" onLayout={({ bounds }) => (width = bounds.size.width)}>
          <View class="absolute inset-0 flex-row items-center justify-between bg-tertiary-container px-6" style={{ opacity: x.interpolate({ inputRange: [0, 1], outputRange: [0, 1] }) }}>
            <Icon name={props.archiveIcon ?? "archive"} tone="on-tertiary-container" />
          </View>
          <View class={`absolute inset-0 flex-row items-center justify-end px-6 ${left().container}`} style={{ opacity: x.interpolate({ inputRange: [-1, 0], outputRange: [1, 0] }) }}>
            <Icon name={props.leftIcon ?? "trash"} class={left().ink} />
          </View>
          <View pan={{ x }} onPanEnd={end} style={{ translate: [x, 0] }}>
            {props.children}
          </View>
        </View>
      </If>
    </If>
  );

  function materialSwipe() {
    const grow = (sign: 1 | -1) =>
      x.interpolate({
        inputRange: sign > 0 ? [0, span() * SWIPE_AT, span() * SWIPE_AT + 1] : [-span() * SWIPE_AT - 1, -span() * SWIPE_AT, 0],
        outputRange: sign > 0 ? [0.6, 1, 1.25] : [1.25, 1, 0.6],
      });
    return (
      <View
        class="mx-2 overflow-hidden rounded-large"
        onLayout={({ bounds }) => {
          width = bounds.size.width;
          span.set(bounds.size.width);
        }}
      >
        <View class="absolute inset-0 flex-row items-center bg-tertiary-container px-6" style={{ opacity: x.interpolate({ inputRange: [0, 8], outputRange: [0, 1] }) }}>
          <View style={{ transform: [{ scale: grow(1) }] }}>
            <Icon name={props.archiveIcon ?? "archive"} class="tint-on-tertiary-container" />
          </View>
        </View>
        <View class={`absolute inset-0 flex-row items-center justify-end px-6 ${left().container}`} style={{ opacity: x.interpolate({ inputRange: [-8, 0], outputRange: [1, 0] }) }}>
          <View style={{ transform: [{ scale: grow(-1) }] }}>
            <Icon name={props.leftIcon ?? "trash"} class={left().ink} />
          </View>
        </View>
        <View
          pan={{ x }}
          onPanStart={() => dragging.set(true)}
          onPanEnd={end}
          class={`overflow-hidden ${transition.fastSpatial} ${dragging() ? "rounded-extra-large" : "rounded-large"}`}
          style={{ translate: [x, 0] }}
        >
          {props.children}
        </View>
      </View>
    );
  }
}
