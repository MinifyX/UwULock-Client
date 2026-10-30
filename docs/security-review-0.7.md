# Security review, 0.7 (October 2026)

The review of the native Apple app, done on 30 October 2026 on `feat/native-apple` over
everything since `v0.7.0-beta.1`: the app logic moved out of Tauri into `crates/uwulock-app`, the
UniFFI layer `crates/uwulock-ffi`, the SwiftUI app in `apps/apple` (iOS, iPadOS, macOS; the
credential provider and the Safari web extension) and the new build and release path
(`.github/workflows/apple.yml`, `scripts/apple-release.sh`, Sparkle signing in
`scripts/release.mjs`). Protocol and crypto in `uwulock-bitwarden` didn't change.

Severity as in 0.4: **High** is a secret leaving the device, or the server able to read or swap
a secret. **Medium** needs a hostile party (server, org member, page, app) or misleads the user
into leaking, but is real. **Low** is defence in depth. **Info** is worth knowing.

No Critical, High or Medium findings. All Low findings are fixed; the Info findings are fixed or
accepted below.

## App core and FFI (R1)

| ID   | Severity | Finding                                                                                                                                                                 | Status                                                                                                                                                                                                                                                 |
| ---- | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| R1-1 | Low      | `clipboard.rs`: `clear_if_ours` checked the generation and read the hash under two separate locks, so a copy in between could be cleared or the new secret left behind. | **Fixed.** Check and take happen under one guard.                                                                                                                                                                                                      |
| R1-2 | Low      | `vault.rs`: a lock (or system lock) arriving while an unlock was still deriving keys was lost; the unlock then opened the vault afterwards.                             | **Fixed.** `unlock` and `unlock_with_hello` remember the lock counter at the start; `open_unlocked` refuses (`locked`) when it moved. `lock_for_system` bumps the counter even with nothing open; `lock_now`/`log_out` clear and bump under one guard. |
| R1-3 | Low      | `uwulock-ffi` `call`: the JSON arguments (master password, PIN, keys) stayed in a plain `String` after parsing.                                                         | **Fixed.** `args_json` is `Zeroizing`. The typed argument structs keep `String` fields; app code wraps them right away (partial, see R1-7).                                                                                                            |
| R1-4 | Low      | `extras.rs` `free_path`: picked a free name, then created it; a symlink planted in between was followed (downloads, private exports).                                   | **Fixed.** `write_new` opens with `create_new` (private files with mode `0600`) and moves on to the next name on `AlreadyExists`; a symlink is never followed. Test covers numbering and a planted symlink.                                            |
| R1-5 | Info     | A failed argument parse echoed the serde message, which can quote input (a password).                                                                                   | **Fixed.** The error names the command, the error class and line/column only.                                                                                                                                                                          |
| R1-6 | Info     | `extension_clear_outbox`/`extension_set_aside` took any file name from Swift; `outbox_name` was unused.                                                                 | **Fixed.** Names are filtered through `passkeys::apple::outbox_name` (`ours`). A needless `Zeroizing` copy in `extension_store` is gone (UniFFI buffers aren't wiped anyway).                                                                          |
| R1-7 | Info     | Strings crossing UniFFI (and Swift `String`s) can't be wiped.                                                                                                           | **Accepted**, as L8 in `security-review-2026-09.md`.                                                                                                                                                                                                   |

## Apple app, extensions and release (R2)

| ID    | Severity | Finding                                                                                                                                                         | Status                                                                                                                         |
| ----- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| R2-1  | Low      | `AppModel`: the `UwULockPhase` launch override (jump to a screen) also worked in Release builds.                                                                | **Fixed.** Read only under `#if DEBUG`.                                                                                        |
| R2-2  | Low      | `Core.swift`: `UWULOCK_DATA_DIR` could point a Release build at another data folder.                                                                            | **Fixed.** Honoured only under `#if DEBUG`.                                                                                    |
| R2-3  | Low      | macOS clipboard: sensitive copies went to Universal Clipboard and clipboard managers.                                                                           | **Fixed.** Sensitive copies use `prepareForNewContents(with: .currentHostOnly)` plus the concealed and transient types.        |
| R2-4  | Low      | `release.mjs` put an ad-hoc signed DMG into the Sparkle appcasts when notarisation was missing.                                                                 | **Fixed.** Only a notarised DMG is Sparkle-signed and listed; `--allow-adhoc` is needed otherwise.                             |
| R2-5  | Low      | Sparkle (`from: 2.7.0`) and XcodeGen (`brew install`) unpinned in the release build.                                                                            | **Fixed.** Sparkle pinned to the 2.10.1 revision; XcodeGen 2.46.0 downloaded with a SHA-256 check in `apple-release.sh xcode`. |
| R2-6  | Info     | `release.mjs` could pick a CI run of another event (a PR from a branch named like the tag).                                                                     | **Fixed.** Only `push` runs on the ref.                                                                                        |
| R2-7  | Info     | `apple-release.sh keychain` wrote the certificate files with the default umask.                                                                                 | **Fixed.** `umask 077`.                                                                                                        |
| R2-8  | Info     | Credential provider logged steps with public privacy.                                                                                                           | **Fixed.** `privacy: .private`.                                                                                                |
| R2-9  | Info     | `SsoBrowser` force-unwrapped the first window scene.                                                                                                            | **Fixed.** Falls back to a plain anchor.                                                                                       |
| R2-10 | Info     | `PasskeyVault.cborHead` encoded lengths ≥ 256 wrongly.                                                                                                          | **Fixed.** 2-byte and 8-byte heads.                                                                                            |
| R2-11 | Info     | `extensionOutbox` read outbox files of any size.                                                                                                                | **Fixed.** Files over 64 KiB are skipped.                                                                                      |
| R2-12 | Info     | Security settings (auto-lock, clipboard time) live in `UserDefaults`, not in the vault.                                                                         | **Accepted.** Same as the Tauri app's settings file; changing them needs access to the user's container.                       |
| R2-13 | Info     | AutoFill: a login picked from the full list fills without a host warning; `excludeCredentials` is ignored; a single matching passkey signs right after Face ID. | **Accepted.** System UI already shows the site; same behaviour as Apple Passwords.                                             |
| R2-14 | Info     | `.userPresence` lets the device passcode stand in for Face ID / Touch ID.                                                                                       | **Accepted** by design (as the Tauri app).                                                                                     |
| R2-15 | Info     | SSO uses a non-ephemeral web session (shared Safari cookies).                                                                                                   | **Accepted.** Needed for single sign-on at the identity provider.                                                              |
| R2-16 | Info     | Direct (DMG) and App Store builds share the App Group and Keychain groups.                                                                                      | **Accepted.** Needed for the migration; both are signed by the same team.                                                      |
| R2-17 | Info     | `UIFileSharingEnabled` shows the documents folder in Files.                                                                                                     | **Accepted**, as the Tauri app (exports and downloads land there).                                                             |
| R2-18 | Info     | No screen-capture protection on macOS; the clipboard isn't cleared on quit.                                                                                     | **Accepted**, the latter as L5 in `security-review-2026-09.md`.                                                                |

## Against 2026-09

L5 (clipboard on quit) and L8 (zeroisation gaps) still apply in the native app in the same way
(R2-18, R1-7). No earlier finding came back.

## Not covered

No signed CI run yet (TestFlight, notarisation); AutoFill and the Safari web extension were not
tested end to end on a device.
