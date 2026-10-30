# uwulock-app

UwULock's app without a window. Everything the page asks for is a plain
method on `App`, with the same name, arguments and JSON as the IPC command.
That covers accounts on disk, the open vault, sync, extras, health, Sends,
the suite, moving and importing, and passkeys.

- The Tauri app (`apps/desktop/src-tauri`) wraps each method in a
  `#[tauri::command]` (`src/commands.rs`) and implements `Host` (`src/host.rs`).
- The native Apple app (`apps/apple`, `docs/native-apple-plan.md`) reaches the
  same methods through `crates/uwulock-ffi` (UniFFI): one
  `UwuLockCore.call(command, argsJson)` for all of them, dispatched by
  `uwulock-ffi/src/dispatch.rs` (generated from the Tauri app's `commands.rs` by
  `scripts/ffi-dispatch.py`), and a Swift `HostDelegate`
  (`apps/apple/UwULock/Core/AppleHost.swift`) behind its `Host`.

The protocol and the crypto are `uwulock-bitwarden`'s and `uwulock-core`'s.

## Modules

| Module                           | What                                                                                                                                                                                   | Commands |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- |
| `vault`                          | logging in (password, 2FA, new device, SSO), unlocking, locking, accounts, sync, items, folders, reveal and copy, the generator, an item's passkeys; `VaultState`, `Failure`, `Result` | 37       |
| `account`                        | what is kept on disk and where (`Storage`, `Account`); unchanged format                                                                                                                | –        |
| `extras`                         | UwULock Server's extras: travel mode, icons, versions, reminders, file requests, masked addresses, share as Send                                                                       | 33       |
| `health`                         | the password check, its review one login at a time, the breach-mail opt-in                                                                                                             | 7        |
| `importing`                      | another app's export file, KeePass KDFs                                                                                                                                                | 4        |
| `moving`                         | moving a vault in from Bitwarden or Vaultwarden                                                                                                                                        | 8        |
| `sends`                          | Sends                                                                                                                                                                                  | 5        |
| `suite` (+ `suite/plan`)         | UwUSSH's and UwURDP's hosts, logins and keys                                                                                                                                           | 10       |
| `wifi`                           | Wi-Fi networks to the phone                                                                                                                                                            | 2        |
| `autofill`                       | the system's AutoFill/credential provider, iOS 26 credential exchange, the extension's protocol                                                                                        | 6        |
| `passkeys` (+ `logins`, `apple`) | the vault's passkeys and logins for the system's providers: requests, answers, settings (`Provider`); the Apple extension's sealed list and outbox                                     | 4        |
| `safari`                         | the Safari extension: switched on?                                                                                                                                                     | 2        |
| `system`                         | links out of the app                                                                                                                                                                   | 3        |
| `clipboard`                      | copies that clear themselves (on top of the host's clipboard)                                                                                                                          | –        |
| `live`                           | changes from other devices as they happen                                                                                                                                              | –        |
| `session_lock`                   | locking when the screen locks or the computer sleeps (`App::system_locked`)                                                                                                            | –        |
| `hello`                          | unlocking with Windows Hello, Touch ID or a phone's biometrics, over `Host::biometric_*`                                                                                               | –        |
| `phone`                          | locking in the background (`App::suspended`, `App::resumed`)                                                                                                                           | –        |
| `host`                           | the `Host` trait                                                                                                                                                                       | –        |
| `runtime`                        | the shared Tokio runtime: `shared`, `spawn`, `spawn_blocking`, `block_on`                                                                                                              | –        |

That makes 121 commands. The Tauri app adds seven of its own: the updater,
appearance, distribution and quitting.

`build.rs` sets `cfg(desktop)` / `cfg(mobile)` the way Tauri does. The moved
code keeps its gates.

## Using it

```rust
let app = App::new(Storage::new(data_dir)?, Arc::new(MyHost::new()));
app.start(); // sync, live changes, extras, screen lock, passkey providers
app.vault_status();
app.unlock(password).await?;
```

- **Cloning:** `App` is cheap to clone, and every clone is the same app.
- **Threading:** async methods run on `runtime::shared()` or on the runtime the
  caller is already in. The Tauri app hands that runtime to Tauri
  (`tauri::async_runtime::set`).
- **Accessors:** `app.vault()`, `app.moves()`, `app.passkeys()` and `app.host()`
  give the state behind the methods to platform code.
- **Listeners:** `app.listen(event, f)` / `app.unlisten(id)` let platform code
  hear the app's own events, for example the Windows plugin refreshing on
  `vault-changed`.

## The `Host` contract

`Host` (`src/host.rs`) is what the app needs from the program around it.

- **Threads:** every method may be called from any thread. The ones that wait
  for the system or the person block, and the app calls those off the
  runtime's threads.
- **Events:** `emit` hands an event to the page unchanged. The app's own
  listeners have already heard it.
- **Errors:** they are the person's words. The app shows them as they are.
- **Missing features:** what a system doesn't have answers with the default,
  either nothing or `unsupported`.

**Required:**

| Method            | Does                                                |
| ----------------- | --------------------------------------------------- |
| `emit`            | sends an event to the page                          |
| `open_url`        | opens a page in the browser                         |
| `clipboard_copy`  | copies, with sensitive and expiry hints             |
| `clipboard_clear` | clears the clipboard if it still holds that SHA-256 |
| `download_dir`    | gives the folder for saved files                    |

**Defaulted:**

- **Window:** `show_window`.
- **Biometrics:** `biometric_kind`, `biometric_secret(name, challenge, create, prompt)`,
  `biometric_forget`.
- **System:** `screen_locked`, `publish_download` (Android),
  `wifi_join` / `wifi_settings`.
- **Platform passkey providers:** `passkey_provider_apply(app, settings)` /
  `passkey_provider_state(app)` (Linux virtual key, Windows plugin, Android).
- **AutoFill:** `autofill_provider_status` / `autofill_provider_request`,
  `credential_exchange_pending` / `credential_exchange_import`,
  `autofill_log` / `autofill_log_clear`.
- **Safari:** `safari_extension_status` / `safari_extension_open`.
- **Apple only:** `extension_problem`, `extension_note`, `extension_store`,
  `extension_outbox`, `extension_clear_outbox`, `extension_set_aside`,
  `extension_clear`. These cover the AutoFill extension's sealed list,
  Keychain key and outbox.

## Events

Every event goes through `App::emit`, then to the listeners, then to
`Host::emit`. Names and payloads are the page's (`apps/desktop/src/lib`).

| Event                                           | From                                            | Payload                                  |
| ----------------------------------------------- | ----------------------------------------------- | ---------------------------------------- |
| `vault-status`                                  | `vault`, `passkeys/apple`                       | the status, as `vault_status` returns it |
| `vault-changed`                                 | `vault`, `extras`, `passkeys`, `passkeys/apple` | none                                     |
| `auth-request`                                  | `live`                                          | `{ id }`                                 |
| `suite-changed`                                 | `live`                                          | none                                     |
| `uwu-notice`                                    | `live`                                          | `{ kind, id }`                           |
| `uwu-changed`                                   | `extras`                                        | the status, as `uwu_status` returns it   |
| `health-progress`                               | `health`                                        | `{ done, total }`                        |
| `import-progress`                               | `importing`                                     | `{ done, total }`                        |
| `move-progress`, `move-finished`, `move-failed` | `moving`                                        | the progress, the summary, the `Failure` |
| `passkey-request`                               | `passkeys`                                      | the request the window answers           |
| `passkey-provider-warning`                      | `passkeys`                                      | the warning                              |

The Tauri app emits one event of its own: the macOS quit event
(`uwu_macos::QUIT_EVENT`).

## Tests

Run `cargo test -p uwulock-app`. The tests moved over with their code.
`host::NoHost` is the test host: nothing on the system.
