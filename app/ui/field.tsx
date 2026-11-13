// M3 text fields (filled, outlined, and plain for a composer's body) with the
// label inside: it rests in the field while it is empty and unfocused, and
// floats to the top, scaled to body-small, on the fast spatial curve. The
// exposed dropdown is a filled field over a menu.
import { Text, View, type InputRef, type JSX } from "nsh";
import { Input, Select } from "nsh/base";
import { Icon, type IconName } from "./icon";
import { menuItem, menuSurface } from "./overlay";

export type InputVariant = "filled" | "outlined" | "search" | "plain";

const frame: Record<InputVariant, string> = {
  // Compose FilledTextFieldTokens: surface-container-highest, extra-small top corners, a 1pt indicator that turns 2pt primary on focus.
  filled:
    "rounded-t-extra-small bg-surface-container-highest border-b border-on-surface-variant hover:border-on-surface focus-within:border-b-2 focus-within:border-primary",
  // OutlinedTextFieldTokens: a 1pt outline, 2pt primary on focus, extra-small corners.
  outlined: "rounded-extra-small border border-outline hover:border-on-surface focus-within:border-2 focus-within:border-primary",
  // M3's search bar: a full pill on surface-container-high.
  search: "rounded-full bg-surface-container-high hover:bg-surface-container-highest focus-within:bg-surface-container-highest",
  plain: "",
};
const invalid: Record<InputVariant, string> = {
  filled: "!border-error",
  outlined: "!border-error",
  search: "",
  plain: "",
};
const ease = "transition duration-350 ease-spatial-fast motion-reduce:transition-none";

export interface TextFieldProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit?: () => void;
  /** The floating label, and the accessible name. */
  label?: string;
  /** Shown in the empty field once focused. */
  placeholder?: string;
  /** Help under the field; `error` replaces it. */
  supporting?: string;
  error?: string | null;
  masked?: boolean;
  invalid?: boolean;
  disabled?: boolean;
  multiline?: boolean;
  autoFocus?: boolean;
  kind?: "text" | "email" | "url" | "number";
  returnKey?: "next" | "done" | "go" | "search" | "send";
  variant?: InputVariant;
  /** Hide the floating label (it stays the accessible name): a field in a row of the composer. */
  bare?: boolean;
  leadingIcon?: IconName;
  trailing?: JSX.Element;
  ref?: (ref: InputRef) => void;
  /** Hears a paste: images and copied files come as `files` and `uris`; text goes in as usual. */
  onPaste?: (e: { files: File[]; uris: string[] }) => void;
  /** Layout of the field from the caller: width, flex, margin. */
  class?: string;
  testID?: string;
}

export function TextField(props: TextFieldProps) {
  return field(props, !!props.multiline);
}

export function Textarea(props: Omit<TextFieldProps, "multiline">) {
  return field(props, true);
}

// One body for both, without spreading props: a spread would copy `ref`'s getter.
function field(props: TextFieldProps, multiline: boolean) {
  const keyboard = { email: "email-address", url: "url", number: "number-pad", text: "default" } as const;
  const variant = () => props.variant ?? "filled";
  const bad = () => props.invalid || !!props.error;
  const floating = () => !!props.label && !props.bare && variant() !== "plain" && variant() !== "search";
  const filled = () => props.value.length > 0;
  // The label rests centered (body-large) and floats 12pt up at 0.75 (body-small) when focused or filled.
  const label = () =>
    `absolute start-0 top-4 line-clamp-1 origin-left text-body-large ${ease} ${filled() ? "-translate-y-3 scale-75" : "group-focus-within:-translate-y-3 group-focus-within:scale-75"} ${
      bad() ? "text-error" : "text-on-surface-variant group-focus-within:text-primary"
    }`;
  const help = () => props.error || props.supporting;
  return (
    <View class={`gap-1 ${props.class ?? ""}`}>
      <View
        class={`group flex-row gap-3 ${multiline ? "flex-1 items-start" : "items-center"} ${variant() === "plain" ? "" : `${multiline ? "min-h-14 py-2" : "h-14"} px-4`} ${frame[variant()]} ${bad() ? invalid[variant()] : ""} transition-colors duration-150 ease-effects-fast ${props.disabled ? "opacity-38" : ""}`}
      >
        <If when={props.leadingIcon}>{(icon) => <Icon name={icon()} tone={bad() ? "error" : "on-surface-variant"} />}</If>
        <View class={`relative min-w-0 flex-1 self-stretch ${floating() ? "pt-6 pb-2" : multiline ? "" : "justify-center"}`}>
          <If when={floating()}>
            <Text accessibilityElementsHidden class={label()} style={{ pointerEvents: "none" }}>
              {props.label}
            </Text>
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
            class={`min-w-0 flex-1 bg-transparent text-body-large text-on-surface caret-primary selection:bg-primary/30 ${
              floating() && !filled() ? "placeholder:text-transparent group-focus-within:placeholder:text-on-surface-variant" : "placeholder:text-on-surface-variant"
            }`}
          />
        </View>
        {props.trailing}
      </View>
      <If when={help()}>
        <Text class={`px-4 text-body-small ${props.error ? "text-error" : "text-on-surface-variant"}`}>{help()}</Text>
      </If>
    </View>
  );
}

/** A label and help around a control that is no text field (a group of switches, a choice). */
export function Field(props: { label: string; description?: string; error?: string | null; children?: JSX.Element; class?: string }) {
  return (
    <View class={`gap-2 ${props.class ?? ""}`}>
      <Text class="text-title-small font-medium tracking-title-small text-on-surface">{props.label}</Text>
      {props.children}
      <If when={props.error || props.description}>
        <Text class={`text-body-small ${props.error ? "text-error" : "text-on-surface-variant"}`}>{props.error || props.description}</Text>
      </If>
    </View>
  );
}

export interface SelectFieldProps {
  value: string | null;
  items: { value: string; title: string }[];
  onChange: (value: string) => void;
  label: string;
  /** Shown floated over the value; without it the field shows the value alone. */
  floatingLabel?: boolean;
  placeholder?: string;
  class?: string;
  testID?: string;
}

/** M3's exposed dropdown menu: a filled field whose menu opens at it. */
export function SelectField(props: SelectFieldProps) {
  const title = () => props.items.find((i) => i.value === props.value)?.title;
  const floated = () => props.floatingLabel ?? true;
  return (
    <Select
      value={props.value}
      items={props.items}
      onChange={(e: { value: string | null }) => e.value && props.onChange(e.value)}
      accessibilityLabel={props.label}
      testID={props.testID}
      offset={4}
      popupClass={`${menuSurface} min-w-[200px]`}
      backdropClass="bg-scrim/32"
      itemClass={`${menuItem} selected:rounded-medium selected:bg-tertiary-container selected:text-on-tertiary-container selected:hover:bg-tertiary-container selected:focus:bg-tertiary-container`}
      class={props.class}
      trigger={
        <View
          class={`group h-14 flex-row items-center gap-3 rounded-t-extra-small border-b border-on-surface-variant bg-surface-container-highest px-4 hover:border-on-surface aria-expanded:border-b-2 aria-expanded:border-primary`}
        >
          <View class="min-w-0 flex-1">
            <If when={floated()}>
              <Text class="text-body-small text-on-surface-variant group-aria-expanded:text-primary">{props.label}</Text>
            </If>
            <Text class={`line-clamp-1 text-body-large ${title() ? "text-on-surface" : "text-on-surface-variant"}`}>{title() ?? props.placeholder ?? ""}</Text>
          </View>
          <Icon name="chevronDown" tone="on-surface-variant" class={`${ease} group-aria-expanded:rotate-180`} />
        </View>
      }
    />
  );
}
