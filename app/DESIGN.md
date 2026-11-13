# nsh-mail client design

Calm, dense and quiet: a mail app people keep open all day. Content (senders, subjects, the message) carries the
screen; color is spent on unread, flags, the account a message belongs to, and the one primary action.

## Targets

| Family | Layout | Sizes |
| --- | --- | --- |
| Linux, Windows, web (desktop idiom) | Three resizable panes from 840pt wide: mailboxes, list, reader. Compose, settings and adding an account open in a dialog with its own stack | Controls 28–32pt, rows about 76pt, text 12–14pt |
| Android, web on phones, narrow windows | One `NavStack`: inbox → message → compose; the mailbox list in a left `Sheet` | Controls 44pt+, body 16pt |

## Tokens

The look is HeroUI's design language (v2, `@heroui/theme` 2.4.26, MIT), ported to NSH; no HeroUI code is used.

- Colors, `theme.json`, light and dark: `background`, `foreground` (+ 50–900), `divider`, `focus`, `overlay`,
  `content1`–`content4` (+ `-foreground`), and the semantic `default`, `primary`, `secondary`, `success`, `warning`,
  `danger`, each with a 50–900 scale, a DEFAULT and a `-foreground`. Dark mode swaps the scales as HeroUI does.
- Mail roles on that palette: `unread` = primary, `flag` = warning, `selected` = primary-50 (dark primary-100),
  `hover` / `pressed` = default-200 washes, `scrim` = overlay/50. Accounts take `chart-1` … `chart-5` (secondary,
  warning, success, danger-400, default-500: never blue, so they never read as unread).
- The shadcn names (`card`, `popover`, `muted`, `muted-foreground`, `accent`, `border`, `input`, `ring`,
  `destructive`, `sidebar*`) stay as aliases of the HeroUI roles for nsh/base and older screens.
- `nsh.json` `tailwind`: radii `rounded-small` 8 / `rounded-medium` 12 / `rounded-large` 14, shadows
  `shadow-small|medium|large` (+ `-dark` layers, paired by `tokens.shadow`), type `text-tiny` 12/16, `text-small` 14/20,
  `text-medium` 16/24, `text-large` 18/28, easings `ease-css`, `ease-soft`, `ease-spring`, `ease-soft-spring`.
- Focus: a 2px `focus` outline 2px off the edge (`tokens.focusRing`). Disabled: opacity 0.5. Hover on solid
  surfaces: opacity 0.8 (0.9 dark). Typed values for code: `ui/tokens.ts`.

## The `ui` layer

`ui/` is the only place that styles `nsh/base`: `Button` (solid, bordered, light, flat, faded, shadow, ghost × the six
colors, sm/md/lg), `IconButton`, `ButtonGroup`, `TextField` / `Textarea` (flat, faded, bordered, underlined), `Field`,
`SelectField`, `Card`, `Listbox` / `NavItem`, `Row`, `Dialog` (Modal, `DialogBody`, `DialogFooter`), `Menu` (Dropdown),
`Popover`, `Tooltip`, `Sheet`, `Tabs`, `Switch`, `Checkbox`, `Chip`, `Avatar`, `Badge`, `Divider`, `Spinner`, `Kbd`,
`Toast`, `Empty`, `AccountMark`, `Icon` (Lucide geometry, tinted with `tint-*`). Features pass variants, colors,
sizes and states, never raw colors or radii. `ui/Gallery.tsx` shows every one; `ui/gallery.test.tsx` drives it.

## Motion tokens

| Token | Value | For |
| --- | --- | --- |
| state | 250ms CSS `ease` | colors, opacity, the press (scale 0.97), switch thumb, tabs cursor |
| popover | in 300ms from scale 0.8 + fade; out 150ms to 0.96 | menus, popovers |
| tooltip | in 200ms from 0.85; out 150ms | tooltips |
| modal | fade, 400ms in / 300ms out (platform `fade`) | dialogs |
| spinner | 0.8s turns, one eased and one linear | Spinner |

Reduced motion: transitions off (`motion-reduce:`), motions not started (`reduceMotion()`), the spinner rests.

## Motion inventory

| Screen | Change | Motion |
| --- | --- | --- |
| All | Button / icon button press | Scale 0.97, 250ms `ease`; solid surfaces dim to 0.8 on hover |
| All | Hover on buttons, rows, nav items | Wash in, 250ms `ease` |
| Mailboxes | Current mailbox changes | Selected fill (default-100) on the new item, 250ms color transition |
| List | Rows arrive (mailbox opened, page loaded) | No motion: rows are recycled data; pull to refresh uses the platform's indicator |
| List | Load more | Spinner in the footer while the next page loads |
| List | Unread → read, flag toggled | Weight and icon change in place, no motion (data ticks) |
| Reader | Another message opens | The reader fades in |
| Reader | Remote images loaded | Notice leaves, images replace their placeholders in place |
| Reader | Attachment pressed | Tile press scale; a spinner replaces its icon until the file opens or downloads |
| Reader | Image thumbnail arrives | The tile grows its preview in place, no motion |
| Compose / settings (wide) | Dialog opens / closes | Scrim and card fade (`Dialog` `fade`) |
| Phones | Pages push / pop | The platform's stack transition |
| Phones | Mailbox sheet | Slides from the start edge over the scrim |
| Menus | Account menu opens | Grows from 0.8 and fades in at the trigger (300ms); leaves at 0.96 (150ms) |
| Onboarding | Sign in ↔ create account | Tabs cursor slides, 250ms ease-out |
| Settings | Notifications switch | Track fills, thumb slides, 250ms |
| All | Toast | Rises from the bottom edge (`slideInUp`), leaves after 3.2s |
| All | Sync running | Refresh glyph turns until the sync ends; rests with reduced motion |

Checked on still screenshots of the web build (`nsh test --target web`) in light and dark, English and Chinese:
layout, colors and states at rest. The motions themselves have not been watched frame by frame, and nothing has
been seen on Android or the Linux and Windows desktops yet (no device or GTK in the sandbox).
