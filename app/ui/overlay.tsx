// M3E overlays on nsh/base: dialogs (surface-container-high, extra-large
// corners, over a 32% scrim), menus (large corners, items that round to
// medium and fill with tertiary-container when chosen), popovers and the
// modal side and bottom sheets. Anchored popups grow from their trigger on
// the fast spatial curve.
import { Keyboard, Text, View, type JSX } from "nsh";
import { t } from "nsh/i18n";
import {
  Dialog as BaseDialog,
  Menu as BaseMenu,
  MenuItem as BaseMenuItem,
  MenuSeparator,
  Popover as BasePopover,
  Sheet as BaseSheet,
} from "nsh/base";
import { touch } from "./density";
import { Icon, type IconName } from "./icon";
import { IconButton } from "./button";
import { popover } from "./motion";

/** The surface anchored popups share (Compose's expressive menu: surface-container-low, large corners, elevation 2). */
export const menuSurface = "rounded-large bg-surface-container-low p-1 shadow-elevation-2";

/** A menu or listbox item: 44pt (48 on touch), extra-small corners that round to medium under the pointer or keys. */
export const menuItem = `group flex-row items-center gap-3 rounded-extra-small px-3 ${touch ? "min-h-12" : "min-h-11"} text-body-large text-on-surface hover:rounded-medium hover:bg-on-surface/8 focus:rounded-medium focus:bg-on-surface/10 active:bg-on-surface/10 transition-all duration-150 ease-effects-fast motion-reduce:transition-none`;

export type ModalSize = "sm" | "md" | "lg" | "xl" | "2xl" | "3xl" | "full";

const modalWidth: Record<ModalSize, string> = {
  sm: "max-w-sm",
  md: "max-w-md",
  lg: "max-w-lg",
  xl: "max-w-xl",
  "2xl": "max-w-2xl",
  "3xl": "max-w-3xl",
  full: "max-w-full h-full !rounded-none",
};
const tall: ModalSize[] = ["2xl", "3xl"];

export interface DialogProps {
  open: boolean;
  onClose: () => void;
  /** What screen readers call it. */
  label: string;
  /** The headline; with `bare` the content draws its own. */
  title?: string;
  bare?: boolean;
  size?: ModalSize;
  /** The height the content needs; a 2xl or wider dialog without one is 640pt. Either way no taller than the window. */
  height?: number;
  children?: JSX.Element;
  testID?: string;
}

export function Dialog(props: DialogProps) {
  const size = () => props.size ?? "md";
  return (
    <BaseDialog
      open={props.open}
      onOpenChange={(e) => !e.open && props.onClose()}
      accessibilityLabel={props.label}
      animation="fade"
      testID={props.testID}
      backdropClass="bg-scrim/32"
      class={`flex-1 items-center ${touch ? "justify-end p-0" : "justify-center px-6 py-12"}`}
    >
      <View
        class={`relative max-h-full w-full overflow-hidden bg-surface-container-high shadow-elevation-3 ${touch ? "rounded-t-extra-large" : "rounded-extra-large"} ${modalWidth[size()]}`}
        style={{ height: props.height ?? (tall.includes(size()) ? 640 : undefined) }}
      >
        <If when={!props.bare}>
          <View class="flex-row items-center pb-4 pe-14 ps-6 pt-6">
            <Text role="header" class="line-clamp-2 flex-1 text-headline-small text-on-surface">
              {props.title}
            </Text>
          </View>
          <IconButton icon="close" label={t("common.close")} onPress={props.onClose} class="absolute end-3 top-4" testID="dialog-close" />
        </If>
        <View class="min-h-0 flex-1">{props.children}</View>
      </View>
    </BaseDialog>
  );
}

export function DialogBody(props: { children?: JSX.Element; class?: string }) {
  return <View class={`flex-1 gap-4 px-6 ${props.class ?? ""}`}>{props.children}</View>;
}

/** The actions, at the end, 8pt apart. */
export function DialogFooter(props: { children?: JSX.Element; class?: string }) {
  return <View class={`flex-row justify-end gap-2 px-6 pb-6 pt-6 ${props.class ?? ""}`}>{props.children}</View>;
}

export interface SheetProps {
  open: boolean;
  onClose: () => void;
  side?: "left" | "bottom" | "right";
  label: string;
  children?: JSX.Element;
}

/** A modal sheet: from the start edge (16pt inner corners) or from the bottom (28pt top corners and a drag handle). */
export function Sheet(props: SheetProps) {
  const side = () => props.side ?? "left";
  return (
    <BaseSheet
      open={props.open}
      onOpenChange={(e) => !e.open && props.onClose()}
      side={side()}
      closeOnSwipe={side() === "bottom"}
      backdropClass="bg-scrim/32"
      popupClass={
        side() === "bottom"
          ? "max-h-[85%] rounded-t-extra-large bg-surface-container-low pb-[env(safe-area-inset-bottom)] shadow-elevation-1"
          : `w-[320px] max-w-[85%] bg-surface-container-low pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)] shadow-elevation-1 ${side() === "left" ? "rounded-e-large" : "rounded-s-large"}`
      }
    >
      <If when={side() === "bottom"}>
        <View accessibilityElementsHidden class="items-center py-4">
          <View class="h-1 w-8 rounded-full bg-on-surface-variant/40" />
        </View>
      </If>
      {props.children}
    </BaseSheet>
  );
}

export interface MenuAction {
  id: string;
  label: string;
  icon?: IconName;
  description?: string;
  shortcut?: string;
  destructive?: boolean;
  selected?: boolean;
  separatorBefore?: boolean;
  onSelect: () => void;
}

/** A menu at its trigger: a popup on desktops, a sheet on phones (nsh/base decides). */
export function Menu(props: { trigger: JSX.Element; actions: MenuAction[]; label: string; align?: "start" | "center" | "end" }) {
  return (
    <BaseMenu
      offset={4}
      align={props.align ?? "end"}
      trigger={props.trigger}
      popupClass={`${menuSurface} min-w-[200px] ${popover.class}`}
      popupStartingClass={popover.starting}
      popupEndingClass={popover.ending}
      backdropClass="bg-scrim/32"
      // A phone's sheet would open under the software keyboard.
      onOpenChange={(e: { open: boolean }) => e.open && touch && Keyboard.dismiss()}
    >
      <Each of={props.actions} key={(a) => a.id}>
        {(action) => (
          <>
            <If when={action().separatorBefore}>
              <MenuSeparator class="mx-3 my-1 h-px bg-outline-variant" />
            </If>
            <BaseMenuItem
              onSelect={() => action().onSelect()}
              class={`${menuItem} ${action().selected ? "rounded-medium bg-tertiary-container text-on-tertiary-container" : action().destructive ? "text-error" : ""}`}
            >
              <If when={action().icon}>
                {(icon) => (
                  <Icon
                    name={icon()}
                    tone={action().selected ? "on-tertiary-container" : action().destructive ? "error" : "on-surface-variant"}
                  />
                )}
              </If>
              <View class="min-w-0 flex-1">
                <Text class="text-body-large">{action().label}</Text>
                <If when={action().description}>
                  <Text class="text-body-medium text-on-surface-variant">{action().description}</Text>
                </If>
              </View>
              <If when={action().shortcut}>
                <Text class="text-label-small tracking-label-small text-on-surface-variant">{action().shortcut}</Text>
              </If>
            </BaseMenuItem>
          </>
        )}
      </Each>
    </BaseMenu>
  );
}

/** Content anchored to a trigger: the same surface as menus. */
export function Popover(props: {
  trigger: JSX.Element;
  children?: JSX.Element;
  label: string;
  placement?: "top" | "bottom" | "left" | "right";
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  return (
    <BasePopover
      offset={6}
      placement={props.placement ?? "bottom"}
      open={props.open}
      onOpenChange={(e: { open: boolean }) => props.onOpenChange?.(e.open)}
      trigger={<View accessibilityLabel={props.label}>{props.trigger}</View>}
      popupClass={`${menuSurface} px-3 py-2 ${popover.class}`}
      popupStartingClass={popover.starting}
      popupEndingClass={popover.ending}
    >
      {props.children}
    </BasePopover>
  );
}
