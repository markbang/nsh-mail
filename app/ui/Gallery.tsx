// Every ui component in its variants, sizes and states: the entry the gallery
// test opens to check them and, on a real target, to picture them.
import { mount, Scroll, signal, Text, View, type JSX } from "nsh";
import {
  AccountMark,
  Avatar,
  Badge,
  Button,
  ButtonGroup,
  Card,
  Checkbox,
  Chip,
  Choice,
  CircularProgress,
  Dialog,
  DialogBody,
  DialogFooter,
  Divider,
  Fab,
  FabMenu,
  FloatingToolbar,
  IconButton,
  LinearProgress,
  ListItem,
  LoadingIndicator,
  Menu,
  NavBar,
  NavigationRail,
  SelectField,
  Snackbar,
  SplitButton,
  Switch,
  TextField,
  ToolbarButton,
  type ButtonVariant,
  type IconButtonVariant,
} from ".";

const buttonVariants: ButtonVariant[] = ["filled", "tonal", "outlined", "elevated", "text"];
const iconVariants: IconButtonVariant[] = ["standard", "filled", "tonal", "outlined"];

function Section(props: { title: string; children?: JSX.Element }) {
  return (
    <View class="gap-4">
      <Text role="header" class="text-title-medium font-medium tracking-title-medium text-on-surface">
        {props.title}
      </Text>
      {props.children}
    </View>
  );
}

const Wrap = (props: { children?: JSX.Element }) => <View class="flex-row flex-wrap items-center gap-3">{props.children}</View>;

function Gallery() {
  const on = signal(true);
  const checked = signal(true);
  const choice = signal<"inbox" | "sent" | "drafts">("inbox");
  const nav = signal("inbox");
  const text = signal("mark@bangwu.top");
  const empty = signal("");
  const modal = signal(false);
  const fab = signal(false);
  const flagged = signal(false);
  const security = signal("tls");
  const bar = signal(0);
  return (
    <View class="flex-1 flex-row bg-surface">
      <View class="w-64 border-e border-outline-variant">
        <NavigationRail
          label="Mailboxes"
          expanded
          selected={nav()}
          onSelect={(k) => nav.set(k)}
          header={<Button icon="compose" label="Compose" variant="tonal" size="md" shape="square" />}
          items={[
            { key: "inbox", label: "All inboxes", icon: "inboxes", badge: 13, testID: "nav-inbox" },
            { key: "work", label: "Work", lead: <AccountMark index={0} size="md" />, badge: 4 },
            { key: "sent", label: "Sent", icon: "send", section: "Folders", testID: "nav-sent" },
            { key: "drafts", label: "Drafts", icon: "drafts" },
          ]}
        />
      </View>
      <View class="w-24 border-e border-outline-variant">
        <NavigationRail
          label="Collapsed"
          selected={nav()}
          onSelect={(k) => nav.set(k)}
          header={<Fab icon="compose" label="Compose" onPress={() => {}} />}
          items={[
            { key: "inbox", label: "Inbox", icon: "inboxes", badge: 13 },
            { key: "sent", label: "Sent", icon: "send" },
            { key: "drafts", label: "Drafts", icon: "drafts" },
          ]}
        />
      </View>
      <Scroll class="flex-1" contentClass="gap-10 p-8">
        <Text testID="state" class="text-body-small text-on-surface-variant">{`${on() ? "on" : "off"} ${checked() ? "checked" : "unchecked"} ${nav()} ${choice()}`}</Text>

        <Section title="Buttons">
          <Wrap>
            <Each of={buttonVariants}>{(v) => <Button variant={v()} label={v()} icon="send" testID={`btn-${v()}`} />}</Each>
            <Button label="Delete" tone="error" icon="trash" />
            <Button label="Disabled" disabled />
            <Button label="Sending" loading />
          </Wrap>
          <Wrap>
            <Button size="xs" label="Extra small" />
            <Button size="sm" label="Small" />
            <Button size="md" label="Medium" />
            <Button size="sm" shape="square" label="Square" variant="tonal" />
            <Button label="Toggle" variant="tonal" selected={on()} onPress={() => on.update((x) => !x)} />
          </Wrap>
          <Wrap>
            <Each of={iconVariants}>{(v) => <IconButton variant={v()} icon="archive" label={`${v()} icon button`} />}</Each>
            <IconButton icon={flagged() ? "flagFilled" : "flag"} label="Flag" variant="tonal" selected={flagged()} onPress={() => flagged.update((x) => !x)} />
            <IconButton icon="trash" label="Wide" width="wide" variant="filled" />
            <IconButton icon="trash" label="Medium" size="md" variant="tonal" shape="square" />
          </Wrap>
          <Wrap>
            <ButtonGroup label="Reply">
              <Button label="Reply" icon="reply" variant="tonal" />
              <Button label="Forward" icon="forward" variant="outlined" />
            </ButtonGroup>
            <Choice
              label="Folder"
              value={choice()}
              onChange={(v) => choice.set(v)}
              options={[
                { value: "inbox", label: "Inbox" },
                { value: "sent", label: "Sent" },
                { value: "drafts", label: "Drafts", testID: "choice-drafts" },
              ]}
            />
            <SplitButton
              label="Send"
              icon="send"
              onPress={() => {}}
              menuLabel="More send options"
              actions={[{ id: "later", label: "Send later", icon: "drafts", onSelect: () => {} }]}
            />
          </Wrap>
        </Section>

        <Section title="FAB, FAB menu, toolbar">
          <Wrap>
            <Fab icon="compose" label="Compose" size="sm" onPress={() => {}} />
            <Fab icon="compose" label="Compose" onPress={() => {}} />
            <Fab icon="compose" label="Compose" extended onPress={() => {}} />
            <Fab icon="compose" label="Compose" size="lg" onPress={() => {}} />
            <View class="h-64 justify-end">
              <FabMenu
                icon="plus"
                label="New"
                closeLabel="Close"
                open={fab()}
                onOpenChange={(o) => fab.set(o)}
                items={[
                  { icon: "compose", label: "Compose", onPress: () => {} },
                  { icon: "plus", label: "Add account", onPress: () => {} },
                ]}
                testID="fab-menu"
              />
            </View>
          </Wrap>
          <Wrap>
            <FloatingToolbar label="Message">
              <ToolbarButton icon="reply" label="Reply" onPress={() => {}} />
              <ToolbarButton icon="replyAll" label="Reply all" onPress={() => {}} />
              <ToolbarButton icon="forward" label="Forward" onPress={() => {}} />
              <ToolbarButton icon="archive" label="Archive" onPress={() => {}} emphasized />
              <ToolbarButton icon="trash" label="Delete" onPress={() => {}} />
            </FloatingToolbar>
            <FloatingToolbar label="Vibrant" vibrant>
              <ToolbarButton icon="reply" label="Reply" onPress={() => {}} vibrant />
              <ToolbarButton icon="archive" label="Archive" onPress={() => {}} vibrant emphasized />
            </FloatingToolbar>
          </Wrap>
        </Section>

        <Section title="Text fields">
          <View class="max-w-xl flex-row gap-4">
            <TextField label="Email" value={text()} onChange={(v) => text.set(v)} kind="email" class="flex-1" testID="field" />
            <TextField label="Subject" value={empty()} onChange={(v) => empty.set(v)} variant="outlined" class="flex-1" supporting="Optional" />
          </View>
          <View class="max-w-xl flex-row gap-4">
            <TextField label="Password" value="" onChange={() => {}} masked error="Wrong password" class="flex-1" />
            <SelectField
              label="Security"
              value={security()}
              onChange={(v) => security.set(v)}
              items={[
                { value: "tls", title: "TLS" },
                { value: "starttls", title: "STARTTLS" },
              ]}
              class="flex-1"
            />
          </View>
        </Section>

        <Section title="Selection">
          <View class="max-w-sm gap-3">
            <Switch label="Notifications" description="New mail on this device" checked={on()} onChange={(v) => on.set(v)} testID="switch" />
            <Checkbox label="Remember me" checked={checked()} onChange={(v) => checked.set(v)} testID="checkbox" />
            <Checkbox label="Some" checked={false} indeterminate onChange={() => {}} />
          </View>
          <Wrap>
            <Chip label="Unread" kind="filter" selected={checked()} onPress={() => checked.update((x) => !x)} />
            <Chip label="Attachments" kind="filter" onPress={() => {}} />
            <Chip label="report.pdf" icon="paperclip" onClose={() => {}} closeLabel="Remove" />
            <Chip label="Suggestion" kind="suggestion" onPress={() => {}} />
          </Wrap>
        </Section>

        <Section title="Lists, cards, menus, dialogs">
          <Card variant="outlined" class="max-w-xl p-1">
            <ListItem headline="Sarah Chen" supporting="Lunch tomorrow?" leading={<Avatar name="Sarah Chen" />} trailing={<Text class="text-label-small text-on-surface-variant">8:29 AM</Text>} onPress={() => {}} />
            <ListItem headline="Airbnb" supporting="Your trip to Kyoto is coming up" leading={<Avatar name="Airbnb" />} selected onPress={() => {}} />
            <Divider inset />
            <ListItem headline="Static item" supporting="No press" icon="mail" />
          </Card>
          <Wrap>
            <Card variant="filled" class="w-48 p-4"><Text class="text-body-medium text-on-surface">Filled card</Text></Card>
            <Card variant="elevated" class="w-48 p-4"><Text class="text-body-medium text-on-surface">Elevated card</Text></Card>
            <Menu
              label="Message actions"
              trigger={<IconButton icon="more" label="More" testID="menu-trigger" />}
              actions={[
                { id: "read", label: "Mark as read", icon: "mailOpen", onSelect: () => {} },
                { id: "archive", label: "Archive", icon: "archive", selected: true, onSelect: () => {} },
                { id: "delete", label: "Delete", icon: "trash", destructive: true, separatorBefore: true, onSelect: () => {} },
              ]}
            />
            <Button label="Open dialog" variant="outlined" onPress={() => modal.set(true)} testID="open-modal" />
          </Wrap>
        </Section>

        <Section title="Feedback">
          <Wrap>
            <LoadingIndicator label="Loading" />
            <LoadingIndicator label="Loading" contained />
            <CircularProgress label="Loading" />
            <CircularProgress label="Half" value={0.5} />
            <Badge label="4"><IconButton icon="inbox" label="Inbox" /></Badge>
            <Badge dot><IconButton icon="mail" label="Mail" /></Badge>
            <Avatar name="Mark Wu" />
            <Avatar name="吴马克" size="lg" />
          </Wrap>
          <View class="max-w-xl gap-4">
            <LinearProgress label="Syncing" />
            <LinearProgress label="Uploading" value={0.6} />
            <Snackbar text="Message archived" action={{ label: "Undo", run: () => {} }} />
          </View>
          <View class="max-w-md">
            <NavBar
              label="Sections"
              selected={bar()}
              onChange={(i) => bar.set(i)}
              items={[
                { title: "Mail", icon: "inbox", badge: "4" },
                { title: "Search", icon: "search" },
                { title: "Settings", icon: "settings" },
              ]}
            />
          </View>
        </Section>
      </Scroll>
      <Dialog open={modal()} onClose={() => modal.set(false)} label="Discard" title="Discard draft?" testID="modal">
        <DialogBody>
          <Text class="text-body-medium tracking-body-medium text-on-surface-variant">The draft and its attachments will be deleted.</Text>
        </DialogBody>
        <DialogFooter>
          <Button label="Keep" variant="text" onPress={() => modal.set(false)} testID="keep" />
          <Button label="Discard" variant="text" onPress={() => modal.set(false)} />
        </DialogFooter>
      </Dialog>
    </View>
  );
}

mount(Gallery);
