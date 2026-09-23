# Security review, September 2026

The first security review of UwULock, done on 0.2.0-beta.1 (commit `1ddcf1c`)
on 23 September 2026, across the whole tree: the protocol and crypto crate
(`crates/uwulock-bitwarden`), the desktop app (the Tauri shell and the page),
the setup, the scripts and CI. Everything below was established by reading the
code first; the fixes were then made with a test each where a test could hold
them.

## The trust boundary

UwULock opens a Bitwarden or Vaultwarden vault, so the interesting attacker is
whoever controls what comes back from the server: the server itself, a
reverse proxy in front of it, or another member of a shared organisation who
can edit items the victim sees. Everything the server sends is hostile input
until it has been decrypted and checked.

The page inside the window is **outside** the boundary around secrets: the
design keeps keys and decrypted secrets in Rust and hands the page one value
at a time. So a finding that needs script in the page is still a finding, and
is rated by what the page could do that it otherwise could not.

Severity: **High** is a secret leaving the device or a vault lost to a normal
user. **Medium** needs a hostile server, a hostile org member or a compromised
page, but is real. **Low** is defence in depth, resource exhaustion, or a
policy only the client enforces. **Info** is worth knowing and nothing to do.

## Fixed

| Severity | Where               | What                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| -------- | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Medium   | TOTP, `totp.rs`     | **M1.** The authenticator key was cut at the length of `steam://` or `otpauth://` without checking that the cut fell between two characters. A key with a multi-byte character across that byte panicked, and `panic = "abort"` took the app with it. An org member could put such a key in a shared item that sorts first: every member who unlocked crashed, again after every restart. The prefix is now compared without slicing, and a test runs a multi-byte character through every offset of both prefixes. (`1f71079`)                                                                                                                                                                                                                                                                                                                                                                                                                |
| Medium   | `logout` command    | **M2.** `logout` took the account id from the page and removed `accounts/<id>` recursively without checking it was an account. `..` removed the whole data folder, an absolute path any folder the user can write. The command now refuses an id that isn't one of the accounts on this device, and the storage refuses any id that isn't the hyphenated UUID it makes, for reading as well as for writing and removing. Tests hand both of them `""`, `..`, an absolute path and an unknown id. (`076ccd5`)                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| Medium   | Passkeys, `wire.rs` | **M3.** Every key of a sync was lowered before reading, including those inside the passkeys, which are kept as they are and sent back on a save. Vaultwarden stores the login object as it gets it, so the first edit of a login with a passkey left it with `credentialid`, `rpid` and so on for every client — the ones that read camelCase strictly would lose the passkey. Passkeys now keep the server's spelling from the sync to the save, unknown fields included; one an earlier UwULock already stored lowered gets Bitwarden's names back on the next save. A test saves against the toy server, which stores the login as Vaultwarden does, and checks every key name and value of the passkey came back unchanged. (`42c17ad`)                                                                                                                                                                                                    |
| Medium   | KDF, `crypto.rs`    | **L1, raised to Medium.** Logging in again to an account already on this device trusted a fresh prelogin. A hostile server could answer PBKDF2 with 5 000 rounds for an account that has 600 000 and get a hash 120 times cheaper to guess the master password from; nor was there any ceiling, so it could also keep the app deriving for hours. A known account now refuses a weaker KDF than the one stored from its last login, before anything is derived or sent; whoever lowered it on purpose logs the account out and adds it again. A first login still takes Bitwarden's old defaults. New ceilings: PBKDF2 10 000 000 rounds, Argon2id 20 passes and 16 lanes (1 GiB was already the memory ceiling). Bitwarden's server allows 2 000 000 and 10; Vaultwarden caps only the lanes, so the rounds and passes get room above Bitwarden's maxima. The ceilings apply to what a server sends, not to a KDF already stored. (`83d5856`) |
| Medium   | Updater on Linux    | **Found in UwUSSH, same code here.** The updater started the setup AppImage with `APPIMAGE_EXTRACT_AND_RUN=1`. The AppImage runtime then unpacks into `$TMPDIR/appimage_extracted_<checksum>` — in the shared `/tmp`, a name anyone can work out — and runs what it finds there; another local user could have created it first with their own `AppRun`. The updater now makes a new folder only this user can open (0700) next to the downloaded setup and points the setup's `TMPDIR` at it. The setup gives what it starts afterwards the `TMPDIR` from before (or none, if there was none), and the folder goes with the updates folder on the next start. (`9be6dfa`)                                                                                                                                                                                                                                                                     |

## Not fixed: Low and Info

These are listed for the record and left as they are for now.

- **L2 Low — a sync can replace the stored protected user key with anything.**
  A different `profile.key` is written to `account.json` without checking its
  type, so a hostile server can lock the offline vault with a misleading
  "wrong password", or swap in a type-0 value that unlock decrypts without a
  MAC.
- **L3 Low — the HTTP client follows redirects and reads bodies without a
  limit.** A 307/308 re-sends the login form (hash, email, two-step code) to
  wherever it points, as long as that is https; and a hostile server can make
  the client allocate without bound.
- **L4 Low — auto-lock doesn't count suspend.** Idle time is a monotonic clock
  that stops while a Linux or macOS machine sleeps, and there is no lock on
  sleep or on the session lock.
- **L5 Low — quitting doesn't clear the clipboard.** A secret copied less than
  the clear delay before quitting stays. `docs/architecture.md` said otherwise
  and now says what happens.
- **L6 Low — local files.** On Linux the data folder and its files get the
  umask's permissions, and every writer of `vault.json` shares one fixed temp
  name, so two writers at once can leave a broken cache.
- **L7 Low — organisation permissions aren't enforced.** "Can view, except
  passwords" (`viewPassword: false`) and disabled organisations are ignored.
  Bitwarden documents this restriction as client-side only.
- **L8 Low — zeroisation gaps.** Folder, collection and organisation names,
  the editor's drafts, the raw token response and the email-code body are
  plain strings that are freed without being wiped.
- **Low — the setup's WebView folder on Linux is in the shared `/tmp`.** It is
  `std::env::temp_dir()/UwULock-Setup-WebView`, a fixed name another user can
  create first. A setup started by the updater now has a private `TMPDIR`, so
  only a setup started by hand is affected.
- **I1 Info — `rsa` 0.9: RUSTSEC-2023-0071 (Marvin).** No fixed version exists.
  Exploiting it would need a server to time many decryptions of crafted org
  keys, and nothing observable follows one right away.
- **I2 Info — one device id for every account and server.** Intended, so no
  server meets a "new device" every time; it lets their operators correlate the
  installation if they compare notes.

## Cleanups

Done alongside, none of them changes behaviour:

- The setup's comments, test fixtures and uninstall were still partly
  UwURDP's: "every open remote session", "hosts, vault and settings", a
  `uwulock.db`. The uninstall on macOS and Linux also cleared a Keychain or
  Secret Service item, `device-seal-key`, that UwULock never creates; it
  doesn't any more.
- Fields of the wire format nothing read are gone (the KDF in the token
  response, `TokenError.error`, `Profile.id`, `Organization.enabled`); serde
  ignores what isn't named.
- An unused workspace dependency (`anyhow`) is gone, and the comment on
  `Device.id` says it is one id for every account.

## Checked and fine

The point of saying so: these were looked at and held.

**Field crypto.** Type 2 values have their IV and MAC lengths checked when
parsed, the MAC is compared in constant time before AES runs, and type 0 is
accepted in exactly one place, the user key of a pre-2019 account, as
Bitwarden does. RSA and AES types can't be swapped for each other. IVs are
fresh from the OS for every encryption.

**The master password.** It leaves the process only as the hash, the login
form is wiped after sending, and unlock and the re-prompt run offline against
the stored key. The KDF, hash and stretch match Bitwarden's.

**TLS.** rustls with the bundled and the system's roots, no custom verifier,
and https for every server except one on loopback.

**Saving.** An item that didn't open is never written back, an item keeps its
key and its organisation, so nothing is re-encrypted under another outer key,
and every save names the revision it last saw. The re-prompt is enforced in
Rust for every action that needs it. Nothing decrypted is ever sent back to the
server as plaintext.

**The page.** No `innerHTML`, `eval` or `href` sink; everything from the server
is rendered as text. The content security policy is strict, prototypes are
frozen, and the page gets `core:default` plus the window controls. Only http
and https addresses open in the browser. Secrets reach the page one at a time
and there is no bulk export.

**The updater and the setup.** The minisign signature is checked against the
pinned key, the signed name binds the version, and the setup refuses to go
backwards. On Windows the file stays locked from the check until it runs; a
Linux package installs from a private copy. The setup was already hardened
(System32-only DLL search, symlink-safe removal).

**CI.** Every action is pinned to a commit, the token is read-only and not
persisted, and no signing key lives in CI.
