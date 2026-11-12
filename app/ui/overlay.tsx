// Overlays: one popup surface for menus and selects, a card dialog and an
// edge sheet over the scrim. Each grows in from where it belongs.
import { Text, View, type JSX } from "nsh";
import { Dialog as BaseDialog, Menu as BaseMenu, MenuItem as BaseMenuItem, MenuSeparator, Sheet as BaseSheet } from "nsh/base";
import { touch } from "./density";
import { Icon, type IconName } from "./icon";
import { IconButton } from "./button";

/** The surface every anchored popup shares. */
export const popup = "min-w-48 rounded-(--radius-lg) border border-border bg-popover shadow-lg";
const popIn = "opacity-0 scale-[0.97]";
const popOut = "opacity-0 scale-[0.97] duration-100";

export interface DialogProps {
  open: boolean;
  onClose: () => void;
  /** What screen readers call it. */
  label: string;
  /** The title bar's text; with `bare` the content draws its own header. */
  title?: string;
  bare?: boolean;
  /** Wide forms and the composer; default a short decision. */
  size?: "md" | "lg";
  children?: JSX.Element;
  testID?: string;
}

/** A centered card with a title and a close button; Escape and the scrim close it. */
export function Dialog(props: DialogProps) {
  return (
    <BaseDialog
      open={props.open}
      onOpenChange={(e) => !e.open && props.onClose()}
      accessibilityLabel={props.label}
      animation="fade"
      testID={props.testID}
      backdropClass="bg-scrim"
      class="flex-1 items-center justify-center p-6"
    >
      <View
        class={`max-h-full w-full overflow-hidden rounded-(--radius-lg) border border-border bg-background shadow-2xl ${props.size === "lg" ? "max-w-[760px] h-[640px]" : "max-w-[520px]"}`}
      >
        <If when={!props.bare}>
          <View class="h-12 flex-row items-center gap-2 border-b border-border pe-2 ps-4">
            <Text role="header" class="line-clamp-1 flex-1 text-sm font-semibold text-foreground">
              {props.title}
            </Text>
            <IconButton icon="close" label="Close" size="sm" onPress={props.onClose} />
          </View>
        </If>
        <View class="min-h-0 flex-1">{props.children}</View>
      </View>
    </BaseDialog>
  );
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
      backdropClass="bg-scrim"
      popupClass={
        side() === "bottom"
          ? "max-h-[85%] rounded-t-(--radius-lg) bg-background pb-[env(safe-area-inset-bottom)]"
          : "w-[300px] max-w-[85%] bg-sidebar pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]"
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
  destructive?: boolean;
  separatorBefore?: boolean;
  onSelect: () => void;
}

/** A visible trigger and its commands: a popup on desktops, a sheet on phones. */
export function Menu(props: { trigger: JSX.Element; actions: MenuAction[]; label: string }) {
  return (
    <BaseMenu
      offset={4}
      align="end"
      trigger={props.trigger}
      popupClass={`${popup} p-1`}
      popupStartingClass={popIn}
      popupEndingClass={popOut}
      backdropClass="bg-scrim"
    >
      <Each of={props.actions} key={(a) => a.id}>
        {(action) => (
          <>
            <If when={action().separatorBefore}>
              <MenuSeparator class="my-1 h-px bg-border" />
            </If>
            <BaseMenuItem
              onSelect={() => action().onSelect()}
              class={`flex-row items-center gap-2 rounded-(--radius) ${touch ? "min-h-11 px-3" : "h-8 px-2"} hover:bg-accent focus:bg-accent active:bg-pressed`}
            >
              <If when={action().icon}>
                {(icon) => <Icon name={icon()} size="sm" tone={action().destructive ? "destructive" : "foreground"} />}
              </If>
              <Text class={`${touch ? "text-base" : "text-sm"} ${action().destructive ? "text-destructive" : "text-popover-foreground"}`}>
                {action().label}
              </Text>
            </BaseMenuItem>
          </>
        )}
      </Each>
    </BaseMenu>
  );
}
