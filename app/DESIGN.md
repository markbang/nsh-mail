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

One design language on every platform: Material 3 Expressive (M3E). The values come from Material's own sources
(androidx Compose Material3 tokens, m3.material.io, material-color-utilities); one set serves web, desktop and Android.

- Colors, `theme.json`, light and dark: every M3 color role (`primary`, `on-primary`, `primary-container`, …,
  `surface`, `surface-dim`, `surface-bright`, `surface-container-lowest` … `-highest`, `on-surface`,
  `on-surface-variant`, `outline`, `outline-variant`, `inverse-*`, `error*`, the `*-fixed` roles, `scrim`), generated
  from the seed `#2f62d0` (the icon's blue) by `scripts/m3e` (material-color-utilities 0.4.0, Fidelity scheme):
  `cd scripts/m3e && npm ci && npm run gen`. Never edit the colors by hand.
- Mail roles on that palette: `unread` = primary, `flag` = tertiary; accounts take `chart-1` … `chart-5`, M3 custom
  colors (purple, orange, green, pink, teal) harmonized to the seed. The shadcn names nsh/base draws with (`card`,
  `popover`, `muted`, `border`, `ring`, …) are aliases of M3 roles.
- `nsh.json` `tailwind`: the corner scale `rounded-extra-small` 4, `-small` 8, `-medium` 12, `-large` 16,
  `-large-increased` 20, `-extra-large` 28, `-extra-large-increased` 32, `-extra-extra-large` 48 (plus `rounded-none`,
  `rounded-full`); the type scale `text-display-large` … `text-label-small` with `tracking-<role>` (and
  `-emphasized` where it differs), weights by `font-normal|medium|bold` (`tokens.type(role, emphasized)` gives all three);
  elevation `shadow-elevation-1` … `-5`; easings `ease-spatial-fast|default|slow`, `ease-effects-fast|default|slow`
  (Material's web curves for the springs, with `duration-350|500|650` and `150|200|300`).
- Springs (expressive scheme): spatial fast 800/0.6, default 380/0.8, slow 200/0.8; effects fast 3800/1,
  default 1600/1, slow 800/1 (stiffness / damping ratio). `tokens.springTo(kind, speed)` gives NSH's
  `spring()` options. State layers: hover 8%, focus 10%, pressed 10%, dragged 16%. Disabled: container 10%,
  content 38%. Focus: a 3pt `secondary` outline 2pt off the edge (`tokens.focusRing`).
- Typed values for code: `ui/tokens.ts`. `scripts/contrast.py` checks every on-X over X pair and the inks drawn on
  surfaces in both appearances.

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
| spatial fast | spring 800 / 0.6; web curve `ease-spatial-fast` 350ms | presses, switch thumb, popups growing from their trigger |
| spatial default | spring 380 / 0.8; `ease-spatial-default` 500ms | sheets, dialogs, panes |
| spatial slow | spring 200 / 0.8; `ease-spatial-slow` 650ms | full-screen changes |
| effects fast / default / slow | critically damped 3800 / 1600 / 800; 150 / 200 / 300ms | colors and opacity |

Reduced motion: transitions off (`motion-reduce:`), motions not started (`reduceMotion()`), indicators rest.

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

The tokens are the shared M3E ones (Tokens above, `ui/tokens.ts`); Android phones add M3E's components that the
pointer-first screens do not use (`ui/m3.tsx`: short navigation bar, top app bar with search bar, FAB menu,
floating toolbar, loading indicator) and turn the shared ones Material where `material` is set (`ui/material.ts`):
round buttons that square up to `small` while pressed, filled search fields, pill drawer items, the snackbar.
NSH 0.5 gives apps no access to the system's dynamic (wallpaper) palette, so the seed stands in for it.

- Shape: message rows are `large` cards, `extra-large` while swiped; a picked row's avatar morphs from a circle to
  a `medium` square; the FAB (`large`) becomes a circle when its menu opens.
- Type: emphasized styles for the tab roots' headlines (headline medium, 500), the reader's subject (headline
  small, 500) and unread rows (bold).
- Motion: `springs` (ExpressiveMotionTokens via `tokens.springTo`) for gestures and the FAB menu; their web curves
  for class transitions.

| Screen | Change | Tool, token |
| --- | --- | --- |
| Inbox | Pull to refresh: the contained loading indicator grows and turns with the pull, morphs through the shape library while refreshing | `pullValue` → shader uniform; `nsh.time` |
| Inbox | FAB menu opens: FAB → circle, items rise one by one | radius + color transition, fast spatial; item springs, fast spatial, 40 ms stagger |
| Inbox | Row swiped: the row rounds to `extra-large`, the action's icon grows, snaps back or flies out | fast spatial transition; pan; spatial spring back |
| Inbox | Row picked: avatar circle → checked square, row fills with the secondary container | fast spatial transition |
| Nav bar | Tab chosen: the indicator grows from the icon, icon recolors | fast spatial (scale), fast effects (color) |
| Buttons | Pressed: round → `small` | fast spatial transition |
| Snackbar | Undo appears; the FAB rises over it | slide-in; spatial transition |

