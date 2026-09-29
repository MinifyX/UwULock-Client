# The UwULock browser extension

[Deutsch weiter unten](#die-browser-erweiterung)

UwULock for the browser: your vault on **UwULock Server, Vaultwarden, a self-hosted Bitwarden,
bitwarden.com or bitwarden.eu** in Chrome, Edge, Brave, Vivaldi, Opera and Firefox. One code
base, two packages, both on every
[release](https://github.com/MinifyX/UwULock-Client/releases) (from 0.3.0-beta.1 on):

| Browser                                                        | File under **Assets**            |
| -------------------------------------------------------------- | -------------------------------- |
| Chrome, Edge, Brave, Vivaldi, Opera (Chromium 116 or newer)    | `UwULock-extension-chromium.zip` |
| Firefox 128 or newer (Developer Edition, Nightly, LibreWolf …) | `UwULock-extension-firefox.xpi`  |

The extension is not in the Chrome Web Store or on addons.mozilla.org, and it doesn't update
itself: install the newer file from the next release the same way. `SHA256SUMS.txt` on the
release checks the download (see the [install guide](install.md)).

## What it does

- **Log in** to your server — self-hosted, bitwarden.com or bitwarden.eu — with two-step login
  (authenticator app, email code, YubiKey OTP, a security key through your server's WebAuthn
  page) and Bitwarden's check of new devices. Several accounts, one open at a time.
- **Unlock** with the master password, or a PIN you set up in this browser. It locks after the
  time you chose, when the browser closes, when the computer's screen locks (a setting, on by
  default), or right when the popup closes.
- **The vault in the popup**: the logins of the page in the tab first, then everything —
  search, favourites, kinds, folders, the trash. Copy username, password and the one-time code
  (live, with its countdown), reveal, open the site. Create, edit and delete logins, notes,
  cards and identities; the password history of an item; a generator for passwords and
  passphrases that keeps its settings.
- **Filling**: a small UwULock button in login fields opens the matching logins; the popup, the
  context menu and **Ctrl+Shift+L** fill too. Addresses match the way Bitwarden matches them
  (domain, host, starts with, exact, regular expression, never) with Bitwarden's equivalent
  domains and your own, a little stricter where that keeps a login on its site (see below).
  Cards and addresses fill checkout forms. Logins in two steps (the username first,
  the password on the next page) and one-time code fields work; when a page has no field for
  the code, it is copied instead.
- **Saving**: after you sign in somewhere, a bar offers to save the login — or to update the
  password if it changed. Signed in while the vault was locked? The popup asks after you
  unlock it. A login is only ever saved into the account it was typed for: if you open another
  account before answering, the bar asks again for that one, and logins typed while locked wait
  for their own account.
- **Passkeys**: sites that use passkeys can create them in your vault and sign you in with
  them, in Bitwarden's format — so passkeys made by Bitwarden's apps work here, and the other
  way round. Cancel, and the browser's own passkey dialog takes over.
- **Live**: changes from your other devices arrive at once through the server's notification
  hub; if that connection is down, the extension asks the server every minute.
- German and English, following the browser (or the setting).

### How it keeps your vault safe

- The crypto is **uwulock-core**, the same Rust code as the desktop app and UwULock Server's
  web vault, compiled to WebAssembly. Your master password never leaves the browser; the
  server gets only its hash. A server that asks for a weaker key derivation than your last
  login here used is refused before anything is sent, even after it logged the extension out;
  if you lowered it yourself, log the account out here (or forget the setting on the login
  screen) and log in again.
- Keys live only in the extension's background. While the vault is unlocked, the user key is
  also kept in the browser's session storage — in memory, never on disk, closed to web pages —
  so the background can open the vault again when the browser ends and restarts it. On disk
  there is only what the server stores anyway: the encrypted vault and the session tokens.
  Each account is kept by its server and its user id, so a server that names another server's
  user id gets an entry of its own and never takes over that account's key or its "remember
  this device" token.
- A PIN works until the browser closes, unless you let it work after a restart too. Then the
  user key sits on disk under the PIN alone, and a copy of the browser profile lets somebody guess
  it offline, so that option wants six characters or more and says so. Five wrong PINs in a row
  remove the PIN; they are counted on disk, one try at a time, so a restart gives no new tries.
- Web pages get nothing unless you pick an item. The list of the inline menu is a page of the
  extension in a frame under the field: the web page never sees the names in it (only how many
  there are, for the button), and can't make up a pick, which goes from that frame straight to
  UwULock. A pick becomes a one-time token for the field's frame alone. Then only that frame gets
  the item's values, and only if the item's address matches **that frame's own address** — a login
  never reaches an iframe of another site. Picking an item in the popup for a page it doesn't
  match fills the page itself, never its frames. Cards and addresses reach only the page itself
  and frames of its own origin. Plain `http://` pages ask before filling.
- An item with the master-password re-prompt asks for the master password every time it is
  filled, as Bitwarden does: in the popup, and the page's menu, the context menu and the
  shortcut open the popup for it. Seeing or copying its values in the popup asks once until the
  vault locks.
- Matching addresses is a little stricter than Bitwarden's: "starts with" also wants the same
  origin (`https://bank.example` doesn't match `https://bank.example.evil.test/`); a regular
  expression is tried only in the top frame, and only when it is short and can't run for long;
  Bitwarden's global list of equivalent domains comes with the extension, and the server only
  says which of its groups you switched off. Your own equivalent domains come from the server
  unencrypted, so a login that matches a page only through one of them is listed there but not
  filled by the shortcut.
- The button and the save bar sit in closed shadow roots and ignore clicks the page fakes.
  Against a page that lays a decoy over them or over the menu's frame (clickjacking), they take a
  click only when the pointer went down on them after they had been shown, unchanged and
  uncovered, for half a second — on Chromium checked by the browser itself. The menu's frame
  checks the same inside, and a pick in it counts only when the page's side saw the frame
  uncovered too. In an embedded frame of another site, Firefox can't tell, so there the menu only
  opens UwULock's window. Chromium gives the frame a new address each session
  (`use_dynamic_url`), so pages can't tell the extension is installed; Firefox's address is fixed
  per profile but random.
- Passkeys are only offered for the site that asks: its relying party is checked against the
  page's own address by WebAuthn's rules, on `https://` pages (and `http://localhost`).
- The clipboard is cleared after 30 seconds (adjustable). No remote code, a strict content
  security policy, and the extension asks for access to your server only when you log in.
  Requests to your server never follow a redirect (one would send the login form or the session
  somewhere else), and no answer is read past a size limit.

## Install in Chrome, Edge, Brave, Vivaldi or Opera

1. Download `UwULock-extension-chromium.zip` and unpack it into a folder that stays (for
   example `Documents/UwULock-extension`) — the browser loads it from there every time.
2. Open the extensions page: `chrome://extensions` (Edge: `edge://extensions`, Brave:
   `brave://extensions`, Vivaldi: `vivaldi://extensions`, Opera: `opera://extensions`).
3. Switch on **Developer mode** (top right; in Edge on the left).
4. Click **Load unpacked** and pick the folder from step 1.
5. Pin UwULock to the toolbar (the puzzle piece → pin) and click it to log in. The browser
   asks once whether UwULock may reach your server: allow it.

Chromium may remind you now and then that developer-mode extensions are active; that's the
price of not being in the Web Store. To update, unpack the new zip over the old folder and
click the reload arrow on the extension's card.

## Install in Firefox

Firefox only installs add-ons that Mozilla signed — and this `.xpi` is **not signed**. So,
honestly:

- **Firefox Developer Edition, Nightly, LibreWolf** (and other builds that allow it): open
  `about:config`, set `xpinstall.signatures.required` to `false`, then open `about:addons` →
  gear icon → **Install Add-on From File…** and pick `UwULock-extension-firefox.xpi`. It stays
  installed.
- **Regular Firefox (release, ESR)** ignores that setting. You can load the extension
  **temporarily**: `about:debugging#/runtime/this-firefox` → **Load Temporary Add-on…** → pick
  the `.xpi`. It is gone when Firefox restarts, and you log in again after that.

When Firefox asks for permissions, allow access to your websites (for filling) and, at login,
to your server. If the popup closes while Firefox asks, open it again and log in once more —
the permission is there now.

## Using it

- **Filling**: click into a login field and then the pink button in it (or press ↓), pick the
  login. Or open the popup and click **Fill**, or press **Ctrl+Shift+L** (the browser's
  extension shortcut settings can change it).
- **Context menu**: right-click in a page → UwULock → fill a login, card or address, copy a
  password, or generate one.
- **Settings** in the popup: lock timeout, locking with the computer, PIN, clipboard clearing, the inline button, the save
  prompt, copying the one-time code, passkeys, the default match detection, language and theme,
  and your accounts.

## For UwULock Server

On a UwULock Server the extension asks `GET /uwu/v1/info` at login and keeps the feature list.
UwULock's own extras switch on per feature when the server lists them; with Vaultwarden and
Bitwarden the extension stays a plain Bitwarden client (sharing as a Send works there too).
Everything goes through the extension's background: the popup and web pages never talk to the
server and never see a key.

- **Icons** in the vault list: an item's own icon (encrypted, opened in the extension — a
  personal item's with your account's extras key, an organisation's item's with its key), else
  the icon your server fetched for the site, else the letter tile. Only your own server is
  asked, never the site. Settings → UwULock Server → _Icons in the list_ switches them off.
- **Masked addresses** from UwUMail (once your account is connected in the web vault, Settings →
  Masked addresses): in the generator (_Masked address_), with the **@** button next to an
  item's username in the editor, and as _New masked address_ in the inline menu of a username or
  email field — sign-up forms included. The address is made for the site of the tab you are on
  and typed into the field.
- **Share as a Send** (the share button of an item): pick the values (never the authenticator
  key), how long the link lives (a day by default), how often it opens (once by default) and an
  optional password. The link is copied from the popup; on UwULock Server it uses your default
  send domain if you chose one.
- **File requests** (Settings → UwULock Server → _File requests_): your links with their label,
  until when they run and what arrived; copy a link again, or open it in the web vault, where
  you make and manage them.

The extension only _opens_ the extras key. If your account has none yet, the web vault or the
desktop app makes it the first time you use an extra there; until then own icons of personal
items and file requests stay off.

## Building it yourself

```bash
pnpm install
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked   # the version crates/uwulock-wasm pins
pnpm --filter @uwulock/extension wasm      # uwulock-core → apps/extension/src/wasm/pkg
pnpm --filter @uwulock/extension build     # dist/chromium, dist/firefox, target/extension/*
pnpm --filter @uwulock/extension test      # unit tests (vitest)
pnpm --filter @uwulock/extension e2e       # end to end: Chromium + a UwULock Server (Docker)
```

The end-to-end test runs the built extension in Playwright's Chromium against UwULock Server's
release image (or `UWULOCK_SERVER_BIN=<binary>`); Chromium needs its system libraries
(`playwright-core install --with-deps chromium`, or run it in the
`mcr.microsoft.com/playwright` image).

---

# Die Browser-Erweiterung

UwULock im Browser: dein Tresor auf **UwULock Server, Vaultwarden, einem selbst gehosteten
Bitwarden, bitwarden.com oder bitwarden.eu** in Chrome, Edge, Brave, Vivaldi, Opera und
Firefox. Beide Pakete liegen bei jedem
[Release](https://github.com/MinifyX/UwULock-Client/releases) (ab 0.3.0-beta.1):
`UwULock-extension-chromium.zip` für Chromium-Browser (ab Version 116),
`UwULock-extension-firefox.xpi` für Firefox (ab 128). Die Erweiterung steht in keinem Store und
aktualisiert sich nicht selbst: Die neuere Datei aus dem nächsten Release installierst du genauso.

Sie meldet dich an (mit zweistufiger Anmeldung), entsperrt mit Master-Passwort oder PIN, zeigt
zuerst die Logins der Seite im Tab und dann den ganzen Tresor, füllt Logins, Karten und Adressen
aus (Knopf im Feld, Popup, Kontextmenü, **Strg+Umschalt+L**), bietet nach dem Anmelden an, den
Login zu speichern oder das Passwort zu aktualisieren, und speichert Passkeys in deinem Tresor –
im Format von Bitwarden, sodass Passkeys aus Bitwardens Apps hier funktionieren und umgekehrt.
Dein Master-Passwort verlässt den Browser nie, Schlüssel gibt es nur im Hintergrund der
Erweiterung, und eine Webseite bekommt nur die Werte des Eintrags, den du ausgewählt hast – und
nur, wenn seine Adresse zu genau diesem Frame passt.

Mit UwULock Server kommen die Extras dazu, sobald der Server sie anbietet: Icons in der Liste
(eigene Icons und die, die dein Server lädt), maskierte Adressen von UwUMail im Generator, im
Editor (**@**) und im Menü von Benutzername- und E-Mail-Feldern, Einträge als Send teilen (auch
mit Vaultwarden und Bitwarden; nie mit dem Einmal-Code-Schlüssel) und deine Dateianfragen zum
Nachsehen und Link-Kopieren.

## In Chrome, Edge, Brave, Vivaldi oder Opera installieren

1. `UwULock-extension-chromium.zip` herunterladen und in einen Ordner entpacken, der bleibt
   (etwa `Dokumente/UwULock-Erweiterung`) – der Browser lädt sie jedes Mal von dort.
2. Die Erweiterungsseite öffnen: `chrome://extensions` (Edge: `edge://extensions`, Brave:
   `brave://extensions`, Vivaldi: `vivaldi://extensions`, Opera: `opera://extensions`).
3. **Entwicklermodus** einschalten (oben rechts, in Edge links).
4. **Entpackte Erweiterung laden** und den Ordner aus Schritt 1 wählen.
5. UwULock an die Symbolleiste anheften und anklicken, um dich anzumelden. Der Browser fragt
   einmal, ob UwULock deinen Server erreichen darf: erlauben.

Zum Aktualisieren die neue Zip über den alten Ordner entpacken und auf der Karte der Erweiterung
auf den Neu-laden-Pfeil klicken.

## In Firefox installieren

Firefox installiert nur Add-ons, die Mozilla signiert hat – und diese `.xpi` ist **nicht
signiert**. Ehrlich gesagt heißt das:

- **Firefox Developer Edition, Nightly, LibreWolf** (und andere Builds, die es erlauben): in
  `about:config` `xpinstall.signatures.required` auf `false` setzen, dann `about:addons` →
  Zahnrad → **Add-on aus Datei installieren …** und `UwULock-extension-firefox.xpi` wählen. Sie
  bleibt installiert.
- **Normales Firefox (Release, ESR)** ignoriert diese Einstellung. Dort geht es nur
  **vorübergehend**: `about:debugging#/runtime/this-firefox` → **Temporäres Add-on laden …** →
  die `.xpi` wählen. Nach einem Neustart von Firefox ist sie wieder weg.

Wenn Firefox nach Berechtigungen fragt: den Zugriff auf Websites (zum Ausfüllen) und beim
Anmelden auf deinen Server erlauben. Geht das Popup dabei zu, öffne es wieder und melde dich noch
einmal an – die Berechtigung ist dann da.
