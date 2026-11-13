# nsh-mail client design

Calm, dense and quiet: a mail app people keep open all day. Content (senders, subjects, the message) carries the
screen; color is spent on unread, flags, the account a message belongs to, and the one primary action.

## Targets

| Family | Layout | Sizes |
| --- | --- | --- |
| Linux, Windows, web (desktop idiom) | Three resizable panes from 840pt wide: mailboxes, list, reader. Compose, settings and adding an account open in a dialog with its own stack | Controls 28–32pt, rows about 76pt, text 12–14pt |
| Android phones | Material 3 Expressive (below): a `NavStack` per tab (Mail with the unread count, Search, Mailboxes, Settings) over M3E's short navigation bar; tab roots draw their own top app bar (search bar + emphasized headline), pushed pages the system toolbar and cover the bar; the FAB menu (New message, Add account) over Mail; the reader's floating toolbar | Controls 40dp with 48dp targets, body 16sp |
| iOS phones | The same tabs on the system tab bar (`nsh/mobile` `TabBar`), a reply bar under the message | Controls 44pt+, body 16pt |
| Web on phones, narrow windows | One `NavStack`: inbox → message → compose; the mailbox list in a left `Sheet` | Controls 44pt+, body 16pt |

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
colors, sm/md/lg), `IconButton`, `ButtonGroup`, `TextField` / `Textarea` (flat, faded, bordered, underlined; `plain` for the composer body), `Field`,
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

## Android: Material 3 Expressive

Sources: m3.material.io ("Start building with M3 Expressive", motion physics "How it works" and "Specs", shape
"Corner radius scale", type scale tokens, component specs: FAB menu, toolbars, loading indicator, navigation bar) and
Compose Material3's tokens (`ExpressiveMotionTokens`, `TypeScaleTokens`, `FabMenuBaselineTokens`,
`FloatingToolbarTokens`, `LoadingIndicatorTokens`, `NavigationBarTokens`, `ButtonSmallTokens`).

- Color: the 2025 color spec's content scheme from seed #0062d6 (material-color-utilities 0.4), `md-*` in
  theme.json. On Android `ui/material.ts` maps the HeroUI roles the screens name onto them (`material.json`), so every
  screen takes Material's surfaces, containers and on-colors; components new to Android name `md-*` directly. NSH 0.5
  gives apps no access to the system's dynamic (wallpaper) palette, so the seed stands in for it.
  `scripts/contrast.py` checks both themes.
- Shape: the M3 corner scale as `rounded-shape-xs` … `rounded-shape-2xl`. Buttons and icon buttons are round and
  square up to `shape-sm` while pressed; the FAB (`shape-lg`) becomes a circle when its menu opens; message rows are
  `shape-lg` cards, `shape-xl` while swiped; a picked row's avatar morphs from a circle to a `shape-md` square.
- Type: the M3 scale (`text-headline-m`, `text-title-l`, `text-label-m` …); emphasized styles for the tab roots'
  headlines (headline medium, 500), the reader's subject (headline small, 500) and unread rows (bold).
- Motion: Material's springs (`springs` in `ui/material.ts`: stiffness and damping ratio from
  ExpressiveMotionTokens, as NSH spring coefficients) for gestures and the FAB menu; their published cubic
  equivalents (`ease-spatial-fast` …, 350/500 ms spatial, 150/200 ms effects) for class transitions.

| Screen | Change | Tool, token |
| --- | --- | --- |
| Inbox | Pull to refresh: the contained loading indicator grows and turns with the pull, morphs through the shape library while refreshing | `pullValue` → shader uniform; `nsh.time` |
| Inbox | FAB menu opens: FAB → circle, items rise one by one | radius + color transition, fast spatial; item springs, fast spatial, 40 ms stagger |
| Inbox | Row swiped: the row rounds to `shape-xl`, the action's icon grows, snaps back or flies out | fast spatial transition; pan; spatial spring back |
| Inbox | Row picked: avatar circle → checked square, row fills with the secondary container | fast spatial transition |
| Nav bar | Tab chosen: the indicator grows from the icon, icon recolors | fast spatial (scale), fast effects (color) |
| Buttons | Pressed: round → `shape-sm` | fast spatial transition |
| Snackbar | Undo appears; the FAB rises over it | slide-in; spatial transition |

