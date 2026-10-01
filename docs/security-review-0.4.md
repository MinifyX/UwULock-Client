# Security review, 0.4 (October 2026)

The third security review of UwULock, done before 0.4.0-beta.1 on 1 October 2026. It covers
everything new since 0.3.0-beta.2 (`v0.3.0-beta.2..main` at `2e7e5bb`): the Android and iOS apps
(PR #14, the `tauri-plugin-uwulock-mobile` plugin, the generated Android project, the mobile
workflows and the release script), Wi-Fi networks as an item type (PR #15) and joining them from
the phone (PR #16), and the password check with its review one login at a time (PR #17) in
`uwulock-core`, `uwulock-bitwarden`, the app and the extension. The server half of the password
check (the proxies, the lists, the address check) is reviewed with UwULock Server 0.7, in that
repository.

Everything below was established by reading the code first. The fixes of the earlier reviews
([0.3](security-review-0.3.md), [the first one](security-review-2026-09.md)) were checked again
where the new code touches them; they hold.

## The trust boundary

As before: the interesting attacker is whoever controls what comes back from the server — the
server itself, a proxy in front of it, or another member of a shared organisation — and, for the
extension, the web page it runs in. The phones add two more:

- **another app on the same phone**, which can register for the same intents as the system, draw
  over UwULock, or read what UwULock leaves on the clipboard;
- **whoever holds the phone, a backup of it or a look at its app switcher**, without the master
  password or the enrolled finger or face.

Severity as in 0.3: **High** is a secret leaving the device, or the server able to read or
silently swap a secret. **Medium** needs a hostile server, a hostile org member, a malicious page
or a malicious app on the phone, but is real. **Low** is defence in depth, or a policy only the
client enforces. **Info** is worth knowing and nothing to do. The ids continue those of 0.3.

There was no Critical or High finding.

## Fixed

| Id    | Severity | Where                                                                                                    | What                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | Status |
| ----- | -------- | -------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| CL-M3 | Medium   | Password check, "Open the page & change the password" (`uwulock-bitwarden` `health.rs`, app `health.rs`) | **The server chose where "change the password" went.** The app opened the `url` of `GET /uwu/v1/change-password/{host}` as long as it began with `https://`. A hostile server (or a proxy) could answer with any page — a copy of the bank's login under its own name — and the review card, which says "open the page & change the password", sent the person there to type the old password and a new one. The API only ever answers `https://{host}/.well-known/change-password` or `null`. Now the server's answer only says _whether_ the page exists: the address is built in `uwulock-bitwarden` from the login's own host (`change_password_url`, plain host names only — no path, user, port or IP literal in front of the path; anything else isn't even asked about). A test answers with a page on another host and gets the login's own well-known address back.      | Fixed  |
| CL-M4 | Medium   | Android, Wi-Fi _Connect_ (`UwuLockMobilePlugin.kt`)                                                      | **The Wi-Fi password went out in an implicit intent.** Android 11+ adds the network through `Settings.ACTION_WIFI_ADD_NETWORKS`, with the network — password included — in the intent's extras. The intent named only the action, so any installed app could register for it: Android then shows a chooser, and an app called "Wi-Fi" picked there (or set as the default) received the password and the Enterprise identity. Now the plugin looks for the activity among system apps only (`MATCH_SYSTEM_ONLY`, `FLAG_SYSTEM`; the plugin's manifest declares the `<queries>` entry Android 11+ needs to see it) and addresses that component explicitly; without one it falls back to a network suggestion, which goes to the system service directly. `apps/desktop/test/mobile-hardening.test.ts` checks the explicit component and the order (component before the password). | Fixed  |

## Low and Info

Every Low below is fixed in this branch; the Info notes describe design decisions or platform
limits and stay as they are.

Phones:

- **CL-L14 Low, the iPhone's app switcher shows the open vault.** Android has `FLAG_SECURE`; iOS
  took its usual picture of the last screen — an item with its password shown, say — for the app
  switcher and kept it on disk. _Fixed:_ the plugin covers every window with a plain view of the
  page's background on `willResignActive` and takes it away on `didBecomeActive`, so the picture
  is empty.
- **CL-L15 Low, the iPhone's data folder went into backups.** Tauri's app data folder
  (`Library/Application Support/app.uwulock`) is backed up to iCloud and to computers by default:
  `account.json` with the refresh token and the user key sealed under the master password, the
  synced vault, and the Face ID copy of the user key. Nothing in it opens without the master
  password (the Face ID copy needs a Keychain item that is `ThisDeviceOnly`), but Android leaves
  everything out of its backups and the iPhone should too. _Fixed:_ the plugin marks the folder
  `isExcludedFromBackup` when it loads.
- **CL-L16 Low, the auto-lock time stood still while the phone slept.** Idle time came from
  `Instant`, which on Android (`CLOCK_MONOTONIC`) and iOS doesn't count time asleep. With "lock in
  the background" switched off, a phone with a 15-minute auto-lock that slept for hours came back
  with the vault open, and even then the timer only looked every ten seconds. _Fixed:_ idle time is
  the longer of the monotonic and the wall clock (a wall clock set back counts as zero), and the
  app checks the moment it comes back to the screen. This also covers a laptop that sleeps without
  "lock with the computer". A unit test covers the clocks.
- **CL-L19 Low, the phone's page could use the title bar's window controls.** The one capability
  granted `start-dragging`, `minimize`, `toggle-maximize`, `close` and `destroy` everywhere. _Fixed:_
  `capabilities/default.json` has `core:default` only; the window controls are in `desktop.json`,
  limited to Linux, macOS and Windows.

Password check:

- **CL-L17 Low, the address check didn't check the consent itself.** `health_check_emails` decrypts
  every login's username and sends the ones that are addresses to the server, which passes them to
  XposedOrNot. Only the page decided whether the switch was on and the account had agreed. _Fixed:_
  the command refuses unless the server offers the check (`breaches.emailCheck`) and the server
  says this account opted in (`GET /uwu/v1/breaches/emails/opt-in`), before anything is decrypted
  or sent.
- **CL-L18 Low, the review saved a new password past the re-prompt.** "Generate & save a new
  password" changed a login with "ask for the master password" without asking. Nothing secret was
  shown (the old password went into the history), but every other change honours the re-prompt.
  _Fixed:_ the save goes through `vault::with_item`, which asks first, as _Connect_ and revealing
  do.

Info:

- **CL-I8 Info, the unlock secret crosses the plugin bridge as a string.** The 32 bytes that open
  the phone's copy of the user key come back from Kotlin and Swift as base64 in JSON. Rust keeps
  them in `Zeroizing`, but the Kotlin and Swift strings and Tauri's IPC buffers can't be wiped.
  Windows Hello's signature takes the same kind of path; only memory access reaches them.
- **CL-I9 Info, Android clears the clipboard only while UwULock runs.** The clear after the set
  time is a thread in UwULock's process. If Android stops the process in the background before
  then, the copy stays until something else is copied (Android 13+ clears the clipboard itself
  after an hour, and the copy is marked sensitive). iOS lets the copy expire by itself.
- **CL-I10 Info, XposedOrNot's prefix is longer than HIBP's.** XposedOrNot's anonymous API takes
  the first 10 hex digits of Keccak-512 (40 bits) where HIBP takes 5 of SHA-1 (20 bits), so the set
  of passwords sharing a prefix is much smaller. That is the source's own design; the server asks
  for every user alike and keeps no relation to them, and the admin can switch the source off.
- **CL-I11 Info, "change the password" may open a login's own `http://` address.** Without a
  change-password page the app opens the login's first URI, as its own "open" button does. Only
  `http` and `https` are opened (never `javascript:`, `file:`, `intent:` …), in the system browser;
  an `http` login is already on the card as "no https".
- **CL-I12 Info, the review opens a re-prompted login's site without asking.** The site's host is
  on the card already; the URI itself isn't shown. Opening it doesn't reveal more than the report.
- **CL-I13 Info, no overlay filter on Android 10 and 11.** UwULock doesn't set
  `filterTouchesWhenObscured`: it would break screen filters and accessibility overlays, Android
  12+ blocks touches through untrusted overlays itself, and the biometric prompt is the system's.
- **CL-I14 Info, the Android unlock key doesn't ask for StrongBox.** It is in the Keystore's TEE
  where the phone has one. StrongBox isn't on every phone, and a key that needs it would fail
  there.

## Checked and fine

- **Android unlock key**: AES-256-GCM in the Android Keystore, `setUserAuthenticationRequired`,
  per-use authentication with `AUTH_BIOMETRIC_STRONG` on Android 11+ (Android 10: per-use keys only
  take a biometric), `setInvalidatedByBiometricEnrollment(true)`, bound to `BiometricPrompt` through
  a `CryptoObject`, `BIOMETRIC_STRONG` only, no device credential. A key invalidated by a new
  enrolment is caught (`KeyPermanentlyInvalidatedException`) and the copy dropped.
- **iOS Keychain**: `kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly` with `.biometryCurrentSet`,
  `kSecAttrSynchronizable: false`; no passcode fallback.
- **Locking in the background** (one minute, both clocks), `FLAG_SECURE`, Android backups and
  device transfer off (`allowBackup="false"`, `fullBackupContent="false"`, every domain excluded),
  cleartext traffic off in release builds, only the launcher activity exported, no deep links,
  providers, services or receivers.
- **The phone plugin is unreachable from the page.** It registers no commands (`COMMANDS = []`)
  and no capability names it; Tauri 2.11 refuses every `plugin:uwulock-mobile|…` call without a
  permission before it reaches the native side (`webview/mod.rs`, the ACL check runs before the
  mobile forwarding). WebView debugging is off in release builds (no `devtools` feature); the CSP
  is unchanged (`script-src 'self'`, no frames, `connect-src` IPC only).
- **Clipboard**: Android marks copies sensitive (`EXTRA_IS_SENSITIVE`) and clears only its own
  copy; iOS `localOnly` with an expiry, cleared by change count without reading the pasteboard.
- **Wi-Fi**: _Connect_ takes the item's id; the network, password included, is read in Rust
  (`wifi::network`, with the re-prompt) and handed to Kotlin, never through the page. Values are
  checked before (WPA passwords 8–63 ASCII, EAP method and phase 2 from fixed lists, the CA domain
  as a host name) and Android's builders check again. PEAP and TTLS always verify the RADIUS server
  by domain against the phone's system CAs only, never user-added ones. The QR code is drawn
  locally (uqr's SVG contains only rectangles and paths, no text), with `\ ; , : "` escaped. The
  extension shows Wi-Fi networks and copies their values but lists and fills them nowhere: they are
  notes to everything that fills (`apps/extension/test/wifi.test.ts`).
- **Password check**: only hash prefixes leave the device (SHA-1, 5 hex; Keccak-512, 10 hex), and
  only to the user's server; the matching happens locally. Lists from the server (breached sites,
  2FA Directory, sources) are rendered as text by React — no link, image or HTML from them. The
  report and the ignore list are sealed under the extras key (EncString type 2); a list that
  doesn't open starts over instead of being trusted; a forged list can only hide or show problems
  of the person's own logins. The address check runs only after the admin switch and the account's
  consent (CL-L17); the consent text says that the server sends the addresses to XposedOrNot in
  plain text.
- **Signing in CI**: the APK is built without any key; a separate job on a fresh runner signs it
  with Android's own tools, the release key only for the push of a `v*` tag, the CI key (or a
  throwaway one for forks) otherwise. The certificate is pinned both ways — a tag's APK must carry
  the release certificate, no other build may — and `scripts/release.mjs` checks it again (v2/v3
  signing block) before publishing. The key file is removed by a trap; `--ks-pass env:` keeps the
  password out of the command line and the log. Checkouts don't persist credentials, every action
  is pinned to a commit, the Gradle wrapper is validated. The IPA is unsigned by design; nothing in
  the iOS workflow holds a secret.

## 0.4.0-beta.2: SSH/RDP entries

The sections "SSH (UwUSSH)" and "Remote Desktop (UwURDP)" (`uwulock_core::suite`,
`suite::openssh`, app `suite.rs` + `suite/plan.rs`, `SuitePane`/`SuiteEditors`). The server is
untrusted as before: it holds the records sealed under each space's key (XChaCha20-Poly1305, AAD
of `docs/uwu-api.md` §6.2: prefix, id, kind, space id, clock, `deleted`), and the space key
under the extras key. `seq`, the cursor and which records it hands out are its own.

### Fixed

- **CL-M5 Medium, edits were sealed on top of records that weren't authenticated.** The app
  opened only the JSON kinds it shows; secrets and tombstones were taken as the server sent them.
  An edit or a delete starts from the record's clock (`Hlc::after`), so a server could hand out a
  secret with `updatedAt` near `u64::MAX` — or a host that didn't open, which could still be
  deleted — and the app sealed, with the real key, a record whose clock was the server's choice;
  every device that merges it carries that clock on. The same went for the secrets a deleted
  identity or key takes along. _Fixed:_ every record of a known kind is authenticated when it
  arrives (secrets and tombstones are opened and the plaintext dropped); one that doesn't open is
  `broken`, and nothing is sealed on a broken record — editing or deleting it is refused, a
  delete's cascade leaves it alone, and the sealing step refuses once more as a safety net.
  Tests: `nothing_is_sealed_on_a_record_the_server_changed`.
- **CL-L20 Low, a saved private key was readable by other users.** _Save private key_ wrote to
  Downloads with the default mode (`0644` under the usual umask); an unencrypted key in a home
  folder that others can enter. _Fixed:_ on Linux and macOS the file is created `0600`
  (`save_download_private`, `create_new`), which is also what `ssh` insists on.
- **CL-L21 Low, the copied ssh command could carry an option or a second line.** `shellQuote`
  stops the shell, not ssh: an address like `-oProxyCommand=…` (with no username) became
  `ssh '-oProxyCommand=…'`, which runs a command when pasted; a line break in the address or user
  (also U+0085, U+2028, U+2029) split the pasted text. _Fixed:_ control characters and Unicode
  line breaks are removed (`oneLine`, also for the copied RDP address), and a target starting
  with `-` gets `--` in front (`apps/desktop/test/suite.test.ts`). Same as the web vault's WV-5.
- **CL-L22 Low, a key's file name came from its label.** `safe_file_name` keeps the extension, so
  a key labelled `deploy.bat` was saved as `deploy.bat`. _Fixed:_ a name that
  `runs_when_opened` has its dots replaced (`deploy_bat`, `deploy_bat.pub`).
- **CL-L23 Low, a pull without end.** The pull loop asked for the next page as long as the server
  said `hasMore` with records, also when the cursor didn't move on. _Fixed:_ it stops when the
  cursor doesn't grow.
- **CL-L24 Low, one record under two ids.** The AAD holds the id's 16 bytes, so the same record
  opened under `ABCD…`, `{abcd…}` or `urn:uuid:…` too, and the app's map (keyed by the text)
  listed it twice. _Fixed:_ an id that isn't in the usual lowercase hyphenated form is ignored.

- **CL-L27 Low, `.rdp` values kept other line breaks.** Only CR and LF were replaced; a reader
  that also splits on U+2028/U+2029 or U+0085 saw a line of the attacker's choosing. _Fixed:_
  every control character and Unicode line break becomes a space (web vault: WV-8).
- **CL-L28 Low, labels looked up by the record's own text.** `workspaceLabel`, `authLabel` and
  the kind chip read a plain object by a value from the record, so `auth_type: "constructor"`
  rendered a function. _Fixed:_ `Object.hasOwn` first. Secret rows (password, private key,
  passphrase) appear only when the pointer leads to a record of kind `secret`; Rust checked the
  kind already (web vault: WV-11). The cascade of a deleted identity or key takes only records of
  kind `secret` along (WV-6: already so here).

### Open Lows and Info

- **CL-L25 Low, bcrypt rounds of an imported key are the file's.** `openssh::passphrase_opens`
  decrypts with the rounds the key text names (a `u32`); a crafted key with millions of rounds
  keeps a core busy (in the web vault: the tab). Only for text the person pastes or picks
  themselves; the app runs it off the main thread. A cap (OpenSSH itself has none) would refuse
  real keys made with `-a` in the hundreds; left as it is.
- **CL-L26 Low, the clock's very end.** `Hlc::after` on a record at `wallMs = u64::MAX` and
  `counter = u32::MAX` saturates and is not strictly after it. Since CL-M5 only a device with the
  space key can write such a clock; UwUSSH's `tick` behaves the same.
- **Info, what the server can still do.** `seq` isn't in the AAD: a server can withhold records,
  replay an older authentic version under a higher `seq`, or claim to have accepted a push. These
  are limits of the protocol (§6), the same for the apps; what it can't do is change a record,
  move it to another kind, space or id, forge a tombstone or choose a clock.
- **Info, secrets and the page.** The page gets the JSON of the shown kinds; a `secret` appears
  as "there is one" without its value. `suite_reveal` and `suite_copy` take any live secret's id
  in the open space — the same trust as `reveal_field`/`copy_field` for Bitwarden items: the page
  is the app's own code (CSP `script-src 'self'`), and copying goes through Rust with the usual
  clearing. Plaintexts in Rust are `Zeroizing`; the spaces (`suite::Cache`, keys `ZeroizeOnDrop`)
  live in `Unlocked` and go on lock. The JSON of hosts and identities (no secrets) isn't zeroised.
  Logs carry kinds and ids, never values.
- **Info, files.** `.rdp` files: no password, `redirectdrives:i:0`, CR/LF removed from every text
  value, numbers typed. File names through `safe_file_name` into Downloads (Android: MediaStore
  Downloads, where other apps with storage access can read an exported private key — the person
  asked for the export; iOS: the app's own folder).
- **Info, deep links.** `uwussh://connect/<id>` / `uwurdp://connect/<id>` are built from the id
  parsed as a UUID and only for a live host; nothing else travels. Not offered on phones.
- **Info, keys.** Ed25519 from `OsRng`; a passphrase encrypts the OpenSSH key the way `ssh-keygen`
  does (bcrypt-pbkdf, 16 rounds, aes256-ctr) and goes into its own secret. Imported text is parsed
  by `ssh-key` (no panics on bad input; an error message without key material); PEM and PuTTY keys
  are stored as they are, after a first-line check.
- **Info, what is never written.** `manifest`, the assistant's kinds and unknown kinds are
  neither shown nor sealed; a Put keeps a record's kind; payloads are edited as JSON objects, so
  fields of newer apps survive.
