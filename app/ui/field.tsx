// HeroUI's Input and Textarea (flat, faded, bordered, underlined), the field
// around them (label, description, error) and a Select whose trigger is a flat
// input and whose popup is a listbox.
import { Text, View, type InputRef, type JSX } from "nsh";
import { Input, Select } from "nsh/base";
import { touch } from "./density";
import { Icon, type IconName } from "./icon";
import { listItem, popup } from "./overlay";
import { wash } from "./motion";
import { rounded, type Radius, type Size } from "./tokens";

export type InputVariant = "flat" | "faded" | "bordered" | "underlined";

const wrapper: Record<InputVariant, string> = {
  flat: "bg-default-100 hover:bg-default-200 shadow-xs focus-within:outline-2 focus-within:outline-focus focus-within:outline-offset-2",
  faded: "border-2 border-default-200 bg-default-100 hover:border-default-400 focus-within:border-default-400 shadow-xs focus-within:outline-2 focus-within:outline-focus focus-within:outline-offset-2",
  bordered: "border-2 border-default-200 hover:border-default-400 focus-within:border-default-foreground shadow-xs",
  underlined: "border-b-2 border-default-200 hover:border-default-300 focus-within:border-default-foreground",
};
const invalidWrapper: Record<InputVariant, string> = {
  flat: "!bg-danger-50 hover:!bg-danger-100",
  faded: "!border-danger",
  bordered: "!border-danger",
  underlined: "!border-danger",
};
const height: Record<Size, string> = { sm: "h-8 px-2", md: "h-10 px-3", lg: "h-12 px-3" };
const text = (size: Size) => (size === "lg" || touch ? "text-medium" : "text-small");
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
        {(icon) => <Icon name={icon()} size="sm" tone={props.invalid ? "danger" : "default-400"} />}
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
        class={`min-w-0 flex-1 self-stretch bg-transparent ${text(size())} ${props.invalid ? "text-danger placeholder:text-danger" : "text-foreground placeholder:text-foreground-500"} selection:bg-primary/30`}
      />
      {props.end}
    </View>
  );
}

/** A label over its control, and the help or error under it. */
export function Field(props: { label: string; description?: string; error?: string | null; required?: boolean; children?: JSX.Element; class?: string }) {
  return (
    <View class={`gap-1.5 ${props.class ?? ""}`}>
      <Text class="text-small text-foreground">
        {props.label}
        <If when={props.required}>
          <Text class="text-danger"> *</Text>
        </If>
      </Text>
      {props.children}
      <If when={props.error}>
        <Text class="px-1 text-tiny text-danger">{props.error}</Text>
      </If>
      <If when={!props.error && props.description}>
        <Text class="px-1 text-tiny text-foreground-400">{props.description}</Text>
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
      backdropClass="bg-overlay/50"
      itemClass={`${listItem} justify-center selected:text-primary`}
      class={props.class}
      trigger={
        <View class={`${box("flat", size(), undefined, false, false)} justify-between aria-expanded:bg-default-200`}>
          <Text class={`line-clamp-1 flex-1 ${text(size())} ${title() ? "text-foreground" : "text-foreground-500"}`}>
            {title() ?? props.placeholder ?? ""}
          </Text>
          <Icon name="chevronDown" tone="default-500" size="sm" />
        </View>
      }
    />
  );
}
