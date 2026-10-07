# Passkeys outside the browser extension

The browser extension has made and used passkeys since 0.3 (docs/extension.md). From 0.5 the apps
offer the vault's passkeys to the rest of the system too: to browsers without the extension and to
other apps. Every way in ends in the same code (`apps/desktop/src-tauri/src/passkeys/`) and the
same format: Bitwarden's `fido2Credentials`, one passkey per login, so passkeys made here work in
Bitwarden's apps, the extension and the web vault, and the other way round.

| System             | How                                                  | State                 |
| ------------------ | ---------------------------------------------------- | --------------------- |
| Linux              | a virtual FIDO2 security key, made by a root helper  | off until switched on |
| Windows 11         | a plugin passkey manager (webauthn.dll's plugin API) | experimental, off     |
| Android 14+        | Credential Manager's provider service                | on, picked in Android |
| iOS 17+, macOS 14+ | an AutoFill credential provider extension            | needs a signed build  |

None of it has been tried on a real device yet; see [What is open](#what-is-open).

## The common part

`crates/uwulock-authenticator` holds what has no platform in it, with tests:

- `ctap2`: CTAP 2.0 (`authenticatorMakeCredential`, `GetAssertion`, `GetNextAssertion`,
  `GetInfo`, `Selection`, the legacy "touch to select"), canonical CBOR, ES256 only,
  resident keys always, user verification built in (`uv: true`), no PIN. A `Backend` trait
  answers; the app's `DesktopBackend` asks the person.
- `ctaphid`: the USB HID framing (INIT, PING, CBOR, CANCEL, KEEPALIVE, ERROR; 64-byte reports,
  channels, up to 7609 bytes).
- `uhid`: Linux's `/dev/uhid` events, with a FIDO report descriptor (usage page `0xF1D0`).
- `broker`: the frames between the app and its root helper on Linux (`uwulock-uhid-broker`).
- `rpid`: which relying party ids are taken: lower-case host names (LDH, `xn--` for IDNs), at
  most 253 characters, no IP addresses, no public suffixes (`com`, `co.uk`, `github.io`; the
  Public Suffix List compiled in through the `psl` crate). `localhost` is fine.
- `opsign`: checks Windows' signatures on the plugin's requests (P-256 or RSA keys).
- `webauthn`: WebAuthn JSON as Android's Credential Manager hands it over, client data, the
  `android:apk-key-hash:` origin, Digital Asset Links.
- `apple`: the sealed list and outbox the Apple extension reads and writes.

Every rpId a request names goes through `rpid` first (CTAP2 parsing, Android, the Apple outbox):
browsers check that a page may speak for its rpId, local callers don't have to. Names (`rp.name`,
`user.name`, `user.displayName`) lose control and bidi characters and are cut at 64 characters.

The app (`passkeys/mod.rs`) finds a site's passkeys in the open vault (relying party id equal,
and, when the site sends an allow list, the passkey's credential id in it), makes a passkey into a login (the one the
person picks, else a new one; a login that has a passkey gets it replaced, since Bitwarden keeps
one), saves it to the server, and signs. Only the account on screen is used. A locked vault
answers nothing.

### Asking the person

On Linux and Windows every request opens UwULock's own dialog (`PasskeyRequestDialog`): which
passkey, or which login a new one goes into, Decline or Allow. While the vault is locked the
request waits in a notice over the lock screen. Requests time out after 120 seconds; the browser
gets keepalives ("waiting for the user") meanwhile and may cancel. When the site asks for user
verification, the master password is typed again and checked against the account's protected user
key; only then does the answer carry the UV flag. Android and the Apple extension ask in the
system's own sheets: the screen lock, Face ID or Touch ID.

The dialog starts on _Ablehnen_ and its yes works only 0.7 s after the request turned up, so a
request that pops up while the person types elsewhere isn't accepted by a stray Enter. The rpId
comes first and large, the site's own name after it. It names who asks, as the system says: on
Linux the programs holding the key's hidraw node open (from `/proc/*/fd`), on Windows "Windows"
(only signed requests get this far); a program that isn't a browser UwULock knows, installed by
the system (under `/usr`, `/opt`, `/snap` or Flatpak's `/app`, owned by root), gets a warning.
That name is **a hint, not proof**: a program of the same user can still drive an installed
browser (`firefox --headless --marionette`, `LD_PRELOAD`), and then the dialog says "Firefox" and
even the origin is real. So the dialog says so for known browsers too ("Wer fragt, sagt dein
System … ein Hinweis, kein Beweis"), and asks to decline unless the person just started a
sign-in.

Silent requests (`up: false`, the check browsers send before the real request: is one of these
credential ids here?) never reach the dialog and never get a real signature:

- without an allow list they are refused (`NO_CREDENTIALS`) — they would list the person's
  accounts at any site;
- with one, the answer says whether one of the named passkeys is in the open vault, without the
  account (no user entity) and signed with a throwaway key: nobody gets a signature from a
  passkey without the person's yes;
- at most 20 at once, then one every 3 seconds.

A request with `up: true` and an allow list none of whose passkeys is in the open vault is
answered `NO_CREDENTIALS` at once (only a caller that knows the credential ids learns that);
without an allow list the person is asked even when there is nothing to pick.

Passkeys that count signatures (imported ones; UwULock's own stay at 0) are signed one at a time,
counted up from the latest saved copy and saved before the signature goes out; when the save
fails, there is no signature.

## Linux: a virtual security key

Settings → Security → _Passkeys in browsers (security key)_. A HID device made through
`/dev/uhid`; the kernel makes a `/dev/hidraw*` of it like a USB key's, and Firefox, Chromium and
everything with libfido2 talk CTAP2 to it. Switching off removes the device.

`/dev/uhid` stays root's. Whoever may write there can make any HID device: a keyboard that types
into the terminal or the lock screen, a device with the ids of a buggy vendor driver that feeds
the kernel crafted descriptors. So UwULock never opens it. A small root helper does:

- **`uwulock-uhid-broker`** (`crates/uwulock-uhid-broker`, installed as
  `/usr/lib/uwulock/uwulock-uhid-broker`), socket-activated by systemd:
  `uwulock-uhid-broker.socket` listens on `/run/uwulock/uhid-broker.sock` (mode 0666,
  `Accept=yes`, at most 8 connections) and starts `uwulock-uhid-broker@.service` as root for each
  connection — no network, `DeviceAllow=/dev/uhid`, read-only system, a `@system-service`
  syscall filter. Its only capability, `CAP_SYS_PTRACE` (with `ProtectProc=invisible`), lets it read
  `/proc/<pid>/exe` of the program it serves or refuses, for the journal and for the warning
  below; it drops every capability (`capset`; only the bounding set keeps it, unreachable under
  `NoNewPrivileges`) right after the lock, before it reads anything from the app.
- **Who**: only the user of the active session on seat0 (`SO_PEERCRED` against logind's
  `/run/systemd/seats/seat0`), checked when connecting and every 2 seconds after; when the seat
  changes hands, the device goes. One device per user (a lock in `/run/uwulock`, which records
  the pid it went to and that process's start time; the journal names that program's executable,
  escaped and cut to 256 bytes, and only while the pid is still that process and the seat user's
  own; otherwise it says "another process"). The app connects again by
  itself (after 5 seconds, then up to every minute) while the setting is on; switching off while
  it connects drops the new connection again.
- **Another program of yours first**: the broker checks the uid, not the program, so any program
  of the person at the seat that connects before UwULock gets the one key per user — and with it
  a FIDO key of its own in front of the browser (it could keep the private key of a passkey the
  person thinks they save "in UwULock"). The broker answers UwULock "already there: held by
  `<exe> (pid …)`", and UwULock shows it: _Ein anderes Programm hält deinen
  UwULock-Sicherheitsschlüssel_ in Settings → Security and as a note, logged at warn. This is a
  limit of the design, not something the broker can prevent: requiring the packaged
  `/usr/bin/uwulock-desktop` as the peer's executable wouldn't stop `LD_PRELOAD` or ptrace into
  it. If the warning names a program you don't know, quit it and check the computer.
- **Limits**: at most 8 connections (4 per user); no systemd trigger limit, which any local
  user could trip to leave the socket failed — instead systemd pauses accepting while
  connections come faster than 50 in 2 seconds (`PollLimit…`, systemd 255+).
- **What**: exactly one `UHID_CREATE2`, everything compiled in — UwULock's FIDO report
  descriptor, the name, bus USB, vendor and product 0000, `uniq = uwulock-<uid>`. The app never
  holds `/dev/uhid` and can't send uhid events: it sends 64-byte reports in frames
  (`uwulock_authenticator::broker`), and the broker writes the `UHID_INPUT2` itself. So no
  `UHID_DESTROY` followed by a `UHID_CREATE2` with another descriptor (which the kernel accepts on
  the same fd) can come from the app; any other frame ends the connection and the device.
- **Back**: from the kernel only 64-byte output reports, opens (with the hidraw node's name, so
  the app can tell who opened it) and closes reach the app; feature report requests are answered
  in the broker. The connection ending (app closed, setting off) closes `/dev/uhid`, which
  removes the device.

Packaging: the `.deb` and `.rpm` (and so the Arch package, which repacks the `.deb`) bring the
broker, its two systemd units and `/usr/lib/udev/rules.d/60-uwulock-passkeys.rules`, which gives
the person at the seat (`uaccess`) the key's hidraw device only (bus USB, vendor and product
0000 — only the broker makes such a device; systemd's `60-fido-id.rules` also recognises it by its
descriptor). The `.deb`/`.rpm` post-install script enables the socket (and takes away the
`/dev/uhid` access packages before 0.5 gave); removing the package disables it. The Arch package
doesn't switch services on by itself: its install script says
`sudo systemctl enable --now uwulock-uhid-broker.socket`. Without the socket the setting says
what to do.

## Windows 11: plugin passkey manager (experimental)

Settings → Security → _Passkeys in Windows_. Windows 11 with third-party passkey manager support
(24H2/25H2 builds 26100/26200 with a recent cumulative update) lets a manager stand next to
Windows Hello through webauthn.dll's plugin API. When switched on, UwULock

- registers a COM class for the user (`HKCU\Software\Classes\CLSID\{5B0C8E7A-…}`,
  `LocalServer32 = "uwulock-desktop.exe" --passkey-plugin`) so Windows can start it. That key
  is the user's: while UwULock isn't running, any program of theirs can point it at itself and
  get Windows' (signed) requests — Linux's "another program first" (above). At every start with
  the setting on, UwULock reads the whole CLSID key; when it holds anything but its own
  `LocalServer32` default (another or a `REG_EXPAND_SZ` command, a `ServerExecutable` value, a
  `TreatAs`, `InprocServer32` or other subkey), UwULock warns in Settings → Security and as a note
  (_Der Windows-Eintrag … zeigte auf ein anderes Programm_), logs it at warn, deletes the key and
  writes it anew. A key it can't clean keeps the plugin off. It notices only afterwards, hence the
  warning,
- adds itself with `WebAuthNPluginAddAuthenticator` (AAGUID `4d0c2e23-4c15-c411-9bd1-f265e4266ad6`),
- tells Windows which passkeys there are (`WebAuthNPluginAuthenticatorAddCredentials`: ids,
  sites, names; no keys), refreshed when the vault changes,
- answers the CTAP2 requests Windows hands over like the Linux key does, in its own dialog —
  **only requests Windows signed**.

The COM class can be called by any program of the user (`CoCreateInstance` with
`CLSCTX_LOCAL_SERVER`), and since Windows 10 1903 ordinary programs can't reach FIDO keys
directly: the WebAuthn service binds origins. So UwULock checks every request: Windows hands the
plugin the public half of an operation signing key when it is added
(`WebAuthNPluginAddAuthenticator`, or `WebAuthNPluginGetOperationSigningPublicKey` when it was
added before — never from a file, which another program of the user could replace), and signs
each request's
encoded bytes with the private half. `uwulock_authenticator::opsign` verifies that signature
(ECDSA P-256 over SHA-256, or RSA PKCS #1 v1.5/PSS; CNG key blob or DER) before anything else; a
request without a valid one gets `E_ACCESSDENIED` and nobody is asked. `CancelOperation` counts
only for the transaction in flight (its id, a random GUID only Windows knows, kept for the whole
process); a cancel can only end a request. One request at a time: a second one while the first
runs gets busy (`ERROR_BUSY`) and leaves the first one cancellable. Without Windows' key the plugin is removed again and
doesn't start (adding it next time hands over a fresh key), and the setting says why. Requests are capped at 7609 bytes.

The person then switches UwULock on in Windows: Settings → Accounts → Passkeys → Advanced options.
Switching off removes the authenticator, the credentials, the registry keys and the listeners
that kept Windows' list current.

Open points: not tried on a real Windows yet, so which data the cancel signature covers isn't
confirmed; until it is, that signature is only logged. The process-wide COM access rights (`CoInitializeSecurity`) are left at their default,
since the WebView initialises COM first; the signature check is what keeps other programs out.
Microsoft may require package identity (MSIX) for plugin managers in later builds; the API is
loaded at run time, so on a Windows without it the setting just says so.

## Android 14+: Credential Manager

The provider is part of the app (`crates/tauri-plugin-uwulock-mobile/android`):
`PasskeyProviderService` (a `CredentialProviderService`, enabled from API 34 via a resource bool),
`PasskeyActivity` (translucent, makes and signs) and `PasskeyBridge` (JNI into
`passkeys/android.rs` in the same process). The person picks UwULock in Android's Settings →
Passwords, passkeys & accounts.

- Listing: when UwULock runs with an open vault, the service lists the site's passkeys; otherwise
  it offers _Unlock UwULock_, which opens the app and lists them after the unlock.
- Listing: the caller is checked before anything is listed — the rpId must pass `rpid`, and the
  origin of a privileged browser must match it, or the site's asset links must name the app
  (cached per site, package and certificates: 5 minutes allowed, 1 minute refused). So an app
  that isn't trusted never sees the person's account names for another site.
- Verification: `BiometricPrompt` with a strong biometric or the screen lock before making or
  signing, unless the site says `userVerification: "discouraged"`. Rust parses the request once
  and tells Kotlin whether to ask (the bridge's `verification` call); Rust refuses a request that
  wants verification when it wasn't done, so the two can't disagree.
- Origins: a browser on Google's list of privileged browsers (`res/raw/privileged_browsers.json`,
  release builds) hands over the web origin Android checked against the browser's certificate;
  UwULock checks that the relying party id is the origin's host or a parent domain of it, over
  HTTPS (the browser has already refused relying party ids like public suffixes). Any other app gets the `android:apk-key-hash:` origin, and only when the site's
  `https://<rp id>/.well-known/assetlinks.json` names the app's package and certificate
  (`delegate_permission/common.get_login_creds` only — `handle_all_urls` is App Links, which a
  site may grant without sharing logins). No redirects, 10 s, 256 KiB. A relying party id that
  is a public suffix is refused on both ways.
- An exclude list hit answers `InvalidStateError`.

### Passwords

The same provider also offers logins' passwords (`TYPE_PASSWORD_CREDENTIAL`), and Android 8+
apps and browsers that don't ask Credential Manager for passwords get them from UwULock's
autofill service (`UwuLockAutofillService`, `FillActivity`, `FillFields`). Kotlin only says who
asks (`LoginBridge`); `passkeys/android_logins.rs` decides, with the pure part in
`passkeys/logins.rs` and the matching of `uwulock_authenticator::autofill` (Bitwarden's match
detection, as on Apple).

- Which logins: with a password, not in the trash, not archived, and not marked to ask for the
  master password again (Android only has the screen lock or a biometric).
- Who asks: a privileged browser's page — Credential Manager's `getOrigin`, or for autofill the
  page's `webDomain` when the browser's package _and_ certificate are on the privileged list —
  matches web logins. Any other app matches its own logins (`androidapp://<package>`) and website
  logins only of sites whose Digital Asset Links grant it `get_login_creds` (same check and cache
  as for passkeys). Which sites to ask: the one the package name points at (`com.example.app` →
  `example.com`) and, in an app's WebView, the page's — never every site in the vault. A
  WebView's domain is never taken as the page's address, so an app can't phish another site's
  login with a WebView. Without the app's certificates (Android hid the app from UwULock) only
  `androidapp://` logins match.
- Listing (`logins`) carries item id, name and user name, never a password. Credential Manager
  shows `PasswordCredentialEntry`s; the autofill service shows one suggestion per login whose
  values are empty and which is locked behind `FillActivity` (dataset authentication). A locked or
  closed UwULock offers _Unlock UwULock_ in both, which opens the app and lists afterwards. No
  match: no suggestion.
- Filling (`password`): after `BiometricPrompt` (strong biometric or the screen lock) — every
  time, for every login. Rust refuses without `verified` and checks again that the login belongs
  to the caller; for autofill the caller is read again from the screen Android hands the activity,
  not from the suggestion. Only then does Android get user name and password.
- Saving passwords isn't offered (no `SaveInfo`, no password create entry): logins are saved in
  UwULock.

### Being the default provider

`providerStatus` (mobile plugin) answers `ProviderState`: `supported` (Android 14+ or autofill),
`enabled` (Credential Manager's `isEnabledCredentialProviderService`, `null` when it won't say),
`autofill` (`hasEnabledAutofillServices`: UwULock's service is the one picked) and `direct`.
`providerRequest` opens Android's own place for either: `credentials` →
`Settings.ACTION_CREDENTIAL_PROVIDER` with `package:` on Android 15+, which asks directly, and
androidx's `createSettingsPendingIntent()` on Android 14 (and when the action has no activity);
`autofill` → `ACTION_REQUEST_SET_AUTOFILL_SERVICE`. It answers the new state when the person
comes back — on Android 14 right away (a pending intent gives no result), so the app asks
`providerStatus` again when it is shown.

## iOS 17+ and macOS 14+: the AutoFill extension

`apps/desktop/src-tauri/apple/PasskeyProvider` (Swift): an `ASCredentialProviderViewController`
for passkeys and passwords, the same code for both. It is its own process, started by the system while UwULock
may not run, so it never sees the open vault. Instead:

- While the vault is open and the setting is on, the app writes a **sealed list** of the
  passkeys of the account on screen (`passkeys.sealed`; `passkeys-apple.account` records whose)
  into the App Group folder it shares with the extension, and on iOS fills the system's list of
  passkey identities (`ASCredentialIdentityStore`: sites, names, ids).
- The list is sealed with the **provider key**: 32 random bytes per account, AES-256-GCM, format
  version 2: `0x02 ‖ n ‖ account id ‖ nonce ‖ ciphertext ‖ tag` with AAD
  `uwulock-passkeys-v2:<list|outbox>:<account>`, so a list doesn't open as an outbox entry and
  neither opens under another account's name. Inside, each private key is sealed on its own,
  bound to the account and the credential id; the extension opens only the one it signs with.
- The list carries a **generation** (milliseconds, only ever going up); the extension keeps the
  highest it has seen per account in its own defaults (not the shared folder) and refuses an
  older list, so an old `passkeys.sealed` can't be put back to resurrect deleted passkeys.
- The app keeps each account's provider key sealed under that account's user key
  (`passkeys-apple-<account>.key`, an EncString like the rest of the vault, written atomically);
  the extension gets the key of the account in the list from the shared Keychain,
  `kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly` with `.userPresence` and not synchronizable,
  on iOS and macOS alike — Face ID, Touch ID or the device passcode on every read. The Keychain
  item is labelled with a key id and rewritten whenever it is missing or holds another key
  (switching accounts, the setting off and on again, the passcode removed).
- Passkeys the extension makes go into an **outbox** (one sealed file each) and into the list —
  the extension only adds to a list it could open, never replaces it. The app takes the outbox
  into the vault as new logins when the account of an entry is open; entries of other accounts
  wait for theirs. An entry that doesn't open is moved to `outbox/unreadable/` and tried again
  later, never deleted. A provider key that no longer opens under the user key isn't
  overwritten: it is kept (`.key.old-<ms>`, the last five) and tried on set-aside entries.
  The extension makes passkeys only for rpIds the app takes (host names, as `rpid`; Apple hands
  over only rpIds the site or app may speak for). Credential ids taken in are remembered (the last 1000), so a replayed
  outbox file isn't imported twice.
- **Logging out** (or switching the setting off) removes the list, the Keychain item and the
  system's identities of that account, and its key file unless the outbox still holds passkeys
  of that account (they are taken in after the next login).
- Passkeys that use a signature counter stay out of the list: the extension can't count up in
  the vault. UwULock's own passkeys don't use one.
- Logins marked "ask for the master password again" (re-prompt) stay out of the list and the
  system's identities: the extension has only Face ID, Touch ID or the passcode, and everywhere
  else such a login's passkey needs the master password (desktop, Android, browser extension).
  Sign in with those from UwULock or the browser extension.

- **Passwords** go into the same list (`logins`): per login the item id, name, user name, the
  addresses as match hints and the password, sealed on its own with AAD
  `uwulock-passkeys-v2:password:<account>:<item id>`. Only logins with a password, not deleted,
  not archived, not re-prompt. The system's identity list gets one `ASPasswordCredentialIdentity`
  per address (domain or URL), names only. Matching follows Bitwarden's match detection
  (`uwulock-authenticator/src/autofill.rs`): domain (registrable domain from the Public Suffix
  List, computed in Rust, so Swift only compares host suffixes), host (with port), starts with
  and exact (only where the system hands over the full address); regular expressions and
  "never" don't show up as suggestions, only in UwULock's own searchable list.
- **Both ways in** answer through one path: picking UwULock's entry right in the system's sheet
  (`prepareInterfaceToProvideCredential(for:)`) and picking from UwULock's own list
  (`prepareCredentialList`). The direct pick finds the passkey by credential id (preferring the
  requested rpId), checks the 32-byte client data hash, signs, verifies its own signature and
  answers on the main thread; it never rewrites the system's identity list while the system
  waits for the answer. An identity that is no longer in the list falls back to UwULock's own
  list instead of failing.
- **Diagnostics**: the extension logs to the unified log, subsystem `app.uwulock.passkeys`,
  category `provider` — which way in, rpId, short credential id prefixes, flags, counts, why it
  stopped. No keys, passwords, user handles or client data. On a Mac with the device attached:
  `log stream --predicate 'subsystem == "app.uwulock.passkeys"' --info` (or Console.app).

On macOS the app writes the list from Rust (`passkeys/apple.rs`, Keychain via
`security-framework`), and the extension fills the system's identity list when it runs (and when
switched on in System Settings → Passwords → Password Options).

### Signing it

Nothing of this works unsigned: the App Group, the Keychain group and the AutoFill entitlement
come from a developer team. The App Store builds have them ([app-store.md](app-store.md)); the
GitHub builds don't, and there the setting says so:

- **iOS**: `scripts/ios-build.sh` adds the extension as target `UwULockPasskeys`
  (`app.uwulock.passkeys`, iOS 17) to the generated Xcode project and embeds it in the app's
  PlugIns, unsigned like the app. For TestFlight, `scripts/ios-sign.sh` signs both with the App
  Store profiles: App Group `group.app.uwulock`, Keychain groups `TEAMID.app.uwulock` (the app's
  own) and `TEAMID.app.uwulock.passkeys` (shared), the AutoFill entitlement on both (App Store
  Connect wants it on the container too); it also writes the team into `UwULockKeychainGroup`, which the unsigned build leaves without
  prefix.
- **macOS**: `scripts/macos-passkeys.sh` compiles it into `UwULockPasskeys.appex` (universal,
  ad-hoc signed) on every macOS build, so the Swift can't rot. The Mac App Store build
  (`scripts/build-mas.mjs`, bundle ID `app.uwulock`) carries it, built with
  `UWULOCK_APPLE_TEAM_ID` (Rust reads it at build time for the group paths) and signed with
  `apple/PasskeyProvider/UwULockPasskeys-macOS.entitlements` (App Group `TEAMID.app.uwulock`). The
  DMG (`app.uwulock.desktop`) leaves it out: an extension's ID has to extend its app's, and the
  DMG has no developer signature anyway.

Turning it on: iOS Settings → General → AutoFill & Passwords, macOS System Settings → General →
AutoFill & Passwords; then UwULock's own setting.

### Default provider prompt

After the first unlock UwULock shows a card asking to make it the provider for passwords and
passkeys (`AutofillCard`, `lib/autofillPrompt.ts`): "Als Standard festlegen" asks the system, "Später"
asks again after 7 days, at most three times; once switched on it never asks again. The
settings (phone, iPad, Mac) show the state and the same button. Windows and Linux show neither.

- **iOS 18 / macOS 15**: `ASSettingsHelper.requestToTurnOnCredentialProviderExtension` — the
  system asks in a sheet of its own. **iOS 17 / macOS 14**:
  `openCredentialProviderAppSettings` opens the settings page. The state comes from
  `ASCredentialIdentityStore.getState().isEnabled`. The button also turns on UwULock's own
  setting (the sealed list), without which the extension has nothing to fill.
- macOS goes through the Objective-C runtime from Rust (`src-tauri/src/autofill.rs`, `objc2`),
  only in the build that carries the extension (Mac App Store).
- **Android**: see "Being the default provider" in the Android section.

### Taking in Apple Passwords (iOS 26)

Apple Passwords → "Export data to another app" → UwULock, the FIDO Credential Exchange Format
(CXF). The extension declares `SupportsCredentialExchange` (version 1.0); the app lists the
`ASCredentialExchangeActivity` activity type in `NSUserActivityTypes` (`scripts/ios-build.sh`
reads its value from the SDK). The system starts UwULock with that activity; the mobile plugin
(`CredentialExchange.swift`) keeps only its token. Once the vault is open, the page asks whether
one waits, and after the person said yes the plugin calls
`ASCredentialImportManager.importCredentials(token:)` (only then, only once) and hands the data
as JSON to the import (`lib/import/cxf.ts`), which shows everything before anything is saved:
logins with passwords, passkeys and one-time codes, cards, Wi-Fi, addresses, notes, the rest
as fields. Nothing is written to disk in between. The Mac app doesn't take part (the API is
Swift-only and the Mac app has no Swift of its own yet); its extension doesn't declare it.

## Threat model

What is protected: the private keys of passkeys, which are as good as the account they sign in to.

- **At rest**: no private key is ever on disk in the clear. The vault cache holds them encrypted
  as before; the Apple list and outbox are sealed with the provider key, which is itself either
  sealed under the user key (only while unlocked) or in the Keychain behind user presence,
  this device only, never synced or backed up (iOS and macOS).
- **Linux**: the keys stay in the UwULock process; `/dev/uhid` stays root's, and the broker makes
  nothing but the FIDO key, for the person at the seat. Installing UwULock doesn't let other
  programs make keyboards or feed the kernel's HID drivers. A program of the same user can still
  open the key's hidraw node and send requests (as it could to a USB key), and could watch the
  screen, but gets no signature without the person's yes to that site's request; the dialog
  names the programs holding the key open (a hint, not proof) and warns when one isn't a known
  browser. Such a program can also take the one key per user before UwULock does and stand in
  for it; UwULock then warns visibly (see "Another program of yours first"). Before UwULock was
  installed, that needed root. The browser
  decides which site may ask for which relying party, as with any security key; UwULock refuses
  rpIds that aren't host names or are public suffixes.
- **Windows**: only requests signed by Windows' WebAuthn service are taken, so another program of
  the user can't drive the plugin (or make the dialog say "Windows" asks). Windows binds the
  origin to the rpId. While UwULock isn't running, a program of the user can re-point UwULock's
  `LocalServer32` entry (or its CLSID key otherwise) at itself and get those requests; UwULock
  notices at its next start, warns and writes the key anew.
- **Silent checks** (`up: false`, Linux and Windows): only with an allow list, never with the
  account, throttled, never signed with a passkey. A program that knows a credential id (sites
  hand those to anyone who types a user name) learns whether that passkey is in the open vault,
  as it could with a USB key.
- **Android**: the provider runs in UwULock's process; Android binds the service only with
  `BIND_CREDENTIAL_PROVIDER_SERVICE`, and the activity isn't exported. The origin comes from
  Android (privileged browsers) or Digital Asset Links, so an app can't sign in to a site that
  doesn't trust it. Each passkey use needs the screen lock or a strong biometric unless the site
  says verification is discouraged.
- **Android passwords**: a password only leaves the vault after the screen lock or a strong
  biometric, for each fill; suggestions and Credential Manager entries carry names only, so
  nothing secret reaches the system (or the app asking) before the person picked a login and
  passed the check. The autofill service is bound only with `BIND_AUTOFILL_SERVICE`; a page's
  domain counts only from a privileged browser with the listed certificate. An app that took a
  package name a login names (`androidapp://`, the real app not installed) gets that login's
  name shown, and its password only after the person picked it and passed the check — as with
  every Android password manager. Logins that ask for the master password again never show up.
- **Apple**: the extension holds only the provider key and the list, for as long as it runs, and
  opens one private key per request. A thief with the unlocked phone still needs Face ID / Touch
  ID / the passcode for each use. The passcode itself unlocks the provider key — weaker than the
  master password, which is the platform's norm for AutoFill; logins marked to ask for the master
  password again stay out of the list. That includes passkeys of organisation items: an organisation that doesn't want its passkeys behind the device passcode
  asks its members to leave the setting off (a per-organisation switch is open). Logging out or
  switching the setting off deletes the list, the key and the system's identities. Passkeys
  deleted on another device stay usable in the extension until the vault is next opened on this
  device. Another app of the same team with App Group access can't put an older list back
  (generation) and can't import an outbox entry twice; it can delete files.
- **Apple passwords**: the same seal and the same Face ID / Touch ID / passcode check as the
  passkeys; each password is opened only for the login picked. The system's identity list
  carries names and addresses only, as for passkeys.
- **Credential exchange**: the system hands the data only to the app it was exported to, and
  only after the person chose UwULock; UwULock asks again before taking it in and shows all of
  it before saving. It stays in memory until then.
- **Not covered**: a compromised OS or a malware that runs as the person and drives the UI; the
  person approving a request they didn't mean (the dialog says which site and which app).

## What is open

- Real devices: Android Credential Manager with Chrome and an app; iOS/macOS with a signed build;
  Windows 11 with the plugin API; Linux with Firefox and Chromium against the broker's key. The
  protocol parts are unit-tested (CTAP2 parsing and answers, CTAPHID framing with its
  transaction timeout, uhid events, the broker's rules, rpIds, Windows' request signatures,
  WebAuthn JSON, the seal); the broker's socket activation and sandbox are checked in CI on the
  installed `.deb`; the platform glue only builds in CI.
- CTAPHID: a message left half-sent is dropped after 3 seconds; one request at a time (a second
  one, also on the same channel, gets `CHANNEL_BUSY`; re-initialising the channel cancels the one
  in flight); the least recently used of 32 channels goes first, never the busy one.
- Windows: package identity if Windows asks for it; the cancel signature's data (above).
- Android: when Android hands over no calling app (it should for every request), passkeys are
  listed with only the rpId checked; signing still checks the caller. Passwords aren't listed
  then.
- Android passwords: whether Android shows the autofilled app to UwULock's service (package
  visibility) on every phone — without it, no Digital Asset Links, only `androidapp://` logins;
  inline (keyboard) suggestions; multi-step sign-ins fill the user name only where the app or
  page marks the field; Digital Asset Links fetched while Android waits (10 s timeout, the
  system's own is shorter).
- Apple: the iOS 18 exclude list; the credential exchange on the Mac and exporting from
  UwULock; whether `getState` answers truthfully in the sandboxed Mac app.
- Apple, untested on a device: picking UwULock's passkey right in the system's sheet (Firefox,
  Google sign-in) after the fix in 0.6.0-beta.2 — the cause of the earlier failure wasn't
  confirmed; the log above says which step fails; password AutoFill; the default-provider
  prompt; the credential exchange with Apple Passwords.
