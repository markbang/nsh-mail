# nsh-mail client design

Calm, dense and quiet: a mail app people keep open all day. Content (senders, subjects, the message) carries the
screen; color is spent on unread, flags, the account a message belongs to, and the one primary action.

## Targets

| Family | Layout | Sizes |
| --- | --- | --- |
| Linux, Windows, web (desktop idiom) | Three resizable panes from 840pt wide: mailboxes, list, reader. Compose, settings and adding an account open in a dialog with its own stack | Controls 28–32pt, rows about 76pt, text 12–14pt |
| Android, web on phones, narrow windows | One `NavStack`: inbox → message → compose; the mailbox list in a left `Sheet` | Controls 44pt+, body 16pt |

## Tokens

`theme.json` holds every color, light and dark. Beyond shadcn's roles: `hover`, `pressed` and `selected` (washes over
any surface), `scrim`, `unread`, `flag`, `success`, `primary-hover`. Accounts take `chart-1` … `chart-5` in the order
they were added (none of them blue, so they never read as the unread dot). Radii: `radius` 6pt for controls and
popups, `radius-lg` 10pt for containers, `radius-inset` 4pt for a segment inside a 2pt track.

Type steps: `text-xs` 12, `text-sm` 14, `text-base` 16, `text-lg` 18, `text-xl` 20. Two weights per screen: regular and
medium/semibold (unread rows).

## The `ui` layer

`ui/` is the only place that styles `nsh/base`: `Button`, `IconButton`, `TextField`, `Field`, `SelectField`, `Switch`,
`Segmented`, `Dialog`, `Sheet`, `Menu`, `Row`, `NavItem`, `SectionLabel`, `Panes`, `AccountMark`, `Avatar`, `Badge`,
`Separator`, `Empty`, `Spinner`, `Toast`, `Icon` (Lucide geometry as tinted SVG). Features pass variants, sizes and
states, never colors or radii.

## Motion tokens

| Token | Value | For |
| --- | --- | --- |
| instant | 100ms ease-out | hover wash, row wash |
| fast | 150ms | press (scale 0.97), switch thumb, popup leaving |
| base | 200ms ease-in-out | segmented indicator sliding, toast rising |
| slow | 300ms | reader and empty states fading in |

Reduced motion: transitions off (`motion-reduce:`), motions not started (`reduceMotion()`), the spinner rests.

## Motion inventory

| Screen | Change | Motion |
| --- | --- | --- |
| All | Button / icon button press | Scale 0.97 + deeper wash, `fast`, soft settle |
| All | Hover on buttons, rows, nav items | Wash in, `instant` |
| Mailboxes | Current mailbox changes | Selected fill on the new item, `instant` color transition (no sliding bar yet) |
| List | Rows arrive (mailbox opened, page loaded) | No motion: rows are recycled data; pull to refresh uses the platform's indicator |
| List | Load more | Spinner in the footer while the next page loads |
| List | Unread → read, flag toggled | Weight and icon change in place, no motion (data ticks) |
| Reader | Another message opens | The reader fades in, `fast` |
| Reader | Remote images loaded | Notice leaves, images replace their placeholders in place |
| Compose / settings (wide) | Dialog opens / closes | Scrim and card fade (`Dialog` `fade`) |
| Phones | Pages push / pop | The platform's stack transition |
| Phones | Mailbox sheet | Slides from the start edge over the scrim |
| Menus | Account menu opens | Grows from 0.97 and fades in at the trigger; leaves faster |
| Onboarding | Sign in ↔ create account | Segmented indicator slides, `base` |
| Settings | Notifications switch | Track color and thumb slide, `fast` |
| All | Toast | Rises from the bottom edge (`slideInUp`), leaves after 3.2s |
| All | Sync running | Refresh glyph turns until the sync ends; rests with reduced motion |

Checked on still screenshots of the web build (`nsh test --target web`) in light and dark, English and Chinese:
layout, colors and states at rest. The motions themselves have not been watched frame by frame, and nothing has
been seen on Android or the Linux and Windows desktops yet (no device or GTK in the sandbox).
