# UwULock Server's extras in the desktop app

With an account on a UwULock Server, the desktop app shows what that server
offers beyond Bitwarden: item icons, earlier versions of items, reminders to
renew a password, file requests, masked e-mail addresses and travel mode. Each
appears only when the server has it switched on. An account on Bitwarden or
Vaultwarden sees none of it — except **Share as Send**, which works there too.

Everything stays end-to-end encrypted: what the server keeps for these extras
it keeps encrypted, and the app decrypts it on your computer.

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
  stores it as an own icon. The server never contacts your local network.
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
  _Downloads_ folder after you confirm.
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

## Travel mode

While travel mode is on, the items in the folders marked for it are hidden on
every device, and the title bar says so, with how many are hidden. Switching it
on and off happens in the web vault, with your master password and your second
factor.

## Families and organisations

Collections of a family or organisation appear in the sidebar under its name,
with _All items_ for everything of it. The external-link button next to the
name opens its management in the web vault.

## If the extras key is lost

The extras (own icons of personal items, file request names) are encrypted
under a key of their own, kept for your account's key pair. If an official
Bitwarden app replaced that key pair, the app says the key can't be opened any
more; the web vault offers to start over.
