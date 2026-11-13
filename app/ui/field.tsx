// Text fields, a labelled field and a select, sharing one height, padding
// and focus edge per size.
import { Text, View, type InputRef, type JSX } from "nsh";
import { Input, Select } from "nsh/base";
import { touch } from "./density";
import { Icon } from "./icon";
import { popup } from "./overlay";
import { wash } from "./motion";

const well = touch
  ? "min-h-11 rounded-(--radius) border border-input bg-background px-3 text-base text-foreground"
  : "h-8 rounded-(--radius) border border-input bg-background px-2.5 text-sm text-foreground";
const focus = "focus:border-ring focus:outline-1 focus:outline-ring";

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
  ref?: (ref: InputRef) => void;
  /** Hears a paste: images and copied files come as `files` and `uris`; text goes in as usual. */
  onPaste?: (e: { files: File[]; uris: string[] }) => void;
  class?: string;
  testID?: string;
}

export function TextField(props: TextFieldProps) {
  const keyboard = () =>
    ({ email: "email-address", url: "url", number: "number-pad", text: "default" }) as const;
  return (
    <Input
      ref={props.ref}
      value={props.value}
      onChange={({ value }) => props.onChange(value)}
      onSubmit={() => props.onSubmit?.()}
      onPaste={props.onPaste && ((e: { files: File[]; uris: string[] }) => props.onPaste!(e))}
      placeholder={props.placeholder}
      masked={props.masked}
      multiline={props.multiline}
      autoFocus={props.autoFocus}
      readonly={props.disabled}
      keyboardType={keyboard()[props.kind ?? "text"]}
      autoCapitalize={props.kind && props.kind !== "text" ? "none" : "sentences"}
      autoCorrect={!props.kind || props.kind === "text"}
      returnKeyType={props.returnKey}
      accessibilityLabel={props.label}
      testID={props.testID}
      class={`${props.multiline ? "rounded-(--radius) border border-input bg-background text-foreground " + (touch ? "p-3 text-base" : "px-2.5 py-2 text-sm") : well} ${focus} placeholder:text-muted-foreground ${wash} ${props.invalid ? "border-destructive" : ""} ${props.class ?? ""}`}
    />
  );
}

/** A label, its control, and the help or error under it. */
export function Field(props: { label: string; description?: string; error?: string | null; children?: JSX.Element; class?: string }) {
  return (
    <View class={`gap-1.5 ${props.class ?? ""}`}>
      <Text class={`font-medium text-foreground ${touch ? "text-sm" : "text-xs"}`}>{props.label}</Text>
      {props.children}
      <If when={props.error}>
        <Text class={`text-destructive ${touch ? "text-sm" : "text-xs"}`}>{props.error}</Text>
      </If>
      <If when={!props.error && props.description}>
        <Text class={`text-muted-foreground ${touch ? "text-sm" : "text-xs"}`}>{props.description}</Text>
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
  class?: string;
  testID?: string;
}

/** A choice from a list: a popup at the trigger on desktops, a sheet on phones. */
export function SelectField(props: SelectFieldProps) {
  const title = () => props.items.find((i) => i.value === props.value)?.title ?? props.placeholder ?? "";
  return (
    <Select
      value={props.value}
      items={props.items}
      onChange={(e: { value: string | null }) => e.value && props.onChange(e.value)}
      accessibilityLabel={props.label}
      testID={props.testID}
      offset={4}
      popupClass={`${popup} p-1`}
      backdropClass="bg-scrim"
      itemClass={`rounded-(--radius) ${touch ? "min-h-11 px-3 text-base" : "h-8 px-2.5 text-sm"} justify-center text-popover-foreground focus:bg-accent selected:font-medium`}
      class={props.class}
      trigger={
        <View class={`${well} ${focus} flex-row items-center justify-between gap-2 hover:border-ring/60 ${wash}`}>
          <Text class={`line-clamp-1 flex-1 ${touch ? "text-base" : "text-sm"} text-foreground`}>
            {title()}
          </Text>
          <Icon name="chevronDown" tone="muted-foreground" size="sm" />
        </View>
      }
    />
  );
}
