# Design

UwULock looks like every UwU app because it is built from the suite's design
package, [@uwusuite/design](https://github.com/MinifyX/UwUSuite-Design): its
tokens, UwU Sans and the font picker, light, dark and high contrast, the
motion rules, the icons (Lucide through `Icon` and `ICONS`), the components
(Button, Dialog, Switch, Segmented, Select, SettingRow, Toaster, TitleBar,
Wordmark, …) and Nyu. The rules for all of that live there, in its `docs/`
(color, typography, icons, components, window, motion, nyu, tone), and the way
an app moves onto it in its
[docs/migration.md](https://github.com/MinifyX/UwUSuite-Design/blob/main/docs/migration.md).

This page is only about what is special about UwULock. A password manager is
opened for a few seconds at a time: find, copy, gone. The interface is built
for exactly that.

## Where things are

- `apps/desktop/src/styles/index.css` imports Tailwind, the package's
  `tailwind.css` and `font-picker.css`, then the app's own sheets into
  Tailwind's `components` layer (a utility class always wins over them).
- `styles/app.css`: the pieces only a vault needs, on the package's tokens —
  fields and checkboxes, the cards of rows an item is shown in, chips, the
  settings layout, the update hint, phone safe areas.
- One sheet per screen: `screens.css` (login, lock), `vault.css` (sidebar,
  list, tiles, menus), `detail.css` (an item and its editor), `generator.css`,
  `extras.css`, `wifi.css`, `passkeys.css`, `moving.css`, `health.css`,
  `suite.css`, `phone.css` (the phone layout, last).
- Theme, contrast and motion: Settings → Darstellung, through the package's
  `useAppearance()`; `/boot.js` (the package's `bootScript()`, a file because
  the CSP allows no inline script) puts them on `<html>` before the first
  paint. UwULock is dark until the person picks something.
- The font: Settings → Darstellung → Schrift, the package's choices and
  `applyUiFont()`, per device. A stored font that is no longer offered falls
  back to UwU Sans.

The browser extension and the installer still use the app's old styles from
their own `legacy/` folders until they move to the package too.

## What is UwULock's own

- **Values read exactly as stored.** Passwords, card numbers, one-time codes,
  fingerprints and keys are monospace with JetBrains Mono's ligatures off (the
  package's `.uwu-mono` rule, the app's `.mono`). A revealed password colours
  digits pink and symbols violet, so `l1I|` can be told apart.
- **Item tiles**: the first letter for logins, the kind's icon for everything
  else, in the package's six avatar tints. **No favicons by default**: fetching
  them would tell a server which sites are in the vault (only UwULock Server's
  site icons, and own icons, encrypted).
- **Details**: cards of rows, label above value, actions on the right — show,
  copy, open in the browser, quiet until the row is hovered (always there on a
  phone). Favourite, trash and **Bearbeiten** sit next to the name; editing
  opens a wide dialog, where a value the page never saw says so ("Bleibt, wie
  es ist") instead of showing dots that could be typed over.
- **The one-time code** counts down in a ring (an SVG of the app's, not an
  icon) and turns to the warning colour for its last five seconds; in its last
  ten, the next code shows below it with its own copy button.
- **Dialogs** are the package's `Dialog` (components/Modal.tsx), with three
  rules of the app's: a click beside a dialog doesn't close it (it would throw
  away what was typed), focus starts on the safe choice (`data-autofocus`,
  never on a `data-secondary` button), and Android's back button closes the
  top one (`lib/backStack.ts`). Security warnings use `tone="warning"`.
- **Checkboxes** are native inputs in the suite's colours (a pink filled box
  with a tick that pops in); on/off settings are the package's `Switch`.
- **Layout**: sidebar | list | details on a computer; one pane at a time below
  700 px (the package's `phone:` breakpoint): the list, the item over it, the
  folders in a drawer. On a phone the title bar is an app bar without window
  buttons, and iOS keeps out of the notch and the home indicator.
- **macOS** keeps the custom title bar for now; the native title bar, menu bar
  (`setMacMenu()`), ⌘W and the quit guard come next (package `docs/macos.md`).

## Nyu, the padlock cat

Nyu's hull is a **padlock** here: the lilac shackle arches up between her
ears, a keyhole sits on her forehead, and the plate on the lock body is her
face. The ears, the face, the sticker edge and the palette are the package's
(`NyuEars`, `NyuFace`, `Sticker`, `NYU`); the padlock shell is
`components/nyu/Nyu.tsx`, and the package's catalogue draws the same cat
(`shell="lock"`).

**Scenes** (`NyuScene`, 320 × 220, the app's own):

| Scene   | When                               |
| ------- | ---------------------------------- |
| Welcome | The login screen                   |
| Keys    | Two-step login                     |
| Sleepy  | The lock screen, an empty section  |
| Vault   | No item picked — Nyu holds her key |
| Pick    | An empty vault                     |
| Puzzled | A search without results           |

**Appearances** (`nyu/stage.tsx`): short scenes, about a second and a half,
in a corner of the window. They take no clicks, never move the layout, and a
new one replaces the playing one.

| Appearance | When                              | Reduced motion |
| ---------- | --------------------------------- | -------------- |
| saved      | An item saved: a tick on its card | still          |
| copied     | A value copied (at most every 6s) | none           |
| trashed    | An item into the trash            | still          |
| shared     | A Send made: a paper plane        | still          |
| unlocked   | The vault opened                  | none           |
| checked    | The password check done           | still          |
| generated  | _Neu würfeln_: the die tumbles    | none           |

Waiting shows `NyuBusy`: Nyu bobbing next to three blinking dots.

## Tone of voice

The suite's tone (package `docs/tone.md`), with one rule that is not
negotiable here: **security is never playful.** A wrong master password, a
refused login, a re-prompt, a certificate problem: no kaomoji, no Nyu.
