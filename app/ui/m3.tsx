// Material 3 Expressive components NSH has no native counterpart for, drawn from its
// primitives to the M3E specs (m3.material.io; sizes from Compose Material3's tokens):
// the loading indicator, the short navigation bar, the FAB menu, the floating toolbar
// and the top app bar of a tab's first page. Every platform draws them.
import { createAnimatedValue, Image, reduceMotion, Shader, shadersAvailable, Text, token, View, type AnimatedNode, type JSX } from "nsh";
import { Icon, type IconName } from "./icon";
import { tip } from "./tooltip";
import type { ColorRole } from "./tokens";
import { springs, transition } from "./material";

// The loading indicator's shapes from the M3E shape library, as polar outlines: a cookie of
// `lobes` lobes (x, y) or an ellipse (z, w). In the spec's order: soft burst, 9-sided cookie,
// pentagon, pill, sunny, 4-sided cookie, oval; each morphs into the next while it turns.
const INDICATOR = /* wgsl */ `
struct Uniforms { color: vec4f, progress: f32 }

var<private> SHAPES: array<vec4f, 7> = array<vec4f, 7>(
  vec4f(10.0, 0.10, 0.0, 0.0),
  vec4f(9.0, 0.07, 0.0, 0.0),
  vec4f(5.0, 0.11, 0.0, 0.0),
  vec4f(0.0, 0.0, 0.95, 0.62),
  vec4f(8.0, 0.05, 0.0, 0.0),
  vec4f(4.0, 0.14, 0.0, 0.0),
  vec4f(0.0, 0.0, 0.92, 0.76),
);

fn outline(s: vec4f, a: f32) -> f32 {
  if (s.x > 0.0) {
    return 0.92 * (1.0 + s.y * cos(s.x * a)) / (1.0 + s.y);
  }
  let c = s.w * cos(a);
  let n = s.z * sin(a);
  return s.z * s.w / sqrt(c * c + n * n);
}

fn shade(uv: vec2f) -> vec4f {
  let p = (uv - vec2f(0.5)) * 2.0;
  let r = length(p);
  let a = atan2(p.y, p.x);
  var edge: f32;
  if (uniforms.progress >= 0.0) {
    let k = clamp(uniforms.progress, 0.0, 1.0);
    edge = outline(SHAPES[0], a - k * 3.1416) * mix(0.25, 1.0, k);
  } else {
    let t = nsh.time / 0.65;
    let i = u32(floor(t)) % 7u;
    let f = fract(t);
    let e = f * f * (3.0 - 2.0 * f);
    let turn = nsh.time * 2.6 + (floor(t) + e) * 1.4;
    edge = mix(outline(SHAPES[i], a - turn), outline(SHAPES[(i + 1u) % 7u], a - turn), e);
  }
  let aa = 3.0 / max(nsh.size.x * nsh.scale, 1.0);
  let alpha = clamp((edge - r) / aa + 0.5, 0.0, 1.0);
  return vec4f(uniforms.color.rgb, uniforms.color.a * alpha);
}`;

const cookie = `data:image/svg+xml,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="-50 -50 100 100"><path fill="#000" d="${Array.from({ length: 73 }, (_, i) => {
    const a = (i / 72) * Math.PI * 2;
    const r = (46 * (1 + 0.07 * Math.cos(9 * a))) / 1.07;
    return `${i ? "L" : "M"}${(r * Math.cos(a)).toFixed(2)} ${(r * Math.sin(a)).toFixed(2)}`;
  }).join("")}Z"/></svg>`,
)}`;

/** M3E's loading indicator: a shape morphing through the shape library while it turns (indeterminate),
 * or growing and turning with `progress` (0..1, a pull). `contained` puts it in a primary-container circle.
 * Where the platform draws no shaders, a turning 9-sided cookie. */
export function LoadingIndicator(props: { label: string; contained?: boolean; size?: number; progress?: AnimatedNode | number; tone?: ColorRole; testID?: string }) {
  const size = () => props.size ?? 48;
  const color = () => token(props.tone ?? (props.contained ? "on-primary-container" : "primary"));
  return (
    <View
      role="progressbar"
      accessibilityLabel={props.label}
      testID={props.testID}
      class={`items-center justify-center rounded-full ${props.contained ? "bg-primary-container" : ""}`}
      style={{ width: size(), height: size() }}
    >
      <If
        when={shadersAvailable()}
        else={
          <Image
            src={cookie}
            accessibilityElementsHidden
            style={{
              width: size() * 0.79,
              height: size() * 0.79,
              tintColor: color(),
              animationName: reduceMotion() ? undefined : "spin",
              animationDuration: 1300,
              animationIterationCount: "infinite",
              animationTimingFunction: "linear",
            }}
          />
        }
      >
        <Shader
          source={INDICATOR}
          paused={reduceMotion()}
          uniforms={{ color: color(), progress: props.progress ?? -1 }}
          style={{ width: size() * 0.79, height: size() * 0.79, pointerEvents: "none" }}
        />
      </If>
    </View>
  );
}

export interface NavBarItem {
  title: string;
  icon: IconName;
  /** A count on the icon (Material's large badge); none when empty. */
  badge?: string;
  testID?: string;
}

/** M3E's short navigation bar: 64dp on Material's surface container, each destination an icon in a
 * pill-shaped active indicator over its label; the indicator grows from the icon on the fast spatial spring. */
export function NavBar(props: { items: NavBarItem[]; selected: number; onChange: (index: number) => void; label: string }) {
  return (
    <View role="tablist" accessibilityLabel={props.label} class="bg-surface-container pb-[env(safe-area-inset-bottom)]">
      <View class="h-16 flex-row">
        <Each of={props.items} key={(item) => item.title}>
          {(item, index) => {
            const on = () => props.selected === index();
            return (
              <View
                role="tab"
                accessibilityLabel={item().badge ? `${item().title}, ${item().badge}` : item().title}
                accessibilityState={{ selected: on() }}
                onPress={() => props.onChange(index())}
                testID={item().testID}
                class="flex-1 items-center justify-center gap-1 pt-1.5 pb-2"
              >
                <View class="h-8 w-14 items-center justify-center" android={{ ripple: { color: `${token("on-surface").slice(0, 7)}1f`, borderless: true, radius: 28 } }}>
                  <View class={`absolute inset-0 rounded-full bg-secondary-container ${transition.fastSpatial} ${on() ? "scale-100 opacity-100" : "scale-x-50 opacity-0"}`} />
                  <Icon name={item().icon} class={`${transition.fastEffects} ${on() ? "tint-on-secondary-container" : "tint-on-surface-variant"}`} />
                  <If when={item().badge}>
                    <View class="absolute top-0.5 left-[30px] min-h-4 min-w-4 items-center justify-center rounded-full bg-error px-1">
                      <Text class="text-[11px] font-medium leading-4 text-on-error">{item().badge}</Text>
                    </View>
                  </If>
                </View>
                <Text class={`text-label-medium ${on() ? "font-bold text-secondary" : "font-medium text-on-surface-variant"}`}>{item().title}</Text>
              </View>
            );
          }}
        </Each>
      </View>
    </View>
  );
}

export interface FabMenuItem {
  icon: IconName;
  label: string;
  onPress: () => void;
  testID?: string;
}

/** M3E's FAB menu: the FAB (56dp, `shape-lg`, primary container) morphs into a round close button in
 * primary as the menu's items (56dp pills) spring up from it, one after the other. */
export function FabMenu(props: { icon: IconName; label: string; closeLabel: string; items: FabMenuItem[]; open: boolean; onOpenChange: (open: boolean) => void; testID?: string }) {
  const shown = props.items.map(() => createAnimatedValue(0));
  const toggle = (open: boolean) => {
    props.onOpenChange(open);
    const order = open ? [...shown].reverse() : shown;
    order.forEach((value, i) => {
      const run = () => (reduceMotion() ? value.set(open ? 1 : 0) : void value.spring(open ? 1 : 0, open ? springs.fastSpatial : springs.fastEffects));
      if (i === 0 || !open) run();
      else setTimeout(run, i * 40);
    });
  };
  return (
    <View class="items-end gap-1" style={{ pointerEvents: "box-none" }}>
      <Each of={props.items.map((item, index) => ({ item, index }))} key={(x) => x.item.label}>
        {(x) => {
          const value = shown[x().index];
          return (
            <View
              role="menuitem"
              accessibilityLabel={x().item.label}
              accessibilityElementsHidden={!props.open}
              onPress={() => {
                toggle(false);
                x().item.onPress();
              }}
              testID={x().item.testID}
              android={{ ripple: { color: `${token("on-primary-container").slice(0, 7)}1f` }, elevation: 3 }}
              class="h-14 flex-row items-center gap-2 overflow-hidden rounded-full bg-primary-container ps-6 pe-6"
              style={{
                pointerEvents: props.open ? "auto" : "none",
                opacity: value,
                transform: [{ translateY: value.interpolate({ inputRange: [0, 1], outputRange: [24, 0] }) }, { scale: value.interpolate({ inputRange: [0, 1], outputRange: [0.6, 1] }) }],
              }}
            >
              <Icon name={x().item.icon} class="tint-on-primary-container" />
              <Text class="text-title-medium font-medium text-on-primary-container">{x().item.label}</Text>
            </View>
          );
        }}
      </Each>
      <View
        role="button"
        accessibilityLabel={props.open ? props.closeLabel : props.label}
        accessibilityState={{ expanded: props.open }}
        onPress={() => toggle(!props.open)}
        testID={props.testID}
        android={{ ripple: { color: `${token("on-primary-container").slice(0, 7)}1f` }, elevation: 3 }}
        class={`mt-1 size-14 items-center justify-center overflow-hidden ${transition.fastSpatial} ${props.open ? "rounded-full bg-primary" : "rounded-large bg-primary-container active:rounded-medium"}`}
      >
        <Icon name={props.open ? "close" : props.icon} class={props.open ? "tint-on-primary" : "tint-on-primary-container"} />
      </View>
    </View>
  );
}

/** M3E's floating toolbar: a 64dp pill of actions floating 16dp over the content. `vibrant` puts it in
 * the primary container. */
export function FloatingToolbar(props: { label: string; vibrant?: boolean; children?: JSX.Element; testID?: string }) {
  return (
    <View
      role="toolbar"
      accessibilityLabel={props.label}
      testID={props.testID}
      android={{ elevation: 3 }}
      class={`h-16 flex-row items-center gap-1 self-center rounded-full px-2 shadow-elevation-3 android:shadow-none ${props.vibrant ? "bg-primary-container" : "bg-surface-container"}`}
    >
      {props.children}
    </View>
  );
}

/** A floating toolbar's action: 48dp, round, squaring up while pressed; `emphasized` is the toolbar's
 * selected button (a surface-container circle in a vibrant toolbar). */
export function ToolbarButton(props: { icon: IconName; label: string; onPress: () => void; emphasized?: boolean; vibrant?: boolean; testID?: string }) {
  const ink = () => (props.emphasized ? "tint-on-surface" : props.vibrant ? "tint-on-primary-container" : "tint-on-surface-variant");
  return (
    <View
      role="button"
      accessibilityLabel={props.label}
      tooltip={tip(props.label)}
      onPress={props.onPress}
      testID={props.testID}
      android={{ ripple: { color: `${token("on-surface").slice(0, 7)}1f` } }}
      class={`size-12 items-center justify-center overflow-hidden rounded-full ${transition.fastSpatial} active:rounded-medium ${props.emphasized ? (props.vibrant ? "bg-surface-container" : "bg-secondary-container") : ""}`}
    >
      <Icon name={props.icon} class={ink()} />
    </View>
  );
}

/** The top app bar of a tab's first page: an emphasized headline under the status bar, with a search
 * bar above it (`search`) and actions at its end. */
export function AppBar(props: { title: string; search?: { label: string; onPress: () => void; end?: JSX.Element }; actions?: JSX.Element; leading?: JSX.Element; testID?: string }) {
  return (
    <View class="bg-surface pt-[env(safe-area-inset-top)]" testID={props.testID}>
      <If when={props.search}>
        {(search) => (
          <View class="mx-4 mt-2 h-14 flex-row items-center gap-1 rounded-full bg-surface-container-high ps-4 pe-1">
            <View role="search" accessibilityLabel={search().label} onPress={search().onPress} class="h-14 flex-1 flex-row items-center gap-3" testID="search-bar">
              <Icon name="search" class="tint-on-surface-variant" />
              <Text class="flex-1 text-body-large text-on-surface-variant">{search().label}</Text>
            </View>
            {search().end}
          </View>
        )}
      </If>
      <View class={`min-h-16 flex-row items-center gap-1 ps-4 pe-2 ${props.search ? "pt-3" : "pt-2"}`}>
        {props.leading}
        <Text role="header" class="line-clamp-1 flex-1 text-headline-medium font-medium text-on-surface">
          {props.title}
        </Text>
        {props.actions}
      </View>
    </View>
  );
}
