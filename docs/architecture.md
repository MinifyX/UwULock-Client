# Architecture

UwULock is a Tauri 2 app like the rest of the UwUSuite: a React interface in
the system's web view, a Rust shell around it, and the real work in a crate
that is tested without a window.

```
┌──────────────────────────── apps/desktop ────────────────────────────┐
│  React (src/)                        Rust (src-tauri/)               │
│  LoginScreen · LockScreen            vault.rs    commands, state,    │
│  VaultScreen · ItemDetail   ──IPC──▶             auto-lock, sync     │
│  Generator · Settings                account.rs  what is on disk     │
│                                      clipboard.rs copies that clear  │
│  names, usernames, notes             updates.rs  signed updates      │
│  — secrets only on "show"                                            │
└──────────────────────────────────────┬───────────────────────────────┘
                                       │
                         crates/uwulock-bitwarden
                   api: prelogin · login · 2FA · sync · saving
                                       │ uses
                           crates/uwulock-core
              crypto · wire · vault · totp · generator (no network)

         crates/uwulock-bitwarden ── HTTPS (rustls) ──▶ Vaultwarden / Bitwarden
                                                       (identity + api)
```

The work is split in three crates:

- **`uwulock-core`** — Bitwarden's crypto and data formats, without a network:
  master key, encrypted values and files, Send keys, key pairs and passkey key
  sets, fingerprint phrases, the sync format (`wire`), the decrypted vault,
  TOTP, passkeys that sign in (`passkey`: Bitwarden's `fido2Credentials`, a
  WebAuthn authenticator's data and signatures), the password and passphrase
  generator, and the shared `Error`. No HTTP, no disk.
- **`uwulock-bitwarden`** — the HTTP side (`api`): prelogin, login, two-step
  login, token refresh, sync, saving. It re-exports `uwulock-core` under its
  old paths (`uwulock_bitwarden::crypto`, `::vault`, …), so the desktop app
  only depends on this one.
- **`uwulock-wasm`** — `uwulock-core` compiled to WebAssembly for the browser
  extension's background worker: unlocking (master password, PIN, the kept
  user key), the vault as the web vault shows it, autofill values, and
  passkeys made and used. Everything in and out as JSON text; the keys stay
  in the module's memory until it is locked.
  `apps/extension/scripts/build-wasm.mjs` builds it.

Why the split: the web vault of UwULock-Server will run `uwulock-core`
compiled to WebAssembly (`wasm32-unknown-unknown`) in the browser — the same
crypto the desktop app uses, checked against the same vectors from
Bitwarden's SDK, with no second implementation in TypeScript. CI runs
`cargo check -p uwulock-core --target wasm32-unknown-unknown` so nothing
networked or browser-unsafe creeps in. In a browser, randomness comes from
`crypto.getRandomValues` (getrandom's `js` feature), and `Totp::now` does not
exist there — the standard clock panics in a browser, so the web side passes
the time to `Totp::code_at` itself.

## The protocol

UwULock logs in as a Bitwarden desktop client: `client_id=desktop`, the
device type of the system (6 Windows, 7 macOS, 8 Linux), a device id of its
own, kept in the data folder across logouts.

1. **Prelogin** — `POST /identity/accounts/prelogin` (older Vaultwardens:
   `/api/accounts/prelogin`) says how to derive the master key: PBKDF2-SHA256
   with n iterations, or Argon2id with iterations, memory and parallelism.
   Settings below Bitwarden's own floors are refused: a server asking for 100
   rounds would make the hash cheap to crack. So are settings far above its
   maxima, which would keep the app busy for hours. An account this device
   already knows never logs in with weaker settings than its last login used;
   whoever lowered them on purpose logs the account out here and adds it again.
2. **Login** — `POST /identity/connect/token`, `grant_type=password`, with the
   master password hash. A 400 with `TwoFactorProviders2` asks for a second
   step; the same request goes again with `twoFactorToken` /
   `twoFactorProvider` / `twoFactorRemember`. Email codes are requested with
   `POST /api/two-factor/send-email-login`. Bitwarden's cloud may answer "new
   device verification" and email a code, sent back as `newDeviceOtp`.
3. **Sync** — `GET /api/sync?excludeDomains=true` with the access token:
   profile (keys), folders, collections, ciphers. Refresh with
   `grant_type=refresh_token` when the token is about to run out.

Bitwarden answers in camelCase, older Vaultwardens in PascalCase. `wire.rs`
lowers every key first and reads one shape. Passkeys are the exception: UwULock
never reads them, and hands them back to the server spelled as it sent them.

## The crypto

All RustCrypto, in `crates/uwulock-core/src/crypto.rs`:

| Step              | How                                                                                                     |
| ----------------- | ------------------------------------------------------------------------------------------------------- |
| Master key        | PBKDF2-SHA256(password, email) or Argon2id(password, SHA-256(email)), 32 bytes                          |
| Password hash     | base64(PBKDF2-SHA256(master key, password, 1 round)) — the only thing sent                              |
| Stretched key     | HKDF-Expand(master key, "enc"/"mac") → 32 + 32 bytes                                                    |
| User key          | `profile.key`: type 2 under the stretched key (type 0 under the bare key for accounts from before 2019) |
| Private key       | `profile.privateKey`: PKCS#8 DER, type 2 under the user key                                             |
| Organisation keys | type 4, RSA-2048-OAEP-SHA1 with the account's public key                                                |
| Item keys         | `cipher.key`, type 2 under the user or organisation key (newer items)                                   |
| Every field       | type 2: AES-256-CBC + HMAC-SHA256 over IV‖ciphertext, MAC checked first, constant time                  |
| Files             | attachments and file Sends: type 2 in binary, byte 2‖IV‖MAC‖ciphertext (`encrypt_file`, `decrypt_file`) |
| Send keys         | HKDF-SHA256(16-byte seed, salt "bitwarden-send", info "send") → 64 bytes (`send_key`)                   |
| Handing over      | the user key RSA-OAEP-SHA1-wrapped for a fresh or someone's RSA-2048 key (`wrap_for`, `PrivateKey`)     |
| Passkey unlock    | PRF output stretched like a master key; key set of user key, public and private key (`PrfKeySet`)       |
| Fingerprint       | SHA-256(public key), HKDF-Expand with the email or user id → five words of EFF's long list              |

`crates/uwulock-core/tests/integration/vectors.rs` checks the master key, hash, stretching, a legacy user
key, shareable and Send keys, a Send, two attachments and a fingerprint phrase against the known
answers in Bitwarden's SDK (`bitwarden/sdk-internal`). The SDK has none for a passkey's key set, so
that test opens one made with Python's `cryptography` the way Bitwarden's web vault makes it.
The desktop app doesn't use files, Sends, passkeys or handing over yet; the web vault of
UwULock-Server does.
`crates/uwulock-bitwarden/tests/integration/flow.rs` runs the whole way — prelogin, two-step login, remembered
device, refresh, revoked session, sync, every item type, organisations, item
keys — against a toy server that encrypts its vault the way Bitwarden's apps
do.

Keys and decrypted values are `Zeroizing` and wiped when dropped.

## What stays where

- **The page** gets item summaries (name, username or card ending, host,
  flags) and item details without secrets. A password, card number, security
  code, hidden field, SSN, private key or old password is fetched one at a
  time by `reveal_field` when the eye is clicked, and hidden again after a
  minute. Copying (`copy_field`) never goes through the page.
- **Rust, while unlocked**: the user key, the decrypted vault and the session.
  Locking — by hand, Ctrl+L or auto-lock — drops all of it and clears a
  copied secret from the clipboard. Quitting drops it with the process, but
  leaves a secret copied just before in the clipboard.
- **Disk** (`account.rs`): one folder per account under `accounts/<id>/`, with
  `account.json` — server, email, KDF settings, the user key as the server
  wraps it, and the refresh token and remember-device token sealed under the
  user key — and `vault.json`, the last sync as the server sent it. Beside
  them `accounts.json` (which accounts there are and which was open last,
  nothing secret) and `device-id`, shared by all of them. Nothing in there
  opens anything without the master password.

Auto-lock counts what the user does (keys, clicks, the wheel, reveal, copy),
not what the page polls: the one-time code refreshes every second and must
not keep the vault open by itself. It locks every account at once.

## Saving

Editing goes the same way round as reading. The editor sends back what was
typed; a value it never had — a password nobody revealed, a card number, a
hidden field — comes as `null`, and Rust takes the one the item already has.
So a name change never brings the password into the web view.

What UwULock doesn't show travels along: an item keeps its own key, its
passkeys, its linked fields, the checksum of an address it still has, and the
date it was archived. Bitwarden stores the login, card, identity, note and SSH
object as the client sends it — whatever a save leaves out is gone from the
item afterwards.

Three rules keep a save from costing anything:

- Every change names the revision UwULock last saw. A server with a newer copy
  refuses it (`Error::Conflict`), and the page says so instead of retrying.
- An item that didn't fully decrypt is never written back.
- An item that asks for the master password can't be changed without it
  either.

What comes back from the server goes straight into the cached sync
(`patch_cache`), so the list and the details are right without waiting for the
next sync.

## The browser extension

`apps/extension` is a Manifest V3 extension for Chromium and Firefox, one
source and two packages (`scripts/build.mjs`; the manifests differ only in the
background: a service worker plus an offscreen document for the clipboard in
Chromium, an event page in Firefox). It is a Bitwarden client of its own —
client `browser`, a device of the account — and needs neither the desktop app
nor UwULock Server's extras.

```
 popup · passkey window ──┐                 ┌── content script (every frame)
 (React, desktop styles)  │ runtime         │   forms, inline menu, save bar,
                          ▼ messages        ▼   filling — closed shadow DOM
            background (service worker / event page)
            session.ts  login, 2FA, unlock, PIN, lock, accounts
            vault.ts    sync, cache, saving   live.ts  notification hub
            autofill.ts which frame may have what, save prompts
            passkeys.ts WebAuthn provider      menus.ts context menu, shortcut
                          │
                 crates/uwulock-wasm (keys, crypto, vault)
                          │
            fetch / WebSocket ──▶ UwULock Server · Vaultwarden · Bitwarden

 page/webauthn.ts (MAIN world, https) ⇄ content/bridge.ts ⇄ background
```

- **Keys** live in the WebAssembly module's memory. A service worker is ended
  after half a minute without work, so while the vault is unlocked the user
  key is also in `storage.session` (memory only, closed to content scripts),
  and the next event opens the vault again from it and the cached sync
  (IndexedDB, still encrypted). `storage.local` has the accounts: server,
  address, KDF, the user key as the server wraps it, the tokens.
- **Content scripts** ask with `content:*` messages and learn names only. The
  values of one item go to a frame after somebody picked it, and only if the
  item matches that frame's own address (the sender's URL, never anything the
  message says). A pick in the popup, the context menu or the shortcut is an
  offer to every frame of the tab; each frame claims it and is judged on its
  own address.
- **Passkeys**: a script in the page's own world replaces
  `navigator.credentials.create/get`, hands the options to the bridge, and
  the background checks the relying party against the frame's origin, asks in
  a window of its own, and signs with the passkey from the vault
  (`uwulock-core::passkey`). Anything it doesn't answer goes to the browser.
- **UwULock Server's extras** are looked up at login (`GET /uwu/v1/info`,
  kept per account as `uwu.features`): a feature appears when the server lists
  it, so Vaultwarden and Bitwarden see a plain Bitwarden client.

## UwULock Server

Against Bitwarden and Vaultwarden UwULock is a plain Bitwarden client. A
UwULock Server says what more it can do at `GET /uwu/v1/info`; the app asks at
every sync and offers a feature only when the server lists it. The contract
for all of it is UwULock-Server's `docs/uwu-api.md`.

- **Delta sync** (`uwulock-bitwarden::delta`, `uwu::uwu_sync`): the first
  sync is complete, every later one brings only what changed since the
  cursor. The pages are merged into a copy in exactly `/api/sync`'s shape, so
  the vault opens from it as before. The cursor and UwULock's own state (own
  icons, reminders, masked addresses, travel mode, badge counts) are kept in
  the same `vault.json` under `uwuLock` — the state sealed under the user
  key — so a crash can never leave a cursor that is ahead of the copy. A
  cursor the server can't read, or a `reset`, means one full sync.
- **Live updates** (`uwulock-bitwarden::live`, desktop `live.rs`): UwULock
  Server's realtime channel (`/uwu/v1/realtime`: the token in the first
  message, never in the URL; a fresh token on the same connection before the
  old one runs out; the cursor, so a reconnect hears what it missed), else
  Bitwarden's SignalR hub in MessagePack. Either says only _that_ something
  changed; a sync follows 250 ms after the last change in a row. Reconnects
  back off by the contract's close codes. While a channel is up, the
  five-minute check rests. A session the server ends locks the account.
- **The extras key** (`uwulock-core::extras`): what UwULock encrypts beyond
  Bitwarden's objects (suite spaces, own icons, file-request labels) is under
  one key per account, wrapped for the user key and under a key derived
  (HKDF) from the account's RSA private key — both secrets the server never
  has, so it can't hand out a key of its own. After an official client
  rotated the user key only the second wrap is left; the next UwULock client
  opens it with the private key and wraps it for the new user key
  (`extras::resolve`, `Client::extras_key`). Where both are there, they must
  hold the same key; an RSA wrap for the public key (0.3's betas) is never
  taken. The desktop app remembers the key's id and warns when it changes.
- **File requests** are checked on the owner's side too: details
  (`publicInfo`) that name another public key than the account's own get no
  link.
- **File requests** (`uwulock-core::file_request`): the link's secret and
  its HKDF key, the public details the uploader's page encrypts for, a key
  per submission wrapped RSA-OAEP-SHA1 for the owner, a key per file — so
  taking a file into an item re-wraps only its key.
- **The password check** (`uwulock-core::health`): the report the web vault
  keeps (the same JSON), HIBP's SHA-1 and XposedOrNot's Keccak-512 prefixes,
  breached sites and 2FA Directory matched by domain, the ignore list (§15.6,
  unknown keys kept) and the review's cards, worst first.
  `uwulock-bitwarden::health` asks the server (a few prefixes at a time);
  `apps/desktop/src-tauri/src/health.rs` puts both together and keeps the
  lists for the unlock. The page only gets findings, never a password.
- **The suite vault** (`uwulock-bitwarden::suite`): UwUSSH and UwURDP log in
  as `App::suite("uwussh")` and get their space's key (under the extras key)
  and pull and push their sealed records. The records' crypto and merge stay
  in the apps.

The crypto of all of this is in `uwulock-core`, with values made
independently in Python as test vectors (`tests/integration/uwu.rs`), and is
what UwULock-Server's web vault builds on as well.

## Locking with the computer, Windows Hello

`session_lock.rs` locks everything when the screen locks or the computer
sleeps: logind (`PrepareForSleep`, `Lock`, `LockedHint`) and the screen
saver's `ActiveChanged` on Linux, the input desktop on Windows, the session's
`CGSSessionScreenIsLocked` on macOS, and everywhere a gap between the wall
clock and the monotonic one. `hello.rs` unlocks with Windows Hello: its key
(KeyCredentialManager) signs a challenge of the account id and 32 random
bytes, new at each switching on and kept under DPAPI next to the copy, and the
hashed, stretched signature seals a copy of the user key in `account.json`.
Switching Windows Hello off and on again makes a new key pair and challenge, so
a signature obtained earlier opens nothing any more. Touch ID
is not offered: a keychain item that only opens with a finger needs an Apple
Developer ID signature and an entitlement, and a prompt in front of a key
kept elsewhere would protect nothing.

## Phones

The same app builds for Android and iOS (`cfg(mobile)` in
`apps/desktop/src-tauri`); updater, tray and the computer's lock signals are
desktop only. `phone.rs` locks after a minute in the background (Tauri's
`Suspended`/`Resumed`). What needs the phone's own APIs goes through the local
plugin `crates/tauri-plugin-uwulock-mobile` (Kotlin and Swift): biometric
unlock, the clipboard, the system bars, saving into Downloads. Biometric unlock
reuses Windows Hello's place in `account.json` with the prefix `m1:`: 32 random
bytes behind the biometric check (an Android Keystore AES key that needs a
strong biometric and dies with a new enrolment; an iOS Keychain item with
`biometryCurrentSet`) are stretched into the key that seals the user key. Plugin
calls block until the phone answers and so never run on the main thread.
Details: [mobile.md](mobile.md).

## Passkeys for the system

`passkeys/` in the app offers the vault's passkeys outside the browser
extension: a virtual FIDO2 security key on Linux (made through `/dev/uhid` by
the root helper `crates/uwulock-uhid-broker`, which makes nothing else), a
plugin passkey manager on Windows 11 (only requests Windows signed),
Credential Manager's provider on Android, an AutoFill extension on iOS and
macOS. The protocol parts (CTAP2, CTAPHID, uhid, the broker's frames, rpId
rules, Windows' request signatures, WebAuthn JSON, the Apple extension's
sealed list) are in `crates/uwulock-authenticator`. Design and threat model:
[passkeys.md](passkeys.md).

## Suite parts

Taken from UwURDP unchanged or nearly: the installer (`apps/setup`), the
updater (`updates.rs`, signed with a key of UwULock's own, feeds on the
`updates` branch), the release scripts, the icon pipeline, tokens, Nyu, the
settings and i18n machinery (German source strings, English catalogue,
`scripts/check-i18n.mjs`).
