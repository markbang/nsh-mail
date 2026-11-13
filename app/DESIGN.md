# nsh-mail client design

Calm, dense and quiet: a mail app people keep open all day. Content (senders, subjects, the message) carries the
screen; color is spent on unread, flags, the account a message belongs to, and the one primary action.

## Targets

One design language everywhere: Material 3 Expressive. Layouts follow M3's window classes.

| Family | Layout | Sizes |
| --- | --- | --- |
| Linux, Windows, web from 840pt (expanded, large) | Navigation rail (collapsed 96pt; expanded 256pt from 1200pt, with sections and counts) and two panes on surface-container, 12pt apart with M3's drag handle: the list (fixed, 320–560pt) and the reader (flexible). The reader's actions float in a toolbar at its bottom. Compose, settings and adding an account open in a dialog with its own stack | M3 sizes: 40pt buttons and icon buttons, 56pt fields and search bar, list rows about 80pt |
| Android phones | Material 3 Expressive (below): a `NavStack` per tab (Mail with the unread count, Search, Mailboxes, Settings) over M3E's short navigation bar; tab roots draw their own top app bar (search bar + emphasized headline), pushed pages the system toolbar and cover the bar; the FAB menu (New message, Add account) over Mail; the reader's floating toolbar | Controls 40dp with 48dp targets, body 16sp |
| iOS phones | The same tabs on the system tab bar (`nsh/mobile` `TabBar`), a reply bar under the message | Controls 44pt+, body 16pt |
| Web below 840pt (compact, medium) | M3's navigation bar (Mail with the unread count, Search, Mailboxes, Settings) over a `NavStack` per section; the FAB menu (compose, add an account) over Mail; pushed pages cover the bar, the reader keeps its floating toolbar | 48pt targets |

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

`ui/` is the only place that styles `nsh/base`. Sizes and colors come from Compose Material3's component tokens.

- Actions: `Button` (filled, tonal, outlined, elevated, text; xs/sm/md; round or square; toggle via `selected`; `tone="error"`), `IconButton` (standard, filled, tonal, outlined; xs/sm/md; narrow/default/wide), `ButtonGroup` (standard or connected), `Choice` (a connected group choosing one option), `SplitButton`, `Fab` (sm/md/lg, extended), `FabMenu`.
- Containers and navigation: `NavigationRail` (collapsed or expanded), `NavBar`, `FloatingToolbar` with `ToolbarButton`, `AppBar`, `Row` and `ListItem` (expressive shapes), `SectionLabel`, `Card` (filled, outlined, elevated), `Dialog` / `DialogBody` / `DialogFooter`, `Menu`, `Popover`, `Sheet`, `Panes` / `Pane`.
- Inputs: `TextField` / `Textarea` (filled, outlined, search, plain; floating label, supporting text, error), `SelectField` (exposed dropdown), `Field`, `Chip` (assist, filter, input, suggestion), `Switch`, `Checkbox`.
- Feedback: `LoadingIndicator` (M3E's morphing shape, a shader, or a turning cookie where there are no shaders), `LinearProgress`, `CircularProgress`, `Snackbar`, `Tooltip` (plain), `Badge`, `Avatar`, `AccountMark`, `Divider`, `Empty`, `FileTile`, `Icon` (Lucide geometry, tinted with `tint-*`).

Every pressable draws M3's state layer (the content color at 8% hovered, 10% pressed or focused) and morphs its corners while pressed. Features pass variants, sizes and states, never raw colors or radii. `ui/Gallery.tsx` shows every component; `ui/gallery.test.tsx` drives it.

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
| All | Button / icon button / FAB press | Corners morph to the pressed shape (small for 32–40pt, medium for 56pt): spatial fast (350ms `ease-spatial-fast`). The state layer fades in: effects fast (150ms) |
| All | Toggle button selected | Round → square (square → round), with the selected colors: spatial fast |
| All | List row hover / press / select | Extra-small → medium (hover) → large (pressed, selected), fill secondary-container: spatial fast |
| Rail, nav bar | Destination changes | The active indicator pill grows from the icon (scale-x 0.5 → 1) and fills: spatial fast |
| List | Rows arrive, unread → read | No motion: rows are recycled data |
| List | Refresh, load more | M3E loading indicator in the list header / footer, morphing through soft burst → cookie 9 → pentagon → pill → sunny → cookie 4 → oval while it turns |
| Reader | Another message opens | The reader fades in (150ms) |
| Reader | Floating toolbar | Stays at the bottom center; its buttons square up while pressed |
| Inbox (narrow) | FAB menu opens | The FAB morphs to a round primary close button; the items rise one after another on the fast spatial spring (40ms apart) |
| Fields | Focus, typing | The label floats up and shrinks to body-small, the indicator thickens to 2pt primary: spatial fast / effects fast |
| Menus, popovers | Open / close | Grow from 0.8 at the trigger (spatial fast); leave at 0.96 (effects fast) |
| Tooltips | Show / hide | Fade, effects fast |
| Dialogs | Open / close | Scrim and container fade (platform `fade`) |
| Settings | Switch | Handle grows 16 → 24pt and slides; 28pt while pressed: spatial fast. Track recolors: effects fast |
| Panes | Drag handle | Appears under the pointer, grows while dragged: spatial fast |
| All | Snackbar | Rises from the bottom edge (`slideInUp`, 300ms), leaves after 3.2s |

Reduced motion: transitions off (`motion-reduce:`), motions and springs not started (`reduceMotion()`), indicators rest.

Checked on still screenshots of the web build in light and dark (`nsh test --target web` on the demo backend, and the
PR screenshot workflow on a real server). The motions have not been watched frame by frame, and the Linux and Windows
desktops have not been seen (no GTK in the sandbox).

## Android

Sources: m3.material.io ("Start building with M3 Expressive", motion physics "How it works" and "Specs", shape
"Corner radius scale", type scale tokens, component specs: FAB menu, toolbars, loading indicator, navigation bar) and
Compose Material3's tokens (`ExpressiveMotionTokens`, `TypeScaleTokens`, `FabMenuBaselineTokens`,
`FloatingToolbarTokens`, `LoadingIndicatorTokens`, `NavigationBarTokens`, `ButtonSmallTokens`).

The tokens and components are the shared M3E ones (Tokens and the `ui` layer above). Where `material` is set
(`ui/material.ts`), Android phones lay a few screens out their own way: tab roots draw M3E's top app bar with the
search bar, the message list swipes and picks rows.
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

