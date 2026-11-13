// HeroUI's overlays on nsh/base: Modal (Dialog), Dropdown (Menu), Popover,
// Tooltip and the edge Sheet. Anchored popups share one content1 surface,
// spring in from 0.8 and leave at 0.96; modals fade over overlay/50.
import { Text, View, type JSX } from "nsh";
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
import { popover, wash } from "./motion";
import { shadow } from "./tokens";

/** The surface every anchored popup shares: content1, large radius, medium shadow. */
export const popup = `min-w-[200px] rounded-large bg-content1 ${shadow.md}`;

/** A listbox or dropdown item: HeroUI's menu item, solid default (hover and keyboard focus fill with `default`). */
export const listItem = `flex-row items-center gap-2 rounded-small px-2 ${touch ? "min-h-11" : "min-h-8 py-1.5"} text-small text-foreground hover:bg-default hover:text-default-foreground focus:bg-default active:bg-default ${wash}`;

export type ModalSize = "xs" | "sm" | "md" | "lg" | "xl" | "2xl" | "3xl" | "4xl" | "5xl" | "full";

const modalWidth: Record<ModalSize, string> = {
  xs: "max-w-xs",
  sm: "max-w-sm",
  md: "max-w-md",
  lg: "max-w-lg",
  xl: "max-w-xl",
  "2xl": "max-w-2xl",
  "3xl": "max-w-3xl",
  "4xl": "max-w-4xl",
  "5xl": "max-w-5xl",
  full: "max-w-full h-full !rounded-none",
};
const tall: ModalSize[] = ["2xl", "3xl", "4xl", "5xl"];

export interface DialogProps {
  open: boolean;
  onClose: () => void;
  /** What screen readers call it. */
  label: string;
  /** HeroUI's ModalHeader text; with `bare` the content draws its own header. */
  title?: string;
  bare?: boolean;
  /** HeroUI's modal sizes; default `md` (448pt). */
  size?: ModalSize;
  /** The height the content needs; a 2xl or wider modal without one is 640pt. Either way no taller than the window. */
  height?: number;
  children?: JSX.Element;
  testID?: string;
}

/** HeroUI's Modal: a content1 card over overlay/50; Escape and the backdrop close it. */
export function Dialog(props: DialogProps) {
  const size = () => props.size ?? "md";
  return (
    <BaseDialog
      open={props.open}
      onOpenChange={(e) => !e.open && props.onClose()}
      accessibilityLabel={props.label}
      animation="fade"
      testID={props.testID}
      backdropClass="bg-overlay/50"
      class={`flex-1 items-center ${touch ? "justify-end p-1" : "justify-center px-6 py-16"}`}
    >
      <View
        class={`relative max-h-full w-full overflow-hidden rounded-large bg-content1 ${shadow.sm} ${modalWidth[size()]}`}
        style={{ height: props.height ?? (tall.includes(size()) ? 640 : undefined) }}
      >
        <If when={!props.bare}>
          <View class="flex-row items-center py-4 pe-12 ps-6">
            <Text role="header" class="line-clamp-1 flex-1 text-large font-semibold text-foreground">
              {props.title}
            </Text>
          </View>
          <IconButton icon="close" label="Close" size="sm" radius="full" onPress={props.onClose} class="absolute end-1 top-1" />
        </If>
        <View class="min-h-0 flex-1">{props.children}</View>
      </View>
    </BaseDialog>
  );
}

/** HeroUI's ModalBody: the content, padded and spaced. */
export function DialogBody(props: { children?: JSX.Element; class?: string }) {
  return <View class={`flex-1 gap-3 px-6 py-2 ${props.class ?? ""}`}>{props.children}</View>;
}

/** HeroUI's ModalFooter: actions at the end. */
export function DialogFooter(props: { children?: JSX.Element; class?: string }) {
  return <View class={`flex-row justify-end gap-2 px-6 py-4 ${props.class ?? ""}`}>{props.children}</View>;
}

export interface SheetProps {
  open: boolean;
  onClose: () => void;
  side?: "left" | "bottom" | "right";
  label: string;
  children?: JSX.Element;
}

/** A panel at an edge: the mailbox list on phones. */
export function Sheet(props: SheetProps) {
  const side = () => props.side ?? "left";
  return (
    <BaseSheet
      open={props.open}
      onOpenChange={(e) => !e.open && props.onClose()}
      side={side()}
      closeOnSwipe={side() === "bottom"}
      backdropClass="bg-overlay/50"
      popupClass={
        side() === "bottom"
          ? `max-h-[85%] rounded-t-large bg-content1 pb-[env(safe-area-inset-bottom)] ${shadow.lg}`
          : `w-[300px] max-w-[85%] bg-sidebar pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)] ${shadow.lg}`
      }
    >
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
  separatorBefore?: boolean;
  onSelect: () => void;
}

/** HeroUI's Dropdown: a trigger and its commands; a popup on desktops, a sheet on phones. */
export function Menu(props: { trigger: JSX.Element; actions: MenuAction[]; label: string; align?: "start" | "center" | "end" }) {
  return (
    <BaseMenu
      offset={4}
      align={props.align ?? "end"}
      trigger={props.trigger}
      popupClass={`${popup} p-1 ${popover.class}`}
      popupStartingClass={popover.starting}
      popupEndingClass={popover.ending}
      backdropClass="bg-overlay/50"
    >
      <Each of={props.actions} key={(a) => a.id}>
        {(action) => (
          <>
            <If when={action().separatorBefore}>
              <MenuSeparator class="mx-2 my-1 h-px bg-divider" />
            </If>
            <BaseMenuItem
              onSelect={() => action().onSelect()}
              class={`group ${listItem} ${action().destructive ? "text-danger hover:bg-danger hover:text-danger-foreground focus:bg-danger focus:text-danger-foreground" : ""}`}
            >
              <If when={action().icon}>
                {(icon) => (
                  <Icon
                    name={icon()}
                    size="sm"
                    class={action().destructive ? "tint-danger group-hover:tint-danger-foreground group-focus:tint-danger-foreground" : "tint-default-500 group-hover:tint-default-foreground"}
                  />
                )}
              </If>
              <View class="min-w-0 flex-1">
                <Text class="text-small">{action().label}</Text>
                <If when={action().description}>
                  <Text class="text-tiny text-foreground-500 group-hover:text-current">{action().description}</Text>
                </If>
              </View>
              <If when={action().shortcut}>
                <Text class="rounded-sm border border-default-300 px-1 py-0.5 text-tiny text-foreground-500">{action().shortcut}</Text>
              </If>
            </BaseMenuItem>
          </>
        )}
      </Each>
    </BaseMenu>
  );
}

/** HeroUI's Popover: content1 content anchored to a trigger; the trigger dims while it's open. */
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
      trigger={<View accessibilityLabel={props.label} class="aria-expanded:scale-[0.97] aria-expanded:opacity-70 transition">{props.trigger}</View>}
      popupClass={`${popup} px-2.5 py-1 ${popover.class}`}
      popupStartingClass={popover.starting}
      popupEndingClass={popover.ending}
    >
      {props.children}
    </BasePopover>
  );
}
