<p align="center">
  <img src="brand/uwulock-app-icon.svg" width="112" alt="UwULock logo" />
</p>

<h1 align="center">UwULock</h1>

<p align="center">
  The password manager I build for myself, because every other one annoyed me. (◕‿◕✿)<br/>
  Vaultwarden · Bitwarden · TOTP · Windows, macOS and Linux, first beta
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
- **Playful.** Nyu, the cat, is a padlock now. Security warnings are never
  playful.

> **Status: beta.** 0.2.0-beta.2 reads and writes: log in (with two-step
> login), browse, search, copy, one-time codes — and create, edit and delete,
> with a private Vaultwarden and one at work side by side.
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
> - Create, edit and delete items, folders and favourites; the trash and back
>   out of it. A password you don't look at never passes through the window,
>   and what UwULock doesn't show — passkeys, linked fields — is handed back
>   to the server untouched.
> - Several accounts on one device: a private Vaultwarden and one at work,
>   each with its own vault, its own session and its own master password.
> - One-time codes (TOTP, also Steam) counting down right in the item.
> - Search, keyboard shortcuts like Bitwarden's (Ctrl+U, Ctrl+P, Ctrl+T), a
>   password generator, also right inside the password field.
> - Auto-lock, clipboard clearing, sync on unlock and every five minutes.
> - Its own installer with Nyu, signed automatic updates, German and English.
>
> **What doesn't, yet.** Attachments, sends, passkeys, moving items into an
> organisation, Duo and FIDO2 as second step, SSO, unlocking with Windows
> Hello. The [roadmap](docs/roadmap.md) has the order.

## Install

Windows 10 or 11 (x64 and ARM), macOS 11 or newer, Linux (x86_64 and arm64).
Download the file for your system from the newest
[release](https://github.com/MinifyX/UwULock-Client/releases), run it, click
**Install** — no admin prompt. The [install guide](docs/install.md) has the
details. [Auf Deutsch](docs/install.md#uwulock-installieren).

## Project layout

| Path                       | What lives there                                                         |
| -------------------------- | ------------------------------------------------------------------------ |
| `apps/desktop`             | The Tauri 2 app (React UI + Rust shell)                                  |
| `apps/desktop/e2e`         | End-to-end run of the real app against a toy Vaultwarden                 |
| `apps/setup`               | The installer, updater and uninstaller, for all three systems            |
| `crates/uwulock-bitwarden` | Bitwarden's protocol and crypto: login, two-step login, sync, decrypting |
| `brand/`                   | Nyu as a padlock: the UwULock icon, symbol, mono symbol                  |
| `docs/`                    | Vision, architecture, design, roadmap, install guide                     |
| `release-notes/`           | What's new, per version                                                  |
| `scripts/`                 | Icons, building the setup, releasing                                     |

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
node apps/desktop/e2e/run.mjs     # end to end, Windows
```

The installer, with the app packed inside: `pnpm build:setup`. Releasing is
`pnpm release`; [release-notes/README.md](release-notes/README.md) has the steps.

## Documentation

- [Install guide](docs/install.md) — installing, updating, uninstalling, in English and German
- [Konzept](KONZEPT.md) — the concept, in German
- [Vision](docs/vision.md) — what I want UwULock to be and what it will never do
- [Architecture](docs/architecture.md) — how the pieces fit together, and the crypto
- [Design](docs/design.md) — colors, type, Nyu, tone of voice
- [Roadmap](docs/roadmap.md) — my wish list, without dates

UwULock is not affiliated with Bitwarden Inc. It speaks the protocol their
open-source clients speak, and Vaultwarden implements.

## License

UwULock is free software under the [GNU GPL v3.0](LICENSE): use it, change it,
fork it, share it. If you pass on a changed version, its source has to stay
open too.
