# UwULock Server's extras in the desktop app

With an account on a UwULock Server, the desktop app shows what that server
offers beyond Bitwarden: item icons, earlier versions of items, reminders to
renew a password, file requests, masked e-mail addresses and travel mode. Each
appears only when the server has it switched on. An account on Bitwarden or
Vaultwarden sees none of it — except **Share as Send**, which works there too.

Everything stays end-to-end encrypted: what the server keeps for these extras
it keeps encrypted, and the app decrypts it on your computer.

## When the admin switches an extra off

Since UwULock Server 0.6.0-beta.2 an admin can switch each extra off
(_Admin portal → Features_). The app follows at once — the server tells it
through the live connection, and every sync asks again:

- menus, buttons, marks and windows of that extra go away (an open window
  closes, the _Due for a new password_ list goes back to all items);
- nothing of it is fetched any more, and what the sync still brings for it
  (reminders, own icons, masked addresses, travel mode, file request badges)
  isn't shown — it stays in the offline copy, so it is all back when the admin
  switches it on again;
- should a click still reach the server in the meantime, the app says calmly
  that the server doesn't offer it any more and catches up; no error, no
  retrying.

A server before 0.6.0-beta.2 has no switches: the app offers what it lists.

## Item icons

- **Website icons** come from your UwULock Server, which fetches and caches
  them. The server then knows which websites are in your vault; if you'd rather
  it didn't, switch off _Settings → Appearance → Website icons_. The app never
  asks a website itself.
- **Own icons**: hover over an item's icon in its details and click the small
  picture button. _Choose a picture…_ takes a PNG, JPEG, WebP or SVG, cuts a
  square from its middle and stores it at 128 × 128 pixels at most, encrypted.
  An item in an organisation gets its icon under the organisation's key, so
  every member sees it.
- **Devices on your network** (a NAS at `nas.local`, a router at
  `192.168.1.1`, a name without dots): _Get the icon from the device_ fetches
  it from the device itself — its start page's icon or `/favicon.ico` — and
  stores it as an own icon. The server never contacts your local network, and
  neither does this fetch leave it: icons the start page names elsewhere are
  skipped, and a name is only used when it resolves to a local address.
  Self-signed certificates are accepted for this one request, since the result
  is only ever read as a picture.

## Earlier versions

Every change to an item keeps the state before it on the server. In the item's
details, _Earlier versions_ lists them, newest first, and shows what differs
from now; passwords and other secrets stay dots until you click the eye.
_Restore_ brings a version back (the current state becomes a version itself).
If the item was changed on another device in the meantime, the app syncs and
asks you to look again first. How many versions are kept, and for how long, is
the server admin's setting.

## Renewal reminders

_Renew password_ in a login's details reminds you every so many months
(counted from the last password change) or on a date. Items that are due get a
bell in the list, and the sidebar gets _Due for a new password_ with all of
them. The server may also send you a mail; it never names the item.

## File requests

_Sidebar → Extras → File requests_ makes a link through which somebody without
an account uploads files and a message to you. It is encrypted in their browser
for your account's key; only you can open it.

- Choose a title and a note for them, how long the link works (7 days by
  default), how many uploads, files and megabytes it takes, and optionally a
  password.
- _New link_ (when changing a request) makes every link handed out so far stop
  working. A password has to be entered again then, because it is tied to the
  link.
- What arrived shows under the request. The sender's name and address are what
  they typed in — nobody checked them. Files are saved decrypted into your
  _Downloads_ folder after you confirm, marked as downloaded from the internet
  (on Windows and macOS), so the system checks them as it checks a browser
  download. A program, script, installer or document with macros gets a clear
  warning first.
- _Take over as item_ makes a secure note with the message and the sender and
  moves the files into it as attachments.

## Masked addresses

With your account connected to UwUMail (in the web vault, _Settings → Masked
addresses_), you get a separate e-mail address for every website that forwards
to your mailbox:

- in the item editor, the mask button next to the username makes one for the
  item's website and puts it in;
- the password generator (Ctrl+G) has a _Masked address_ mode;
- _Sidebar → Extras → Masked addresses_ lists them; switch them off and on,
  or delete them for good.

Deleting an item for good asks whether to switch its masked address off
(ticked by default), so no more mail arrives for a login that is gone.

## Share as Send

The paper plane in an item's details shares the values you pick — never the
one-time code key — as a Send: by default deleted after one day and opened at
most once, optionally with a password. On a UwULock Server with mail, _Only for
these addresses_ lets only the addresses you list open it, with a code by
e-mail; with send domains you choose which address the link uses.

An item in an organisation that hides its passwords from you (the collection's
_Hide passwords_) can't share its password, one-time code key, hidden fields,
card number and code or SSH private key: official Bitwarden apps don't let them
leave the device either.

### Entry Sends

Shared as an _entry_, a Send shows on UwULock's Send page as the entry it is:
the fields with copy buttons, every website, hidden values behind a click, and
— if you included it — the live one-time code with its countdown. Never the
one-time code key, never a QR code of it. It stays a plain text Send, so
Bitwarden's apps open it too. The contract (`uwulock-core`, `entry_send`; the
web vault's WASM and the apps share it):

- Send type text. The text is the readable lines — the name, then
  `label: value` for every chosen value — **without** the one-time code key,
  followed by one last line `uwulock-entry:v1:<base64url(JSON)>`.
- JSON: `{name, username?, password?, websites[], notes?, fields[{name, value,
hidden}], totp?}`. `totp` is the key (secret or `otpauth://` URI), only for
  making the codes; card, identity and SSH values travel as `fields`, the
  sensitive ones `hidden`.
- A page that finds the marker on the last line hides the raw text and shows
  the entry. No marker, another version (`v2`) or one that doesn't decode: the
  text shows as it is. Older Sends are untouched.

The desktop app, the phone apps and the browser extension make entry Sends
whenever the server is UwULock Server (_Share as Send_ on an item); with
Vaultwarden and Bitwarden they make plain text Sends as before. The choice
lists every website by its address, and _One-time code_ when the login has
one — with the hint that whoever can open the Send gets codes as long as it
exists. The apps don't open Sends themselves; the Send page does.

## Travel mode

While travel mode is on, the items in the folders marked for it are hidden on
every device, and the title bar says so, with how many are hidden. Switching it
on and off happens in the web vault, with your master password and your second
factor.

## SSH and Remote Desktop (UwUSSH, UwURDP)

_Sidebar → UwU apps_ has a section each for what UwUSSH and UwURDP sync
through UwULock Server (its suite vault, `docs/uwu-api.md` §6 of
UwULock-Server; feature `suite`, switch `suite.enabled`): hosts by workspace
and group, logins, keys, snippets, port forwards and known hosts. Everything
can be viewed, edited, created and deleted, on the computer and the phone.

- The records are opened and sealed in Rust (`src-tauri/src/suite.rs`, over
  `uwulock_core::suite`), kept for the rest of the unlock and pulled again from
  the last cursor when the realtime channel says `suite` changed. A secret (a
  password, a private key, a passphrase) reaches the window only when the eye
  is clicked; copying goes from Rust.
- An edit is the record's JSON with the changed fields laid over it, so fields
  of a newer UwUSSH or UwURDP survive; Rust checks that the app can still read
  it and that every new pointer leads to a record of the right kind. Its clock
  carries this device's id, made once per account on this device.
- `baseSeq` is the version the window showed. When the server has a newer one,
  nothing is overwritten: the newer one is taken over and the window says so.
  After a rekey (409 `space_changed`) the space is fetched again.
- Deleting works as in the apps: a host takes its port forwards along, its
  login and key stay; a group leaves its hosts without one; a login or key is
  deleted only once nothing points at it, with its secrets.
- `manifest` records, the command assistant's kinds and kinds this build
  doesn't know are neither shown nor written.
- If no app made the space yet, _Create the space_ makes it (a fresh key under
  the extras key); if an app was quicker, its space is taken.
- On a host: copy the command, an `.rdp` file (no password, drives off), and on
  a computer _Open in UwUSSH/UwURDP_ (`uwussh://connect/<id>`,
  `uwurdp://connect/<id>`; only the record's id travels).

## Password check

_Sidebar → Password check_ looks at every login with a password (not in the
trash, not archived) and lists what it finds, in groups:

- **In breaches** — Have I Been Pwned or XposedOrNot saw the password. Only
  the first five hex digits of its SHA-1 (HIBP) and the first ten of its
  Keccak-512 (XposedOrNot) go to your UwULock Server, which asks the sources;
  the password never leaves the device. _Check now_ / _Check again_ asks; the
  answers are kept (encrypted under the extras key, as the web vault keeps
  them) and count for every password that hasn't changed since. The server
  asks XposedOrNot about one prefix a second for everyone on it; when its
  queue is long it answers `busy` (429 with `Retry-After`), and the app waits
  that long and asks again (up to 8 times) — the check shows how far each
  source is and that it waits. Only a source that really fails marks the
  check as incomplete.
- **Breach after your last password change** — the site (or a domain above
  it) is on the server's list of breached sites (HIBP and XposedOrNot's
  public lists) with a breach in which passwords were taken on or after the
  day the password was last changed. The list comes whole; matching happens
  on the device.
- **Reused**, **weak** (below 50 bits) and **without https** (an `http://`
  address) — found on the device, on any server, Bitwarden and Vaultwarden
  included.
- **2FA possible, not set up** — the site offers codes from an authenticator
  app ([2FA Directory](https://2fa.directory/), mirrored by the server), and
  the item has none.

_Review one by one_ shows one card per login, the worst first, with
"3 of 12": swipe (or ← →) to the next or previous. Each card offers:

- **Open the page & change the password** — the site's
  `/.well-known/change-password` if your server found one (it checks; the app
  never asks the site, and builds the address from the login's own host —
  only whether it exists comes from the server), else the login's address, in
  your browser;
- **Generate & save a new password** — the generator; saving changes only
  the password, and the old one goes into the item's password history (five
  at most, as in Bitwarden);
- **Later** — off the stack until UwULock is closed;
- **Ignore** per problem — hidden on every device until undone, there or in
  the report's _Ignored_ list. The list is kept on the server, encrypted under
  the extras key, the same list as the web vault's.

**Addresses in breaches.** If your admin allows it, _Settings → Account →
Check addresses for breaches_ lets your server ask XposedOrNot about your
account address and the addresses used as usernames. Your server sends each
address **in plain text** to XposedOrNot for that — never passwords — and
keeps the answers a week, under a hash of the address. Switching it off
withdraws the consent for every UwULock app.

Each source is the admin's to switch on or off (UwULock Server 0.7,
`/uwu/v1/info` → `breaches`); what is off is left out. A server before 0.7
offers Have I Been Pwned only and keeps no ignore list (there's no _Ignore_
then); on Bitwarden and Vaultwarden the check finds what the device can.

## Families and organisations

Collections of a family or organisation appear in the sidebar under its name,
with _All items_ for everything of it. The external-link button next to the
name opens its management in the web vault.

## If the extras key is lost

The extras (own icons of personal items, file request names) are encrypted
under a key of their own, kept for your account's key pair. If an official
Bitwarden app replaced that key pair, the app says the key can't be opened any
more; the web vault offers to start over.

The server can't swap in a key of its own: both copies it keeps are made with
keys only your account has. If the key is a different one than this device
used before — because somebody started over, or the server lost the old one —
the app tells you once. A file request whose link would encrypt uploads for a
key that isn't yours shows no link; edit it with a new link or delete it.
