# Passkeys outside the browser extension

The browser extension has made and used passkeys since 0.3 (docs/extension.md). From 0.5 the apps
offer the vault's passkeys to the rest of the system too: to browsers without the extension and to
other apps. Every way in ends in the same code (`apps/desktop/src-tauri/src/passkeys/`) and the
same format: Bitwarden's `fido2Credentials`, one passkey per login, so passkeys made here work in
Bitwarden's apps, the extension and the web vault, and the other way round.

| System             | How                                                  | State                 |
| ------------------ | ---------------------------------------------------- | --------------------- |
| Linux              | a virtual FIDO2 security key over `/dev/uhid`        | off until switched on |
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
- `webauthn`: WebAuthn JSON as Android's Credential Manager hands it over, client data, the
  `android:apk-key-hash:` origin, Digital Asset Links.
- `apple`: the sealed list and outbox the Apple extension reads and writes.

The app (`passkeys/mod.rs`) finds a site's passkeys in the open vault (relying party id equal, or
the passkey's credential id in the site's allow list), makes a passkey into a login (the one the
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

Silent requests (a browser checking whether a key holds a passkey, `up: false`) are answered from
the vault without the dialog, and only for an open vault.

## Linux: a virtual security key

Settings → Security → _Passkeys in browsers (security key)_. UwULock creates a HID device through
`/dev/uhid`; the kernel makes a `/dev/hidraw*` of it like a USB key's, and Firefox, Chromium and
everything with libfido2 talk CTAP2 to it. Switching off destroys the device.

Packaging: the `.deb` and `.rpm` (and so the Arch package, which repacks the `.deb`) bring

- `/usr/lib/udev/rules.d/60-uwulock-passkeys.rules`: `uaccess` on `/dev/uhid` (with
  `static_node=uhid`, so opening it loads the module) and on the key's hidraw device (bus USB,
  vendor and product 0000; systemd's `60-fido-id.rules` also recognises it by its descriptor),
- `/usr/lib/modules-load.d/uwulock-passkeys.conf`: `uhid`,
- a post-install script that loads `uhid` and reloads udev, so it works without a reboot. A new
  login may still be needed for `uaccess` to reach a running session.

**Trade-off**: `uaccess` on `/dev/uhid` lets any program of the person at the seat create HID
devices — keyboards too, which could type into other sessions on this seat. It is the same
trade Steam makes for `/dev/uinput`. A program running as that person can already do most harm
to that person's own session; what it adds is input outside it (another VT). Who doesn't want it
removes the rule; the setting then says that `/dev/uhid` can't be opened.

## Windows 11: plugin passkey manager (experimental)

Settings → Security → _Passkeys in Windows_. Windows 11 with third-party passkey manager support
(24H2/25H2 builds 26100/26200 with a recent cumulative update) lets a manager stand next to
Windows Hello through webauthn.dll's plugin API. When switched on, UwULock

- registers a COM class for the user (`HKCU\Software\Classes\CLSID\{5B0C8E7A-…}`,
  `LocalServer32 = "uwulock-desktop.exe" --passkey-plugin`) so Windows can start it,
- adds itself with `WebAuthNPluginAddAuthenticator` (AAGUID `4d0c2e23-4c15-c411-9bd1-f265e4266ad6`),
- tells Windows which passkeys there are (`WebAuthNPluginAuthenticatorAddCredentials`: ids,
  sites, names; no keys), refreshed when the vault changes,
- answers the CTAP2 requests Windows hands over like the Linux key does, in its own dialog.

The person then switches UwULock on in Windows: Settings → Accounts → Passkeys → Advanced options.
Switching off removes the authenticator, the credentials and the registry keys.

Open points: Windows signs each request with a key it gives the plugin at registration; UwULock
doesn't check that signature yet (the COM server only answers Windows' own broker, which runs as
the same user — a local program could ask too, but would still need the person's yes in the
dialog). Microsoft may require package identity (MSIX) for plugin managers in later builds; the
API is loaded at run time, so on a Windows without it the setting just says so.

## Android 14+: Credential Manager

The provider is part of the app (`crates/tauri-plugin-uwulock-mobile/android`):
`PasskeyProviderService` (a `CredentialProviderService`, enabled from API 34 via a resource bool),
`PasskeyActivity` (translucent, makes and signs) and `PasskeyBridge` (JNI into
`passkeys/android.rs` in the same process). The person picks UwULock in Android's Settings →
Passwords, passkeys & accounts.

- Listing: when UwULock runs with an open vault, the service lists the site's passkeys; otherwise
  it offers _Unlock UwULock_, which opens the app and lists them after the unlock.
- Verification: `BiometricPrompt` with a strong biometric or the screen lock before making or
  signing, unless the site says `userVerification: "discouraged"`.
- Origins: a browser on Google's list of privileged browsers (`res/raw/privileged_browsers.json`,
  release builds) hands over the web origin Android checked against the browser's certificate;
  UwULock checks that the relying party id is the origin's host or a parent domain of it, over
  HTTPS (the browser has already refused relying party ids like public suffixes). Any other app gets the `android:apk-key-hash:` origin, and only when the site's
  `https://<rp id>/.well-known/assetlinks.json` names the app's package and certificate
  (`delegate_permission/common.get_login_creds`). No redirects, 10 s, 256 KiB.
- An exclude list hit answers `InvalidStateError`.

## iOS 17+ and macOS 14+: the AutoFill extension

`apps/desktop/src-tauri/apple/PasskeyProvider` (Swift): an `ASCredentialProviderViewController`
for passkeys, the same code for both. It is its own process, started by the system while UwULock
may not run, so it never sees the open vault. Instead:

- While the vault is open and the setting is on, the app writes a **sealed list** of the account's
  passkeys (`passkeys.sealed`) into the App Group folder it shares with the extension, and on iOS
  fills the system's list of passkey identities (`ASCredentialIdentityStore`: sites, names, ids).
- The list is sealed with the **provider key**: 32 random bytes per account, AES-256-GCM
  (`0x01 ‖ nonce ‖ ciphertext ‖ tag`, AAD `uwulock-passkeys-v1`). The app keeps it sealed under
  the user key (`passkeys-apple-<account>.key`, an EncString like the rest of the vault); the
  extension gets it from the shared Keychain, `kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly`
  with `.userPresence` — Face ID, Touch ID or the device passcode on every read.
- Passkeys the extension makes go into an **outbox** (one sealed file each) and right into the
  list. The app takes the outbox into the vault as new logins at its next unlock.
- Passkeys that use a signature counter stay out of the list: the extension can't count up in
  the vault. UwULock's own passkeys don't use one.

On macOS the app writes the list from Rust (`passkeys/apple.rs`, Keychain via
`security-framework`), and the extension fills the system's identity list when it runs (and when
switched on in System Settings → Passwords → Password Options).

### Signing it (once the Apple developer account exists)

Nothing of this works unsigned: the App Group, the Keychain group and the AutoFill entitlement
come from a developer team. Until then the setting says so, and the builds are:

- **iOS**: `scripts/ios-build.sh` adds the extension as target `UwULockPasskeys`
  (`app.uwulock.passkeys`, iOS 17) to the generated Xcode project and embeds it in the app's
  PlugIns; signing is off like the app's.
- **macOS**: `scripts/macos-passkeys.sh` compiles it into `UwULockPasskeys.appex` (universal,
  ad-hoc signed) on every macOS build, so the Swift can't rot; it goes into UwULock.app only when
  `UWULOCK_APPLE_TEAM_ID` is set.

Steps with an account (team id `TEAMID`):

1. Identifiers in the developer portal: `app.uwulock` and `app.uwulock.passkeys`, both with
   _App Groups_ (`group.app.uwulock` for iOS) and the extension with _AutoFill Credential
   Provider_. On macOS the group is `TEAMID.app.uwulock`.
2. Entitlements. Extension: `apple/PasskeyProvider/UwULockPasskeys-iOS.entitlements` and
   `…-macOS.entitlements` (the script fills in `TEAMID`). App: the same App Group and
   `keychain-access-groups = TEAMID.app.uwulock.passkeys`, without the AutoFill entitlement — on
   iOS in the generated `uwulock-desktop_iOS.entitlements`, on macOS through Tauri's
   `bundle.macOS.entitlements`.
3. iOS: drop the `CODE_SIGNING_ALLOWED: NO` lines from `scripts/ios-build.sh` for both targets
   and set `DEVELOPMENT_TEAM`. `Info.ios.plist` already carries `UwULockAppGroup` and
   `UwULockKeychainGroup` (`$(AppIdentifierPrefix)` is filled in by Xcode).
4. macOS: build with `UWULOCK_APPLE_TEAM_ID=TEAMID` (Rust reads it at build time for the group
   paths) and `APPLE_SIGNING_IDENTITY="Developer ID Application: …"`; the script signs the
   extension with its entitlements and the hardened runtime before Tauri signs the app.
5. Turn it on: iOS Settings → General → AutoFill & Passwords, macOS System Settings → General →
   AutoFill & Passwords; then UwULock's own setting.

## Threat model

What is protected: the private keys of passkeys, which are as good as the account they sign in to.

- **At rest**: no private key is ever on disk in the clear. The vault cache holds them encrypted
  as before; the Apple list and outbox are sealed with the provider key, which is itself either
  sealed under the user key (only while unlocked) or in the Keychain behind user presence,
  this device only, never synced or backed up.
- **Linux and Windows**: the keys stay in the UwULock process. A request reaches them only
  through the person's yes in UwULock's window, for an open vault. Another program of the same
  user can send requests (as it could to a USB key) and could watch the screen, but can't get a
  signature without the person agreeing to that site's request. The dialog shows the relying
  party; on Linux the browser decides which site may ask for which relying party, as with any
  security key. The silent check (`up: false`) tells a program of the same user whether the
  vault has a passkey for a given site and credential id.
- **Android**: the provider runs in UwULock's process; Android binds the service only with
  `BIND_CREDENTIAL_PROVIDER_SERVICE`, and the activity isn't exported. The origin comes from
  Android (privileged browsers) or Digital Asset Links, so an app can't sign in to a site that
  doesn't trust it. Each passkey use needs the screen lock or a strong biometric unless the site
  says verification is discouraged.
- **Apple**: the extension holds only the provider key and the list, for as long as it runs. A
  thief with the unlocked phone still needs Face ID / Touch ID / the passcode for each use. The
  passcode itself unlocks the provider key — weaker than the master password, which is the
  platform's norm for AutoFill. Switching the setting off deletes the list, the key and the
  system's identities.
- **Not covered**: a compromised OS or a malware that runs as the person and drives the UI; the
  person approving a request they didn't mean (the dialog says which site and which app).

## What is open

- Real devices: Android Credential Manager with Chrome and an app; iOS/macOS with a signed build;
  Windows 11 with the plugin API; Linux with Firefox and Chromium against `/dev/uhid`. The
  protocol parts are unit-tested (CTAP2 parsing and answers, CTAPHID framing, uhid events,
  WebAuthn JSON, the seal), the platform glue only builds in CI.
- Windows: checking the request signature; package identity if Windows asks for it.
- Apple: the iOS 18 exclude list; passwords in the extension (only passkeys for now).
