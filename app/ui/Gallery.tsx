// Every ui component in its variants, sizes and states: the entry the gallery
// test opens to check them and, on a real target, to picture them.
import { mount, Scroll, signal, Text, View, type JSX } from "nsh";
import {
  Avatar,
  Badge,
  Button,
  Card,
  CardBody,
  CardFooter,
  CardHeader,
  Checkbox,
  Chip,
  Dialog,
  DialogBody,
  DialogFooter,
  Divider,
  Field,
  IconButton,
  Kbd,
  Listbox,
  Menu,
  NavItem,
  SelectField,
  Spinner,
  Switch,
  Tabs,
  TextField,
  Textarea,
  Toast,
  type Color,
  type Variant,
} from ".";

const colors: Color[] = ["default", "primary", "secondary", "success", "warning", "danger"];
const variants: Variant[] = ["solid", "bordered", "light", "flat", "faded", "shadow", "ghost"];

function Section(props: { title: string; children?: JSX.Element }) {
  return (
    <View class="gap-3">
      <Text role="header" class="text-small font-semibold text-foreground-500">
        {props.title}
      </Text>
      {props.children}
    </View>
  );
}

function Gallery() {
  const on = signal(true);
  const checked = signal(true);
  const tab = signal<"inbox" | "sent" | "drafts">("inbox");
  const nav = signal("inbox");
  const text = signal("mark@bangwu.top");
  const modal = signal(false);
  const security = signal("tls");
  return (
    <View class="flex-1 bg-background">
      <Scroll class="flex-1" contentClass="gap-8 p-8">
        <Text testID="state" class="text-tiny text-foreground-400">{`${on() ? "on" : "off"} ${checked() ? "checked" : "unchecked"} ${nav()} ${tab()}`}</Text>
        <Section title="Button">
          <View class="gap-2">
            <Each of={variants}>
              {(v) => (
                <View class="flex-row flex-wrap items-center gap-2">
                  <Text class="w-16 text-tiny text-foreground-500">{v()}</Text>
                  <Each of={colors}>{(c) => <Button variant={v()} color={c()} size="sm" label={c()} testID={`btn-${v()}-${c()}`} />}</Each>
                </View>
              )}
            </Each>
            <View class="flex-row flex-wrap items-center gap-2">
              <Button color="primary" size="sm" icon="send" label="Small" />
              <Button color="primary" icon="send" label="Medium" />
              <Button color="primary" size="lg" icon="send" label="Large" />
              <Button color="primary" loading label="Loading" />
              <Button disabled label="Disabled" />
              <IconButton icon="reply" label="Reply" />
              <IconButton icon="archive" label="Archive" variant="flat" />
              <IconButton icon="trash" label="Delete" variant="flat" color="danger" />
              <IconButton icon="flagFilled" label="Flag" variant="bordered" color="warning" />
            </View>
          </View>
        </Section>

        <View class="flex-row gap-8">
          <View class="flex-1 gap-8">
            <Section title="Input">
              <TextField value={text()} onChange={text.set} label="Email" startIcon="mail" testID="input-flat" />
              <TextField value="" onChange={() => {}} placeholder="Faded" label="Faded" variant="faded" />
              <TextField value="" onChange={() => {}} placeholder="Bordered" label="Bordered" variant="bordered" />
              <TextField value="" onChange={() => {}} placeholder="Underlined" label="Underlined" variant="underlined" />
              <Field label="Password" error="Wrong password">
                <TextField value="hunter2" onChange={() => {}} masked invalid label="Password" />
              </Field>
              <Field label="Security" description="TLS on connect">
                <SelectField
                  value={security()}
                  items={[
                    { value: "tls", title: "TLS" },
                    { value: "starttls", title: "STARTTLS" },
                  ]}
                  onChange={security.set}
                  label="Security"
                />
              </Field>
              <Textarea value={"Sounds good, see you then."} onChange={() => {}} label="Body" class="h-24" />
            </Section>
          </View>

          <View class="flex-1 gap-8">
            <Section title="Card · Listbox">
              <Card>
                <CardHeader>
                  <Avatar name="Jonas Weber" color="primary" />
                  <View class="flex-1">
                    <Text class="text-small font-semibold text-foreground">Jonas Weber</Text>
                    <Text class="text-tiny text-foreground-500">jonas@northwind.dev</Text>
                  </View>
                  <Chip label="Work" size="sm" variant="flat" color="secondary" />
                </CardHeader>
                <Divider />
                <CardBody>
                  <Listbox label="Mailboxes">
                    <NavItem label="Inbox" icon="inbox" end={<Badge label="12" color="primary" variant="solid" />} selected={nav() === "inbox"} onPress={() => nav.set("inbox")} testID="nav-inbox" />
                    <NavItem label="Sent" icon="send" selected={nav() === "sent"} onPress={() => nav.set("sent")} testID="nav-sent" />
                    <NavItem label="Drafts" icon="drafts" detail="3" selected={nav() === "drafts"} onPress={() => nav.set("drafts")} />
                  </Listbox>
                </CardBody>
                <Divider />
                <CardFooter class="justify-end">
                  <Menu
                    label="More"
                    trigger={<Button variant="bordered" size="sm" endIcon="chevronDown" label="Actions" testID="menu-trigger" />}
                    actions={[
                      { id: "read", label: "Mark as read", icon: "mailOpen", shortcut: "⇧U", onSelect: () => {} },
                      { id: "archive", label: "Archive", icon: "archive", onSelect: () => {} },
                      { id: "delete", label: "Delete", icon: "trash", destructive: true, separatorBefore: true, onSelect: () => {} },
                    ]}
                  />
                  <Button color="primary" size="sm" label="Open modal" onPress={() => modal.set(true)} testID="open-modal" />
                </CardFooter>
              </Card>
            </Section>

            <Section title="Tabs">
              <Tabs value={tab()} onChange={tab.set} options={[{ value: "inbox", label: "Inbox" }, { value: "sent", label: "Sent" }, { value: "drafts", label: "Drafts" }]} />
              <Tabs value={tab()} onChange={tab.set} variant="underlined" options={[{ value: "inbox", label: "Inbox" }, { value: "sent", label: "Sent" }, { value: "drafts", label: "Drafts" }]} />
              <Tabs value={tab()} onChange={tab.set} variant="bordered" size="sm" options={[{ value: "inbox", label: "Inbox" }, { value: "sent", label: "Sent" }, { value: "drafts", label: "Drafts" }]} />
            </Section>
          </View>

          <View class="flex-1 gap-8">
            <Section title="Chip · Avatar · Badge">
              <View class="flex-row flex-wrap gap-2">
                <Each of={colors}>{(c) => <Chip label={c()} color={c()} variant="flat" />}</Each>
              </View>
              <View class="flex-row flex-wrap gap-2">
                <Chip label="Dot" variant="dot" color="success" />
                <Chip label="Bordered" variant="bordered" color="primary" />
                <Chip label="Q4-roadmap.pdf" icon="paperclip" variant="flat" onPress={() => {}} onClose={() => {}} closeLabel="Remove" />
              </View>
              <View class="flex-row items-center gap-3">
                <Avatar name="Mark Wu" size="sm" />
                <Avatar name="吴马克" color="secondary" />
                <Avatar name="Lumen Labs" size="lg" color="success" bordered />
                <Badge label="5" color="danger" variant="solid">
                  <Avatar name="Jonas Weber" color="primary" />
                </Badge>
                <Badge dot color="success" variant="solid">
                  <Avatar name="Ann Lee" color="warning" />
                </Badge>
              </View>
            </Section>

            <Section title="Switch · Checkbox · Spinner · Kbd">
              <Switch checked={on()} onChange={on.set} label="Notifications" description="New mail on this device" testID="switch" />
              <Switch checked={!on()} onChange={(v) => on.set(!v)} label="Small" size="sm" color="success" />
              <Checkbox checked={checked()} onChange={checked.set} label="Remember me" testID="checkbox" />
              <Checkbox checked={false} indeterminate onChange={() => {}} label="Some selected" color="secondary" />
              <View class="flex-row items-center gap-4">
                <Spinner label="Loading" size="sm" />
                <Spinner label="Loading" />
                <Spinner label="Loading" size="lg" color="secondary" />
                <Kbd keys="⌘ K" label="Command K" />
              </View>
              <Toast text="Message sent" />
            </Section>
          </View>
        </View>
      </Scroll>

      <Dialog open={modal()} onClose={() => modal.set(false)} label="Discard draft" title="Discard draft?" size="sm" testID="modal">
        <DialogBody>
          <Text class="text-small text-foreground-500">The message to Jonas Weber hasn't been sent.</Text>
        </DialogBody>
        <DialogFooter>
          <Button variant="light" color="danger" label="Discard" onPress={() => modal.set(false)} />
          <Button color="primary" label="Keep editing" onPress={() => modal.set(false)} testID="keep" />
        </DialogFooter>
      </Dialog>
    </View>
  );
}

mount(Gallery);
