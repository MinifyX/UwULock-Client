<p align="center">
  <img src="brand/uwulock-app-icon.svg" width="112" alt="UwULock logo" />
</p>

<h1 align="center">UwULock</h1>

<p align="center">
  The password manager I build for myself, because every other one annoyed me. (◕‿◕✿)<br/>
  Vaultwarden · Bitwarden · TOTP · passkeys · Windows, macOS, Linux, Android, iPhone and a browser extension, beta
</p>

<p align="center">
  <a href="https://github.com/MinifyX/UwULock-Client/releases"><b>Download for Windows, macOS and Linux</b></a>
  ·
  <a href="docs/install.md"><b>How to install</b></a>
  ·
  <a href="docs/install.md#uwulock-installieren">Anleitung auf Deutsch</a>
</p>

---

## Why this exists

My passwords live in a Vaultwarden on my own box, and I like that. What I
don't like is the desktop app I open fifty times a day to get them out: it
looks like every other enterprise tool, and it has nothing to do with the rest
of the tools I use all day. So UwULock is the vault client in the UwUSuite
family — same cat, same pink, same installer — and one day it will be more
than a client: a self-hosted password manager of its own, with the things
UwUSSH and UwURDP need from a vault built in.

- **Just for fun.** No company, no team, no schedule, no promises. I work on it
  when I have time and feel like it.
- **Written with AI.** Almost all of the code is written with Claude, because
  I'm honestly not a great programmer. Not your thing? No hard feelings.
- **Use it, fork it, do what you want with it.** The license only asks one
  thing: if you pass on a changed version, its source stays open too.
- **No support.** Issues and pull requests are okay, but I might answer late or
  not at all.

## What it is

UwULock opens a **Vaultwarden** or **Bitwarden** vault — self-hosted,
bitwarden.com or bitwarden.eu — with the same encryption the official apps use.

- **Your master password stays here.** UwULock derives the key from it
  (PBKDF2 or Argon2id, whatever your account uses), and the server only ever
  gets a hash — exactly like Bitwarden's own apps. The crypto is checked
  against the known answers from Bitwarden's SDK.
- **Secrets stay in Rust.** The interface runs in a web view, and a password
  only reaches it when you click the eye. Copying goes from the vault straight
  to the clipboard, is kept out of the Windows clipboard history, and is gone
  again after 30 seconds.
- **Offline too.** The last sync stays on the device exactly as the server
  sent it, still encrypted, and opens with the master password without a
  network. Locking forgets everything decrypted.
- **Playful.** Nyu, the cat, is a padlock now, and pops up for a moment when
  something is saved, copied, shared or checked — still, or not at all, when
  your system asks for less motion. Security warnings are never playful.

> **Status: beta.** 0.5.0-beta.1 reads and writes: log in (with two-step
> login), browse, search, copy, one-time codes, passkeys — and create, edit and
> delete, with a private Vaultwarden and one at work side by side. It fits
> [UwULock Server](https://github.com/MinifyX/UwULock-Server) 0.8 (and works
> with older ones from 0.4 on): items with attachments, Sends and
> organisations come through the sync and stay intact.
>
> **What works.**
>
> - Log in to Vaultwarden, self-hosted Bitwarden, bitwarden.com and bitwarden.eu,
>   with PBKDF2 or Argon2id accounts, old ones (from before 2019) included.
> - Two-step login with an authenticator app, email codes and YubiKey OTP,
>   "remember this device", and Bitwarden's new-device check.
> - Logins, cards, identities, secure notes and SSH keys; folders,
>   organisations and collections, favourites, the trash. Items with their own
>   key and items that ask for the master password again.
> - Create, edit and delete items, folders and favourites (a star in the
>   editor); the trash and back out of it. A password you don't look at never
>   passes through the window, and what UwULock doesn't show — linked fields —
>   is handed back to the server untouched. An item shows its first website,
>   the others behind "+2 more", and a reminder to renew only when it is
>   switched on in the editor.
> - Passkeys: a login's passkeys with site, user and when they were made, and
>   deleting one. The apps offer them to the rest of the system too
>   ([passkeys](docs/passkeys.md)): Credential Manager on Android 14+, a
>   virtual security key on Linux (off until switched on), a plugin passkey
>   manager on Windows 11 (experimental, off until switched on) and an
>   AutoFill extension on iOS 17+ and macOS 14+, which is built in but only
>   works once the app is signed with an Apple developer team. None of them
>   has been tried on a real device yet.
> - Several accounts on one device: a private Vaultwarden and one at work,
>   each with its own vault, its own session and its own master password.
> - One-time codes (TOTP, also Steam) counting down right in the item; in a
>   code's last 10 seconds the next one shows below it, with its own copy
>   button.
> - Search, keyboard shortcuts like Bitwarden's (Ctrl+U, Ctrl+P, Ctrl+T), a
>   password generator, also right inside the password field, with a minimum
>   per kind of character (A–Z, a–z, 0–9, symbols) — the length grows to fit
>   them.
> - Auto-lock, clipboard clearing, sync on unlock and every five minutes.
> - A password check: weak, reused and `http://` logins, and a review that
>   goes through them one card at a time.
> - Wi-Fi networks as an item type of their own, shared as a QR code; on
>   Android joined with one tap.
> - On the phone too: Android (APK) and iPhone (unsigned IPA for
>   sideloading), with fingerprint or face unlock ([phones](docs/mobile.md)).
> - With UwULock Server 0.7: UwUSSH's and UwURDP's hosts, logins, keys,
>   snippets and port forwards in sections of their own, to view and edit.
> - UwU Sans, the UwU apps' font, and a font choice per device (Manrope,
>   Rubik, DM Sans or the system's); checkboxes and switches like UwUMail's.
> - Its own installer with Nyu, signed automatic updates, German and English.
>
> **With UwULock Server 0.6**: changes from your other devices
> arrive as they happen, over the server's realtime channel and delta sync (or
> Bitwarden's notification hub elsewhere); item icons, earlier versions of an
> item, reminders to renew a password, file requests, masked addresses from
> UwUMail, sharing an item as a Send (also only for given addresses, or on a
> send domain), families and travel mode — each where the server offers it.
> With UwULock Server 0.7 the password check also finds breached passwords
> (Have I Been Pwned, XposedOrNot), sites breached after your last change and
> sites that offer two-step login, opens the change-password page and keeps
> what you ignore in step with the web vault; when XposedOrNot is busy, the
> check waits as the server says instead of ending incomplete, and shows how
> far each source is. With UwULock Server 0.8 an item shared as a Send is an
> [entry Send](docs/uwu-extras.md#entry-sends): its page shows the entry with
> copy buttons and the websites you picked, and, if you tick it, live one-time
> codes — never the key on the page, but the key travels encrypted in the Send,
> so whoever has the link can read it out (UwULock asks before it goes along).
> UwULock locks with the computer, unlocks with Windows Hello, and moves a
> whole vault over from Bitwarden, attachments, Sends and organisations
> included. [Extras](docs/uwu-extras.md) ·
> [Moving from Bitwarden](docs/moving-from-bitwarden.md).
>
> **What doesn't, yet.** Attachments and Sends of their own in the desktop app
> (they move and stay intact), moving items into an organisation, Duo and
> FIDO2 as second step, SSO, Touch ID. The [roadmap](docs/roadmap.md) has the
> order.

## Install

Windows 10 or 11 (x64 and ARM), macOS 11 or newer, Linux (x86_64 and arm64).
Download the file for your system from the newest
[release](https://github.com/MinifyX/UwULock-Client/releases), run it, click
**Install** — no admin prompt. The [install guide](docs/install.md) has the
details. [Auf Deutsch](docs/install.md#uwulock-installieren).

Passkeys in browsers on Linux: the `.deb` and `.rpm` bring a small root helper,
`uwulock-uhid-broker`, which makes the virtual security key and nothing else
(UwULock itself never gets `/dev/uhid`), and switch its socket on. The Arch
package doesn't switch services on by itself:
`sudo systemctl enable --now uwulock-uhid-broker.socket`. A test build from
before the helper gave the person at the seat `/dev/uhid` itself; updating
takes that back (`setfacl`, or `chmod` without the acl package). Then switch
it on in the app: Settings → Security.
[Passkeys for the system](docs/passkeys.md#linux-a-virtual-security-key).

Phones: Android 10 or newer (`UwULock-android.apk`) and the iPhone with iOS 17
or newer (`UwULock-ios.ipa`, unsigned, for sideloading with your own Apple ID).
Vault, search, copy, one-time codes and the generator, the password check with
its swipe review, unlocking with a fingerprint or face. Passkeys for other apps
and browsers on Android 14 or newer (pick UwULock in Android's Settings →
Passwords, passkeys & accounts); on the iPhone only with a signed build.
[UwULock on Android and iPhone](docs/mobile.md).

## Browser extension

UwULock for Chrome, Edge, Brave, Vivaldi, Opera and Firefox, from one code base: log in (with
two-step login), unlock with the master password or a PIN, the page's logins first, copy and
one-time codes (the next one too, in a code's last 10 seconds), create and edit items, a
generator with minimums per kind of character, sharing an item as a Send (an entry Send on
UwULock Server), a font of your choice — and filling from a button in the login field, the
context menu or Ctrl+Shift+L, a bar that offers to save what you signed in with, and passkeys
kept in your vault in Bitwarden's format, listed and deleted in the item. It speaks to UwULock Server, Vaultwarden and
Bitwarden directly; the crypto is uwulock-core, compiled to WebAssembly.

It isn't in any store: `UwULock-extension-chromium.zip` and `UwULock-extension-firefox.xpi` are on
the [releases](https://github.com/MinifyX/UwULock-Client/releases). Chromium loads it in developer
mode; the Firefox file is unsigned, so it stays installed only in Developer Edition, Nightly or
LibreWolf and loads temporarily elsewhere. [How to install it](docs/extension.md).

## Project layout

| Path                                 | What lives there                                                             |
| ------------------------------------ | ---------------------------------------------------------------------------- |
| `apps/desktop`                       | The Tauri 2 app (React UI + Rust shell), for desktop, Android and iOS        |
| `apps/desktop/e2e`                   | End-to-end run of the real app against a toy Vaultwarden                     |
| `apps/setup`                         | The installer, updater and uninstaller, for all three systems                |
| `apps/extension`                     | The browser extension for Chromium and Firefox (Manifest V3, React)          |
| `crates/uwulock-core`                | Bitwarden's crypto and data formats, no network; also builds to WebAssembly  |
| `crates/uwulock-bitwarden`           | Bitwarden's and UwULock Server's protocol: login, sync, live updates, saving |
| `crates/uwulock-wasm`                | uwulock-core as WebAssembly, for the browser extension                       |
| `crates/uwulock-authenticator`       | CTAP2, CTAPHID, uhid and WebAuthn JSON for the system passkey providers      |
| `crates/uwulock-uhid-broker`         | Linux root helper that makes the virtual security key, and nothing else      |
| `crates/tauri-plugin-uwulock-mobile` | Android's and iOS's own APIs: biometric unlock, clipboard, system bars       |
| `brand/`                             | Nyu as a padlock: the UwULock icon, symbol, mono symbol                      |
| `docs/`                              | Vision, architecture, design, roadmap, install guide                         |
| `release-notes/`                     | What's new, per version                                                      |
| `scripts/`                           | Icons, building the setup, releasing                                         |

## Development

Requirements: Node.js 24 and pnpm 11 (`corepack enable`), Rust stable, and
[Tauri's prerequisites](https://tauri.app/start/prerequisites/).

```bash
pnpm install
pnpm tauri dev
```

No server at hand? A toy Vaultwarden for trying things out — plain HTTP on
`127.0.0.1:8087`, account `nyu@uwu.local`, master password `uwu-nyu-nyu-nyu`,
with logins, a card, an identity, a note, an SSH key and an organisation.
`UWU_2FA=1` turns on two-step login (email code `123456`):

```bash
cargo run -p uwulock-bitwarden --example dev_vaultwarden
```

`UWULOCK_DATA_DIR=<folder>` keeps a dev build away from your real login.

Checks:

```bash
pnpm typecheck && pnpm lint
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo check -p uwulock-core --target wasm32-unknown-unknown   # the web vault's build
node apps/desktop/e2e/run.mjs     # end to end, Windows
```

The browser extension: `pnpm --filter @uwulock/extension wasm` (needs `wasm-bindgen-cli`
0.2.129), then `… build`, `… test` and `… e2e` — see [the extension guide](docs/extension.md).

Against a real UwULock Server instead of the fakes (a checkout of UwULock-Server with a
`cargo build` done there): `scripts/live-server.sh ../UwULock-Server` starts it with a test CA,
registers an account per test and runs the ignored tests of `uwulock-bitwarden`'s `server`
module — delta sync, the realtime channel and Bitwarden's hub, the extras key (also after an
official key rotation), own icons, versions, reminders, file requests, Sends.
`LIVE_VAULTWARDEN=1` adds the move from a filled Vaultwarden (Docker), `LIVE_BROWSER=1` an
upload through the web vault's file request page (Playwright's Docker image).

`uwulock-core` is kept free of anything networked so the web vault of
UwULock-Server can run it compiled to WebAssembly: the same crypto, checked
against the same vectors from Bitwarden's SDK
([architecture](docs/architecture.md)). The wasm check needs
`rustup target add wasm32-unknown-unknown`.

The phone apps: `pnpm mobile:prepare`, then `pnpm tauri android build --apk`
or `pnpm tauri ios dev` — see [UwULock on Android and iPhone](docs/mobile.md#building).

The installer, with the app packed inside: `pnpm build:setup`. Releasing is
`pnpm release`; [release-notes/README.md](release-notes/README.md) has the steps.

## Documentation

- [Install guide](docs/install.md) — installing, updating, uninstalling, in English and German
- [Browser extension](docs/extension.md) — what it does, installing it in Chromium and Firefox
- [Android and iPhone](docs/mobile.md) — installing, what's different on a phone, building, the phone plugin
- [Passkeys for the system](docs/passkeys.md) — the vault's passkeys in browsers and apps on Linux, Windows, Android, iOS and macOS, threat model
- [UwULock Server's extras](docs/uwu-extras.md) — icons, versions, reminders, file requests, masked addresses, Sends, travel mode
- [Moving from Bitwarden](docs/moving-from-bitwarden.md) — a whole vault into UwULock, attachments and Sends included
- [Konzept](KONZEPT.md) — the concept, in German
- [Vision](docs/vision.md) — what I want UwULock to be and what it will never do
- [Architecture](docs/architecture.md) — how the pieces fit together, and the crypto
- [Design](docs/design.md) — colors, type and the font choice, Nyu, tone of voice
- [Roadmap](docs/roadmap.md) — my wish list, without dates
- [Security review of 0.5](docs/security-review-0.5.md) — what was found in the apps, the
  extension, the core and the passkey providers, and how it was fixed
- [Privacy policy](PRIVACY.md) — what the browser extension sends, and to whom

UwULock is not affiliated with Bitwarden Inc. It speaks the protocol their
open-source clients speak, and Vaultwarden implements.

## License

UwULock is free software under the [GNU GPL v3.0](LICENSE): use it, change it,
fork it, share it. If you pass on a changed version, its source has to stay
open too.
