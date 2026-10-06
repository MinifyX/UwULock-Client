# UwULock on Android and iPhone

[Deutsch weiter unten](#uwulock-auf-android-und-iphone)

The phone app is the desktop app: the same Rust core, the same React UI from
`apps/desktop`, built by Tauri 2 for Android and iOS. On a phone it shows one
pane at a time — the list, an item, the editor — with the folders in a drawer
and dialogs filling the screen. Vault, search, copying, one-time codes, the
generator and the password check work as on the desktop; its review goes card
by card with a swipe ([uwu-extras.md](uwu-extras.md#password-check)).

| Phone                       | File under **Assets**                  |
| --------------------------- | -------------------------------------- |
| Android 10 or newer (arm64) | `UwULock-android.apk`                  |
| iPhone with iOS 17 or newer | `UwULock-ios.ipa` (unsigned, sideload) |

**Not in this release:** UwULock doesn't fill logins into other apps yet — no
Android autofill service, no iOS password provider. Copy and paste works; filling
comes later ([roadmap](roadmap.md)).

## Installing

**Android.** Open `UwULock-android.apk` on the phone and allow installing from
the browser or file manager it came from. A new version installs over the old
one the same way; your login stays. Every released APK is signed with the same
key, SHA-256 certificate fingerprint

```
fd:63:23:ae:a8:32:99:6e:d5:6a:55:d1:1c:fc:d2:6a:61:ec:b1:59:b3:f9:b9:80:f1:42:9b:5d:e6:50:0a:1c
```

(`apksigner verify --print-certs UwULock-android.apk` shows it). Builds of
unreleased commits from CI carry a different key and don't install over a
released UwULock.

**iPhone.** UwULock has no Apple developer account, so the IPA is unsigned and
doesn't come from the App Store. A sideloading tool such as
[AltStore](https://altstore.io) or [Sideloadly](https://sideloadly.io) signs it
with your own Apple ID on the way to the phone. With a free Apple ID the app has
to be signed again every 7 days (AltStore does that by itself while the phone is
on the same network). Then allow it under **Settings → General → VPN & Device
Management**.

## What's different on a phone

- **Unlocking with the fingerprint or face.** Settings → Security →
  **Unlock with fingerprint** (Face ID / Touch ID on the iPhone). It works like
  Windows Hello on the desktop: 32 random bytes, kept by the phone behind the
  biometric check, are stretched into a key that seals a copy of the user key
  in `account.json` (prefix `m1:`). On Android the bytes are sealed with an AES
  key in the Android Keystore that only works after a strong biometric check
  and dies when a finger is added; on the iPhone they are a Keychain item with
  `biometryCurrentSet` and `WhenPasscodeSetThisDeviceOnly`. A new fingerprint or
  face makes the copy unusable: UwULock asks for the master password once and
  switching it on again makes a new key.
- **Locking.** The lock timeout from the settings applies as everywhere, by
  both clocks (time asleep counts) and checked the moment UwULock comes back;
  on top of that, UwULock locks when it has been in the background for more
  than a minute (both clocks, so a phone asleep in between counts).
- **No screenshots, no recent-apps preview** on Android (`FLAG_SECURE`); on the
  iPhone the app switcher shows an empty screen instead of the vault. Neither
  Android's backup nor iCloud or computer backups of the iPhone take UwULock's
  data folder.
- **Clipboard.** Copied passwords are marked as sensitive (Android 13 hides
  them in the clipboard preview; iOS keeps them on the device, not in the
  universal clipboard) and are cleared after the set time if they're still
  UwULock's.
- **Back button.** Android's back closes the top dialog, the item on screen or
  the drawer; with nothing open it puts UwULock in the background, it doesn't
  quit.
- **Updates** come as a new file from the releases page; the in-app updater
  and the tray are desktop only.
- **Certificates.** UwULock trusts the usual public certificate authorities on
  phones (built in, `webpki-roots`), not certificates you added to the phone
  yourself. A server with its own CA needs a certificate from a public one (or
  pinning, as UwULock Server offers).
- **File requests** save into Downloads on Android and into UwULock's folder in
  the Files app on the iPhone.
- **Wi-Fi networks** have a **Connect** button on Android: Android 11+ asks in
  its own sheet whether to add the network, Android 10 suggests it. Not on the
  iPhone (see [The phone plugin](#the-phone-plugin)); there the QR code and
  copying remain.

## Building

Requirements on top of the desktop ones: for Android JDK 21, the Android SDK
(platform 36, build tools) and NDK 28, with `JAVA_HOME`, `ANDROID_HOME` and
`NDK_HOME` set, and `rustup target add aarch64-linux-android`; for iOS a Mac
with Xcode, `xcodegen` and `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`.

```bash
pnpm mobile:prepare          # once: the app library also as cdylib/staticlib
pnpm tauri android build --apk --target aarch64
pnpm tauri android dev       # on a connected phone or an emulator
pnpm tauri ios init && pnpm tauri ios dev
```

`pnpm mobile:prepare` changes `crate-type` in
`apps/desktop/src-tauri/Cargo.toml` in the working tree only; desktop builds
and tests stay faster without it. Don't commit it
(`git checkout apps/desktop/src-tauri/Cargo.toml`).

`apps/desktop/src-tauri/gen/android` is in the repository (MainActivity, theme,
icons, signing in `app/build.gradle.kts`). The Xcode project isn't:
`tauri ios init` writes it fresh, and `scripts/ios-build.sh` switches signing
off and merges `Info.ios.plist` into it.

## CI and releases

- `.github/workflows/android.yml` builds the APK without any key, signs it in a
  separate job (release key only for a `v*` tag, otherwise the CI key; the
  release certificate's fingerprint above is checked in both directions), and
  starts it in an emulator (`scripts/android-smoke.sh`).
- `.github/workflows/ios.yml` builds for the simulator and starts that build
  (`scripts/ios-smoke.sh`), and in a second job side by side builds for the
  iPhone and packs the unsigned IPA (artifact `UwULock-iOS-<sha>`). Pull
  requests only get the simulator job.
- Both run for pull requests that touch the app, for main and for tags.
  `pnpm release` takes the APK and the IPA of the tag's runs, checks the APK's
  certificate (`scripts/apk-cert.mjs`) and publishes them as
  `UwULock-android.apk` and `UwULock-ios.ipa`.

Signing secrets (repository secrets of MinifyX/UwULock-Client):
`UWULOCK_ANDROID_KEYSTORE_BASE64` / `UWULOCK_ANDROID_KEYSTORE_PASSWORD` (the
release key, PKCS#12, alias `uwulock`) and `UWULOCK_ANDROID_CI_KEYSTORE_BASE64` /
`UWULOCK_ANDROID_CI_KEYSTORE_PASSWORD`. The release keystore is kept offline as
well; losing it means a new key, and every phone would have to uninstall
UwULock once to switch.

## The phone plugin

Everything that needs Android's or iOS's own APIs lives in one local Tauri
plugin, `crates/tauri-plugin-uwulock-mobile`:

| Part                                           | What it is                                                  |
| ---------------------------------------------- | ----------------------------------------------------------- |
| `src/lib.rs`                                   | The Rust side: `Mobile<R>`, one method per command          |
| `android/src/main/java/UwuLockMobilePlugin.kt` | Kotlin, `@Command` functions (package `app.uwulock.mobile`) |
| `ios/Sources/UwuLockMobilePlugin.swift`        | Swift, `@objc` functions                                    |

Commands today: `unlockStatus`, `unlockCreate`, `unlockOpen`, `unlockDelete`
(biometric unlock), `copySecret`, `clearClipboard`, `setAppearance` (system bar
colours), `saveToDownloads`, `connectWifi`, `openWifiSettings` (Android), and for the AutoFill
extension on iOS `passkeysStatus`, `passkeysStore`, `passkeysOutbox`, `passkeysClearOutbox`,
`passkeysClear` (`ios/Sources/Passkeys.swift`). Android's passkey provider isn't a command: the
system starts `PasskeyProviderService` and `PasskeyActivity`, which call into Rust through JNI
(`PasskeyBridge`) — see [passkeys.md](passkeys.md). The app reaches the plugin through
`phone::plugin()` in `apps/desktop/src-tauri/src/phone.rs`. Every call waits for
the phone's answer, so it must never run on the main thread — spawn a thread
(`std::thread::spawn`) or use `tauri::async_runtime::spawn_blocking`, as
`hello.rs` and `clipboard.rs` do.

**Adding a feature** (for example the swipe check or joining a WiFi network):

1. A method on `Mobile<R>` in `src/lib.rs` calling
   `self.0.run_mobile_plugin("yourCommand", args)`, with `#[derive(Serialize)]`
   args in camelCase.
2. `@Command fun yourCommand(invoke: Invoke)` in the Kotlin file with an
   `@InvokeArg class YourCommandArgs`, and `@objc public func yourCommand(_ invoke: Invoke)`
   in the Swift file. Answer with `invoke.resolve(...)`, fail with
   `invoke.reject(message, code)` — codes the Rust side maps (`Error::is`).
3. Android permissions go into the plugin's `android/src/main/AndroidManifest.xml`,
   iOS usage strings into `apps/desktop/src-tauri/Info.ios.plist`.
4. A Tauri command in the app (`cfg(mobile)`, with a desktop stub when the UI
   calls it everywhere), and in the UI behind `isMobile()` from
   `apps/desktop/src/lib/platform.ts`.

**Wi-Fi "Connect"** (`wifi.rs` in the app, `connectWifi` and
`openWifiSettings` in the plugin; what the person sees is in
[wifi.md](wifi.md#connecting-on-the-phone)). Android 11+ gets the network
through `Settings.ACTION_WIFI_ADD_NETWORKS`, the system's own sheet the person
confirms in — addressed to that sheet's component, found among system apps
only (`MATCH_SYSTEM_ONLY`, a `<queries>` entry in the plugin's manifest), so no
other app that registers for the action can receive the password; without
such a sheet the network is suggested instead. Android 10 gets a
`WifiNetworkSuggestion`. Both are built in
Kotlin from what `wifi::network` made of the item — the password goes from
Rust to Kotlin and never through the page. The only permission is
`CHANGE_WIFI_STATE` (a normal one, granted at install, needed for the
suggestion); no location, no `ACCESS_WIFI_STATE`, so UwULock can't see which
network the phone is on and doesn't claim to have connected.

iOS has no _Connect_ button. Its API, `NEHotspotConfiguration`, works only with
the _Hotspot Configuration_ entitlement in the app's provisioning profile. The
IPA is built unsigned and gets its signature from whoever sideloads it — with
a free Apple ID (AltStore, Sideloadly) the profile can't carry that
entitlement, and without it iOS rejects every call. A paid developer account
could re-sign with it, but UwULock can't rely on that, so the button stays
hidden on iOS; the QR code and copying the password remain (iOS joins a
network from the camera's QR scan).
In the UI the button goes into the Wi-Fi details' slot, `ItemDetail`'s
`wifiActions`, which `VaultScreen` fills only when `platform()` is `android`.

---

# UwULock auf Android und iPhone

Die Handy-App ist die Desktop-App: derselbe Rust-Kern, dieselbe Oberfläche aus
`apps/desktop`, von Tauri 2 für Android und iOS gebaut. Auf dem Handy zeigt sie
eine Ansicht auf einmal — Liste, Eintrag, Bearbeiten —, die Ordner in einer
Seitenleiste zum Aufziehen, Dialoge über den ganzen Bildschirm. Tresor, Suche,
Kopieren, Einmalcodes, der Generator und die Passwortprüfung funktionieren wie
am Computer; ihr Durchgehen läuft Karte für Karte per Wischen.

| Handy                         | Datei unter **Assets**                   |
| ----------------------------- | ---------------------------------------- |
| Android 10 oder neuer (arm64) | `UwULock-android.apk`                    |
| iPhone mit iOS 17 oder neuer  | `UwULock-ios.ipa` (unsigniert, Sideload) |

**Noch nicht dabei:** UwULock füllt Logins noch nicht in anderen Apps aus —
kein Android-Autofill, kein iOS-Passwortanbieter. Kopieren und Einfügen geht;
das Ausfüllen kommt später.

## Installieren

**Android.** `UwULock-android.apk` auf dem Handy öffnen und die Installation aus
dem Browser oder Dateimanager erlauben, aus dem sie kommt. Neue Versionen
genauso darüber installieren; die Anmeldung bleibt. Jede veröffentlichte APK ist
mit demselben Schlüssel signiert (SHA-256-Fingerabdruck oben).

**iPhone.** UwULock hat kein Apple-Entwicklerkonto, die IPA ist also unsigniert
und nicht aus dem App Store. Ein Sideloading-Werkzeug wie AltStore oder
Sideloadly signiert sie auf dem Weg aufs Handy mit deiner eigenen Apple-ID. Mit
einer kostenlosen Apple-ID muss die App alle 7 Tage neu signiert werden
(AltStore macht das selbst, solange das Handy im selben Netz ist). Danach unter
**Einstellungen → Allgemein → VPN und Geräteverwaltung** erlauben.

## Was auf dem Handy anders ist

- **Entsperren mit Fingerabdruck oder Gesicht** (Einstellungen → Sicherheit).
  Ein neuer Finger oder ein neues Gesicht macht die Kopie unbrauchbar: dann
  einmal mit dem Master-Passwort entsperren und neu einschalten.
- **Sperren:** zusätzlich zur eingestellten Zeit sperrt UwULock, wenn es länger
  als eine Minute im Hintergrund war.
- **Keine Bildschirmfotos** und keine Vorschau in den letzten Apps (Android).
- **Kopierte Passwörter** gelten als vertraulich und verschwinden nach der
  eingestellten Zeit.
- **Zurück** schließt den obersten Dialog, den Eintrag oder die Seitenleiste;
  ohne etwas Offenes geht UwULock in den Hintergrund.
- **Updates** kommen als neue Datei von der Release-Seite.
- **Zertifikate:** UwULock vertraut auf dem Handy den üblichen öffentlichen
  Zertifizierungsstellen, nicht selbst hinzugefügten.
- **WLAN verbinden** (nur Android): _Verbinden_ in einem WLAN-Eintrag übergibt das
  Netz an Android – ab Android 11 bestätigst du es im Fenster des Systems,
  Android 10 schlägt es vor. Dafür braucht UwULock nur die Berechtigung
  `CHANGE_WIFI_STATE`, keinen Standort. Auf dem iPhone gibt es den Knopf nicht:
  iOS erlaubt das nur Apps mit einer Berechtigung (_Hotspot Configuration_), die
  eine selbst signierte App mit kostenloser Apple-ID nicht bekommt. QR-Code und
  Kopieren gehen überall.
