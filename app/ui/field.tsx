// HeroUI's Input and Textarea (flat, faded, bordered, underlined), the field
// around them (label, description, error) and a Select whose trigger is a flat
// input and whose popup is a listbox.
import { Text, View, type InputRef, type JSX } from "nsh";
import { Input, Select } from "nsh/base";
import { touch } from "./density";
import { Icon, type IconName } from "./icon";
import { listItem, popup } from "./overlay";
import { wash } from "./motion";
import { rounded, type Radius, type Size } from "./variants";
import { material } from "./material";

/** HeroUI's four, and `plain`: no frame, for a composer's body, where the caret shows the focus. */
export type InputVariant = "flat" | "faded" | "bordered" | "underlined" | "plain";

const wrapper: Record<InputVariant, string> = {
  // Android: Material's filled field and search bar, whose caret shows the focus.
  flat: material ? "bg-surface-container-high" : "bg-surface-container-high hover:bg-surface-container-highest shadow-xs focus-within:outline-2 focus-within:outline-secondary focus-within:outline-offset-2",
  faded: "border-2 border-outline-variant bg-surface-container-high hover:border-outline focus-within:border-outline shadow-xs focus-within:outline-2 focus-within:outline-secondary focus-within:outline-offset-2",
  bordered: "border-2 border-outline-variant hover:border-outline focus-within:border-on-surface shadow-xs",
  underlined: "border-b-2 border-outline-variant hover:border-outline-variant focus-within:border-on-surface",
  plain: "",
};
const invalidWrapper: Record<InputVariant, string> = {
  flat: "!bg-error-container hover:!bg-error-container",
  faded: "!border-error",
  bordered: "!border-error",
  underlined: "!border-error",
  plain: "",
};
const height: Record<Size, string> = { sm: "h-8 px-2", md: "h-10 px-3", lg: "h-12 px-3" };
const text = (size: Size) => (size === "lg" || touch ? "text-body-large" : "text-body-medium");
const corner: Record<Size, Radius> = { sm: "sm", md: "md", lg: "lg" };

const box = (variant: InputVariant, size: Size, radius: Radius | undefined, invalid: boolean | undefined, multiline: boolean) =>
  `flex-row gap-2 ${multiline ? "items-start py-2 px-3" : `items-center ${height[size]}`} ${variant === "underlined" ? "rounded-none !px-1" : rounded[radius ?? corner[size]]} ${wrapper[variant]} ${invalid ? invalidWrapper[variant] : ""} ${wash}`;

export interface TextFieldProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit?: () => void;
  placeholder?: string;
  masked?: boolean;
  invalid?: boolean;
  disabled?: boolean;
  multiline?: boolean;
  autoFocus?: boolean;
  kind?: "text" | "email" | "url" | "number";
  /** The accessible name when no visible label is next to it. */
  label?: string;
  returnKey?: "next" | "done" | "go" | "search" | "send";
  variant?: InputVariant;
  size?: Size;
  radius?: Radius;
  /** HeroUI's startContent / endContent. */
  startIcon?: IconName;
  end?: JSX.Element;
  ref?: (ref: InputRef) => void;
  /** Hears a paste: images and copied files come as `files` and `uris`; text goes in as usual. */
  onPaste?: (e: { files: File[]; uris: string[] }) => void;
  /** Layout of the field from the caller: width, flex, margin. */
  class?: string;
  testID?: string;
}

/** HeroUI's Input; `multiline` makes it the Textarea. */
export function TextField(props: TextFieldProps) {
  return field(props, !!props.multiline);
}

/** HeroUI's Textarea: the input over several lines. */
export function Textarea(props: Omit<TextFieldProps, "multiline">) {
  return field(props, true);
}

// One body for both, without spreading props: a spread would copy `ref`'s getter.
function field(props: TextFieldProps, multiline: boolean) {
  const keyboard = { email: "email-address", url: "url", number: "number-pad", text: "default" } as const;
  const size = () => props.size ?? "md";
  return (
    <View
      class={`${box(props.variant ?? "flat", size(), props.radius, props.invalid, multiline)} ${props.disabled ? "opacity-50" : ""} ${props.class ?? ""}`}
    >
      <If when={props.startIcon}>
        {(icon) => <Icon name={icon()} size="sm" tone={props.invalid ? "error" : "on-surface-variant"} />}
      </If>
      <Input
        ref={props.ref}
        value={props.value}
        onChange={({ value }) => props.onChange(value)}
        onSubmit={() => props.onSubmit?.()}
        onPaste={props.onPaste && ((e: { files: File[]; uris: string[] }) => props.onPaste!(e))}
        placeholder={props.placeholder}
        masked={props.masked}
        multiline={multiline}
        autoFocus={props.autoFocus}
        readonly={props.disabled}
        keyboardType={keyboard[props.kind ?? "text"]}
        autoCapitalize={props.kind && props.kind !== "text" ? "none" : "sentences"}
        autoCorrect={!props.kind || props.kind === "text"}
        returnKeyType={props.returnKey}
        accessibilityLabel={props.label}
        testID={props.testID}
        class={`min-w-0 flex-1 self-stretch bg-transparent ${text(size())} ${props.invalid ? "text-error placeholder:text-error" : "text-on-surface placeholder:text-on-surface-variant"} selection:bg-primary/30`}
      />
      {props.end}
    </View>
  );
}

/** A label over its control, and the help or error under it. */
export function Field(props: { label: string; description?: string; error?: string | null; required?: boolean; children?: JSX.Element; class?: string }) {
  return (
    <View class={`gap-1.5 ${props.class ?? ""}`}>
      <Text class="text-body-medium text-on-surface">
        {props.label}
        <If when={props.required}>
          <Text class="text-error"> *</Text>
        </If>
      </Text>
      {props.children}
      <If when={props.error}>
        <Text class="px-1 text-body-small text-error">{props.error}</Text>
      </If>
      <If when={!props.error && props.description}>
        <Text class="px-1 text-body-small text-on-surface-variant">{props.description}</Text>
      </If>
    </View>
  );
}

export interface SelectFieldProps {
  value: string | null;
  items: { value: string; title: string }[];
  onChange: (value: string) => void;
  label: string;
  placeholder?: string;
  size?: Size;
  class?: string;
  testID?: string;
}

/** HeroUI's Select: a popup listbox at the trigger on desktops, a sheet on phones. */
export function SelectField(props: SelectFieldProps) {
  const title = () => props.items.find((i) => i.value === props.value)?.title;
  const size = () => props.size ?? "md";
  return (
    <Select
      value={props.value}
      items={props.items}
      onChange={(e: { value: string | null }) => e.value && props.onChange(e.value)}
      accessibilityLabel={props.label}
      testID={props.testID}
      offset={4}
      popupClass={`${popup} p-1`}
      backdropClass="bg-scrim/32"
      itemClass={`${listItem} justify-center selected:text-primary`}
      class={props.class}
      trigger={
        <View class={`${box("flat", size(), undefined, false, false)} justify-between aria-expanded:bg-surface-container-highest`}>
          <Text class={`line-clamp-1 flex-1 ${text(size())} ${title() ? "text-on-surface" : "text-on-surface-variant"}`}>
            {title() ?? props.placeholder ?? ""}
          </Text>
          <Icon name="chevronDown" tone="on-surface-variant" size="sm" />
        </View>
      }
    />
  );
}
