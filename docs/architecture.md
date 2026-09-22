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
              crypto · api · wire · vault · totp · generator
                                       │ HTTPS (rustls)
                        Vaultwarden / Bitwarden (identity + api)
```

## The protocol

UwULock logs in as a Bitwarden desktop client: `client_id=desktop`, the
device type of the system (6 Windows, 7 macOS, 8 Linux), a device id of its
own, kept in the data folder across logouts.

1. **Prelogin** — `POST /identity/accounts/prelogin` (older Vaultwardens:
   `/api/accounts/prelogin`) says how to derive the master key: PBKDF2-SHA256
   with n iterations, or Argon2id with iterations, memory and parallelism.
   Settings below Bitwarden's own floors are refused: a server asking for 100
   rounds would make the hash cheap to crack.
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
lowers every key first and reads one shape.

## The crypto

All RustCrypto, in `crypto.rs`:

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

`tests/vectors.rs` checks the master key, hash, stretching and a legacy user
key against the known answers in Bitwarden's SDK (`bitwarden/sdk-internal`).
`tests/flow.rs` runs the whole way — prelogin, two-step login, remembered
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
  Locking — by hand, Ctrl+L, auto-lock or quitting — drops all of it and
  clears a copied secret from the clipboard.
- **Disk** (`account.rs`): `account.json` with server, email, KDF settings,
  the user key as the server wraps it, and the refresh token and
  remember-device token sealed under the user key; `vault.json`, the last
  sync as the server sent it; `device-id`. Nothing in there opens anything
  without the master password.

Auto-lock counts what the user does (keys, clicks, the wheel, reveal, copy),
not what the page polls: the one-time code refreshes every second and must
not keep the vault open by itself.

## Suite parts

Taken from UwURDP unchanged or nearly: the installer (`apps/setup`), the
updater (`updates.rs`, signed with a key of UwULock's own, feeds on the
`updates` branch), the release scripts, the icon pipeline, tokens, Nyu, the
settings and i18n machinery (German source strings, English catalogue,
`scripts/check-i18n.mjs`).
