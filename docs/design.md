# Design

Clean, bright, soft — with a wink. Same design system as
[UwUSSH](https://github.com/MinifyX/UwUSSH-Client),
[UwURDP](https://github.com/MinifyX/UwURDP-Client) and
[UwUMail](https://github.com/MinifyX/UwUMail-Client), same cat, one confident
bubblegum pink. A password manager is opened for a few seconds at a time:
find, copy, gone. The interface is built for exactly that.

## Color

Tokens come from UwUMail unchanged, including the `--uwu-*` naming, and live in
`apps/desktop/src/styles/tokens.css`. Components never use raw hex values —
with one exception: the six item tile colours in `vault.css`, a fixed palette
with a light and a dark variant each.

| Token              | Light     | Dark      | Use                                              |
| ------------------ | --------- | --------- | ------------------------------------------------ |
| `--uwu-canvas`     | `#f8f4f6` | `#141016` | App background, list and detail panes            |
| `--uwu-surface`    | `#ffffff` | `#1c171f` | Sidebar, cards, dialogs                          |
| `--uwu-elevated`   | `#fcf8fa` | `#241e28` | Hover rows, the account card                     |
| `--uwu-ink`        | `#1c1420` | `#f8f2f6` | Primary text                                     |
| `--uwu-muted`      | `#716672` | `#b3a8b3` | Secondary text, labels                           |
| `--uwu-pink`       | `#ff4d8d` | `#ff7fac` | **Brand.** Selection, focus, digits in passwords |
| `--uwu-pink-solid` | `#e11d74` | `#ff7fac` | Filled buttons with text                         |
| `--uwu-pink-tint`  | `#ffe4ef` | `#3a1a2a` | Selected item, active section                    |
| `--uwu-alarm`      | `#8e5510` | `#d8a25c` | Errors, a one-time code about to expire          |

**Why two pinks?** White text on `#ff4d8d` reaches only 3.1:1. Filled buttons
therefore use `#e11d74` (4.5:1, WCAG AA).

## Type

- **UwU Sans** for the interface, as in every UwU app: UwUMail's font
  (Atkinson Hyperlegible, SIL OFL 1.1, see `brand/fonts/uwu-sans/`), set a
  touch tighter. It has no ligatures (`:3` and `<3` stay as typed), and
  contextual alternates are off on the whole page so JetBrains Mono never joins
  characters of a vault value: a value must read exactly as stored.
- **Font choice** (Settings → Appearance, per device; the extension per
  browser): UwU Sans, Manrope (UwULock's font until 0.5), Rubik, DM Sans or
  the system's. All bundled; nothing loads from the network. One variable
  `--uwu-font` on `<html>` carries it (`lib/fonts.ts`).
- **Monospace** for passwords, card numbers, one-time codes, fingerprints and
  keys — anything to compare character by character. A revealed password
  colours digits pink and symbols violet, so `l1I|` can be told apart.
- Sizes: 12 caption · 13 meta · 14 body/list · 20 one-time code · 22 titles.

## Layout

```
┌─────────────┬────────────────────┬───────────────────────────────┐
│ Alle        │ [ Suche (Strg+F) ] │  [G]  GitHub ★                │
│ Favoriten   │ ALLE EINTRÄGE    8 │  Login · Privat               │
│ TYPEN       │ F FritzBox         │ ┌───────────────────────────┐ │
│ Logins      │ G GitHub      ⏱ ★ │ │ Benutzername     nyu  ⧉  │ │
│ Karten      │ S Synology NAS     │ │ Passwort     •••••  👁 ⧉  │ │
│ ORDNER      │ V Vaultwarden   🔒 │ │ Einmal-Code  123 456 ◔ ⧉ │ │
│ HOMELAB     │                    │ └───────────────────────────┘ │
│ ─────────── │                    │  Website · Notizen · Felder   │
│ (N) nyu@…  ⟳│                    │                               │
└─────────────┴────────────────────┴───────────────────────────────┘
```

- Custom title bar like every UwU app, with the generator (dice), lock and
  settings next to the window buttons.
- **Sidebar**: all items, favourites, types, folders, organisations with
  their collections, the trash; the account, its sync state and the way to the
  other accounts at the bottom.
- **List**: a tile per item — the first letter for logins, the kind's icon
  for everything else. **No favicons**: fetching them would tell a server
  which sites are in the vault. Arrow keys move, typing in the search field
  filters by name, username and host.
- **Details**: cards of rows, label above value, actions on the right —
  show, copy, open in the browser. Favourite, trash and **Bearbeiten** sit next
  to the name; editing opens a wide dialog, where a value the page never saw
  says so ("Bleibt, wie es ist") instead of showing dots that could be typed over. Secrets are dots until the eye is clicked.
  The one-time code counts down in a ring and turns amber for its last five
  seconds; in its last ten, the next code shows below it, small, with its
  own copy button. Only the first website shows; the others wait behind
  "+2 weitere Websites". A login's passkeys get a card of their own (site,
  user, since when, delete). The renewal reminder row only shows when it
  is switched on in the editor.
- **Controls**: checkboxes as in UwUMail — a pink filled box with a tick that
  pops in; on/off settings are switches (`role="switch"`, a pill with a
  knob). The editor's favourite is a star next to the name.
- **Login and lock** get the whole window: Nyu on the left and a card on the
  right to log in; Nyu asleep on a centred card to unlock.

## Nyu, the mascot

Nyu is the same cat as in UwUMail, UwUSSH and UwURDP — this time her hull is
a **padlock**. The lilac shackle arches up between her ears, a keyhole sits on
her forehead, and the plate on the lock body is her face: UwU eyes, `w` mouth,
blush. Plate, eyes and mouth sit exactly where the other hulls have them, so
every mood and scene fits.

- **Sticker style**, unchanged. Plum outlines `#4B1D3F`, pink body `#FF6FA6`,
  light plate `#FFB8D3`, lilac shackle `#C9B6F0`, a white die-cut edge.
- **App icon** (website, GitHub, macOS Dock): Nyu slightly tilted on
  UwUMail's pastel pink tile, the night-blue plate with the pink UwU face and
  a golden keyhole. Here the big star is top left, a small star right, the
  heart bottom left.
- **Taskbar icon**: Nyu alone, upright, no tile: shackle, ears, and a big
  golden keyhole on the dark plate, so it reads as a lock at 32 px.
  `uwulock-taskbar-icon-small.svg` takes over at 16 and 24 px.
  `node scripts/icons.mjs` regenerates all desktop icons from these three.

**Scenes** (`NyuScene`, 320 × 220):

| Scene   | When                               |
| ------- | ---------------------------------- |
| Welcome | The login screen                   |
| Keys    | Two-step login                     |
| Sleepy  | The lock screen, an empty section  |
| Vault   | No item picked — Nyu holds her key |
| Pick    | An empty vault                     |
| Puzzled | A search without results           |

**Appearances** (`nyu/stage.tsx`): short scenes, about a second and a half,
in a corner of the window, as in UwUMail. They take no clicks, never move the
layout, and a new one replaces the playing one.

| Appearance | When                              | Reduced motion |
| ---------- | --------------------------------- | -------------- |
| saved      | An item saved: a tick on its card | still          |
| copied     | A value copied (at most every 6s) | none           |
| trashed    | An item into the trash            | still          |
| shared     | A Send made: a paper plane        | still          |
| unlocked   | The vault opened                  | none           |
| checked    | The password check done           | still          |
| generated  | _Neu würfeln_: the die tumbles    | none           |

Reduced motion is the app's setting, else the system's
(`prefers-reduced-motion`). Waiting shows `NyuBusy`: Nyu bobbing next to
three blinking dots, still with reduced motion.

## Tone of voice

Warm and a little playful: kaomoji now and then, small jokes in empty states.
German strings are the source and use "du"; English follows.

1. **Information first.** The joke never replaces what happened or what to do.
2. **Short.** One kaomoji at most, never in buttons that act on data.
3. **Kind.** Never mock the user; the app laughs at itself.
4. **Security is never playful.** A wrong master password, a refused login, a
   re-prompt, a certificate problem: no kaomoji, no Nyu. Not negotiable.
