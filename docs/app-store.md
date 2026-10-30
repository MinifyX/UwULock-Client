# UwULock in the App Store (TestFlight)

[Deutsch weiter unten](#uwulock-im-app-store-testflight)

UwULock comes to Apple's systems as one native app (apps/apple, SwiftUI over the Rust core;
[native-apple-plan.md](native-apple-plan.md)), built three ways from the same Xcode project. The
**GitHub builds**: the disk image for the Mac (updates itself with Sparkle) and the unsigned IPA
for sideloading on the iPhone ([mobile.md](mobile.md)). The **App Store builds**: signed by the
developer team and uploaded to App Store Connect, one app for iPhone, iPad and Mac (bundle ID
`app.uwulock`, universal purchase). For now they go to **TestFlight only**: nothing has been
submitted for review, and nothing in CI ever submits.

## What is different between the builds

|                  | iPhone/iPad: sideload IPA     | iPhone/iPad: TestFlight                            | Mac: DMG (GitHub)                                   | Mac: App Store                                  |
| ---------------- | ----------------------------- | -------------------------------------------------- | --------------------------------------------------- | ----------------------------------------------- |
| Scheme / config  | `UwULock` / Release           | `UwULock` / Release                                | `UwULock Direct` / Release-Direct                   | `UwULock` / Release                             |
| Bundle ID        | `app.uwulock`                 | `app.uwulock`                                      | `app.uwulock.desktop`                               | `app.uwulock`                                   |
| Signed by        | whoever sideloads it          | Apple Distribution, App Store profile              | Developer ID + Developer ID profiles, notarized     | Apple Distribution + Mac Installer Distribution |
| Passkeys         | extension inside, can't work  | AutoFill extension with App Group + Keychain group | AutoFill extension (needs the Developer ID)         | AutoFill extension, sandboxed                   |
| Updates          | new IPA from the release page | TestFlight / App Store                             | Sparkle (`appcast-stable.xml` / `appcast-beta.xml`) | the store; Sparkle not linked                   |
| Sandbox          | iOS's                         | iOS's                                              | no, hardened runtime                                | yes (`Config/UwULock-macOS.entitlements`)       |
| Architectures    | arm64                         | arm64                                              | universal (Apple silicon + Intel)                   | universal                                       |
| Privacy manifest | `PrivacyInfo.xcprivacy`       | same                                               | same                                                | same                                            |

The TestFlight IPA and the sideload IPA come from the same archive: one build for both. The disk
image without a Developer ID in CI is ad-hoc signed instead (`apple-release.sh adhoc`): it starts,
but without AutoFill, the App Group and the shared Keychain groups, which only a profile allows.

## How CI builds, signs and uploads

Everything Apple is in `.github/workflows/apple.yml` (Xcode 27, `macos-27` runners), driven by
`scripts/apple-release.sh`:

| Job         | Runs on                | What it does                                                                                                                                                                                                                        |
| ----------- | ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `core`      | PRs, `main`, `v*` tags | the Rust core as xcframework (`scripts/apple-xcframework.sh --universal`), cached on `Cargo.lock`, `crates/**` and the script; built only on a cache miss                                                                           |
| `build`     | PRs, `main`            | xcodegen, then unsigned iOS Simulator, macOS (with `UwULockPasskeysTests`) and Direct builds side by side (`scripts/apple-build.sh`)                                                                                                |
| `archive`   | `v*` tags              | `apple-release.sh archive` per product (`ios`, `mas`, `direct`), unsigned, then ad-hoc signed with each bundle's entitlements; holds no secret. The `ios` leg also packs the sideload IPA (artifact `apple-ios`)                    |
| `sign`      | `v*` tags              | on a fresh runner that builds nothing: keychain, profiles (`scripts/asc.mjs profiles`), `xcodebuild -exportArchive`; `ios`/`mas` upload to TestFlight, `direct` is notarized, packed into the DMG and the DMG notarized and stapled |
| `migration` | `v*` tags              | the universal setup that replaces the old Tauri app with the native one (`scripts/build-setup.mjs --payload`, artifact `apple-migration`)                                                                                           |

The signing jobs use a keychain of their own that is deleted at the end (`apple-release.sh
cleanup`). Without the store secrets the `ios`/`mas` legs are skipped; without the Developer ID
the disk image is ad-hoc signed. `scripts/release.mjs` (`pnpm release`) then takes the disk
image, the IPA and the migration setup from the tag's run, signs the disk image for Sparkle and
the migration setup for the Tauri updater, and publishes.

- **Version**: App Store Connect takes three numbers, so `0.8.0-beta.1` goes up as `0.8.0`; the
  disk image keeps the full version (`CFBundleShortVersionString`).
- **Build number** (`CFBundleVersion`): `<UTC date>.<run number>.<attempt>` of the tag's run, the
  same for all three products. It grows with every run and every re-run, as App Store Connect and
  Sparkle (`sparkle:version`) want.
- **Profiles**: `scripts/asc.mjs profiles <ios|macos|direct>` asks the App Store Connect API for
  `UwULock <iOS App Store|macOS App Store|macOS Developer ID> <bundle id>`, made for the
  certificate in the secret. A missing or invalid one (a capability changed, the certificate was
  renewed) is made anew in the same run. The Safari extension's identifiers
  (`app.uwulock.safari`, `app.uwulock.desktop.safari`) are registered on the way.
- **Developer ID identifiers**: `app.uwulock.desktop` and `app.uwulock.desktop.passkeys` need
  the AutoFill Credential Provider and App Groups capabilities (group
  `TEAMID.app.uwulock`, as in the store app); register them once by hand in the developer portal
  (Identifiers → +, App IDs), `asc.mjs` won't set capabilities.
- **Seeing the builds**: `node scripts/asc.mjs builds` lists the newest builds with their
  processing state (with `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_PATH` set). Processing takes
  5–30 minutes after the upload; Apple mails problems with a build.

Trying it on a Mac at hand: `UWULOCK_VERSION=0.8.0 BUILD_NUMBER=1 APPLE_TEAM_ID=… scripts/apple-release.sh
archive direct /path/direct.xcarchive`, then `adhoc` on a copy of the app and `dmg`
(`UWULOCK_SKIP_NOTARIZE=1` skips notarizing).

**Safari extension**: all three Mac/iOS apps carry it (`PlugIns/UwULockSafari.appex`,
docs/extension.md), built from the tag's `extension-safari` artifact (`extension.yml`). It needs no
App Group or keychain group.

### Sparkle (the disk image's updates)

The Direct build carries `SUFeedURL` and `SUPublicEDKey` from `apps/apple/Config/Direct.xcconfig`
(public key `7/6Acto3RlDtF9AS1XBDGVM6oMtCYPTySpFX+u6TZAI=`). The **private key** never goes to
CI: it lives in the **login keychain** of the release Mac (Sparkle's `generate_keys`, account
`app.uwulock`), and `pnpm release` signs the disk image there (`scripts/sparkle.mjs`, through
Sparkle's `sign_update`; macOS asks once to allow it), the way it signs the Tauri setups with
their key. It checks every signature against the public key in `Direct.xcconfig` before
publishing and writes `appcast-stable.xml` (stable releases) and `appcast-beta.xml` (every
release) to the `updates` branch, with `minimumSystemVersion` 27.0 and the notes in German and
English.

- **Backup / another Mac**: `generate_keys --account app.uwulock -x sparkle-key.txt` exports it
  (base64); `generate_keys --account app.uwulock -f sparkle-key.txt` imports it. Keep the file
  offline. A lost key means no installed app accepts an update anymore.
- **As a secret** (only if signing should ever move to CI; nothing uses it now): the exported
  text as `SPARKLE_PRIVATE_KEY`. `scripts/sparkle.mjs` also takes it from that variable or from
  `SPARKLE_KEY_FILE`.
- `node scripts/sparkle.mjs public-key` shows which key is in use and whether it matches.

### The way over from the Tauri app

The Tauri app installed on a Mac reads `stable.json` / `beta.json`. Their `darwin-aarch64` and
`darwin-x86_64` entries point to `UwULock-update-macos-universal`, the migration setup, signed
with the Tauri key as before. It puts the native app (the Developer ID build from the same run)
in the old app's place; from then on Sparkle updates it. Both use the same bundle ID
(`app.uwulock.desktop`) and the same data folder and format, so the login, the vault copy and the
biometric keys carry over (docs/install.md).

### Secrets (MinifyX/UwULock-Client)

| Secret                    | Content                                                                                       |
| ------------------------- | --------------------------------------------------------------------------------------------- |
| `APPLE_TEAM_ID`           | `7N8YX2CL7J`                                                                                  |
| `APPLE_ASC_KEY_ID`        | App Store Connect API key ID (team key, role App Manager)                                     |
| `APPLE_ASC_ISSUER_ID`     | its issuer ID                                                                                 |
| `APPLE_ASC_KEY_P8`        | the key itself (`AuthKey_….p8`, PEM)                                                          |
| `APPLE_DISTRIBUTION_P12`  | base64 of a .p12 with the **Apple Distribution** certificate and its key (iOS and Mac)        |
| `APPLE_INSTALLER_P12`     | base64 of a .p12 with the **Mac Installer Distribution** certificate and its key (the `.pkg`) |
| `APPLE_P12_PASSWORD`      | the password of both .p12                                                                     |
| `APPLE_DEVELOPER_ID_KEY`  | the disk image's signing key (PEM, `apple.yml` job `sign`)                                    |
| `APPLE_DEVELOPER_ID_CERT` | its **Developer ID Application** certificate (PEM or base64 DER)                              |
| `SPARKLE_PRIVATE_KEY`     | optional, unused: the Sparkle key, only if signing moves to CI (see above)                    |

Both certificates were made through the App Store Connect API from a key that never left Lorin's
machine (a copy is kept offline). They run out on **2027-10-06**. To renew: a new key and CSR
(`openssl req -new -key …`), `POST /v1/certificates` with type `DISTRIBUTION` and
`MAC_INSTALLER_DISTRIBUTION` (or the developer portal), new .p12s into the secrets. The profiles
follow by themselves on the next run.

## What is left to click in App Store Connect

1. **TestFlight → Internal Testing**: make a group (e.g. "Ich"), add yourself, tick
   "Automatically distribute builds" — every processed build then shows up in the TestFlight app on
   iPhone, iPad and Mac.
2. **Export compliance** per build ("Missing Compliance" until answered). UwULock brings its own
   standard encryption (AES-256, HMAC-SHA256, RSA-OAEP, PBKDF2/Argon2 — Bitwarden's scheme) on top
   of HTTPS, and uses it only to protect the user's own data and to sign in. That is the case the
   U.S. export rules exempt (Category 5 Part 2, data protection / authentication), and password
   managers in the store declare it so. Answer, when asked "What type of encryption algorithms does
   your app implement?": **Standard encryption algorithms instead of, or in addition to, using or
   accessing the encryption within Apple's operating system**, then follow the questions; for
   France, App Store Connect asks whether the app is distributed there. Once the answer is settled,
   `ITSAppUsesNonExemptEncryption = false` in the app's Info.plist stops the
   question per build — left out on purpose until then.
3. **macOS on the app record**: nothing to do. The record has an iOS and a macOS version, and
   the first Mac build was accepted.

## Later: publishing (not done now)

- **Store page** per platform (iOS and macOS versions in App Store Connect): name "UwULock",
  subtitle, description, keywords (100 characters), support URL (GitHub issues), marketing URL
  (uwu.minifyx.de), copyright "© 2026 MinifyX", category Utilities (secondary Productivity).
- **Privacy**: privacy policy URL ([PRIVACY.md](../PRIVACY.md) on GitHub or the website); App
  Privacy → "Do you or your partners collect data?" → **No** → "Data Not Collected": the vault goes
  only to the user's own server, end-to-end encrypted (matches `PrivacyInfo.xcprivacy`).
- **Age rating**: every answer "None"/"No" → 4+. **Price**: free. **Content rights**: no
  third-party content.
- **Review notes and demo account**: App Review needs a server to sign in to — a UwULock Server
  (or Vaultwarden) on a public address with a demo account and a few items, passkeys and one-time
  codes, plus the steps: "Choose 'Self-hosted', server https://…, e-mail …, master password …".
  Mention that UwULock is a client for self-hosted Bitwarden-compatible servers and that the
  AutoFill extension is switched on under Settings → General → AutoFill & Passwords.
- **Screenshots** (PNG/JPEG, no transparency): iPhone 6.9" 1320×2868 (or 6.5" 1284×2778); iPad
  13" 2064×2752 (or 2048×2732) — needed because the app runs on iPad; Mac 16:10, one of 1280×800,
  1440×900, 2560×1600, 2880×1800.
- **iPhone text**: the update row in the settings must not point to the release page in a store
  build (App Review, guideline 2.5.2/2.3): tell the sideload build apart at run time before the
  first review.
- **Guidelines to keep in mind**: 2.5.2 (no downloading code — the extension packs nothing), 3.1
  (no purchases), 4.8 (no third-party sign-in), 5.1.1 (privacy policy, account deletion: the account
  lives on the user's own server; say so in the review notes).

## Not tried yet

The native builds were archived and the disk image packed ad-hoc on a Mac (milestone 7), but no
signed native build has gone through CI, TestFlight or notarization yet. On the first tag:

- iPhone/iPad: sign in, unlock with Face ID, turn on UwULock under AutoFill & Passwords and
  sign in with a passkey in Safari (App Group and Keychain group work);
- Mac: the window comes up in the sandbox, signing in reaches the server, a file request lands
  in Downloads, a Bitwarden export can be moved in, the AutoFill extension shows up under
  System Settings → General → AutoFill & Passwords and offers passkeys;
- no update row with a channel on the Mac, no update check in Console.

---

# UwULock im App Store (TestFlight)

UwULock kommt auf Apples Systeme als eine native App (apps/apple), aus einem Xcode-Projekt
dreifach gebaut: **GitHub-Builds** sind das DMG für den Mac (aktualisiert sich per Sparkle) und
die unsignierte IPA zum Sideloaden; die **App-Store-Builds** (`app.uwulock`, iPhone, iPad und Mac)
landen vorerst **nur in TestFlight**. Zur Prüfung eingereicht ist nichts, und CI reicht nie etwas
ein.

- **CI**: `.github/workflows/apple.yml` (Xcode 27). Bei PRs und auf `main`: xcframework (gecacht),
  xcodegen, unsignierte Builds für iOS-Simulator, macOS und Direct, `UwULockPasskeysTests`. Bei
  `v*`-Tags: `archive` (ohne Secrets), `sign` (signiert per `exportArchive` auf frischem Runner,
  lädt IPA und `.pkg` zu TestFlight, notarisiert App und DMG) und `migration` (Setup, das die alte
  Tauri-App durch die native ersetzt). Alles über `scripts/apple-release.sh`.
- **Version** `0.8.0-beta.1` → `0.8.0` für die Stores, das DMG behält sie ganz; **Build-Nummer**
  = `<Datum>.<Laufnummer>.<Versuch>`, für alle drei Produkte gleich.
- **Sparkle-Schlüssel**: privat nur im Schlüsselbund des Release-Macs (Konto `app.uwulock`);
  `pnpm release` signiert das DMG lokal und schreibt `appcast-stable.xml` / `appcast-beta.xml`.
  Export: `generate_keys --account app.uwulock -x datei`; als Secret `SPARKLE_PRIVATE_KEY` nur,
  falls das Signieren je nach CI wandert.
- **Developer-ID-Bundle-IDs** `app.uwulock.desktop` und `.desktop.passkeys` einmal von Hand mit
  AutoFill Credential Provider und App Groups registrieren; die Profile macht `scripts/asc.mjs`.
- **Profile** holt bzw. erneuert `scripts/asc.mjs` über die App-Store-Connect-API; die
  Zertifikate (gültig bis 2027-10-06) liegen als Secrets im Repo, Erneuern siehe oben.

**Noch zu klicken in App Store Connect:**

1. **TestFlight → Interne Tests**: Gruppe anlegen (z. B. „Ich“), dich hinzufügen, „Builds
   automatisch verteilen“ anhaken. Dann erscheint jeder fertig verarbeitete Build in der
   TestFlight-App auf iPhone, iPad und Mac.
2. **Exportbestimmungen** pro Build („Fehlende Konformität“): UwULock nutzt eigene
   Standard-Verschlüsselung (AES-256, HMAC, RSA, PBKDF2/Argon2) zusätzlich zu der von Apple,
   nur zum Schutz der eigenen Daten und zur Anmeldung. Antwort: **„Standard-Verschlüsselungs­algorithmen
   anstelle von oder zusätzlich zu der Verschlüsselung in Apples Betriebssystem“**, dann den
   Fragen folgen. Steht die Antwort fest, kann `ITSAppUsesNonExemptEncryption = false` ins
   Info.plist, damit nicht mehr pro Build gefragt wird.
3. **macOS-Plattform** am App-Eintrag: erledigt, der Eintrag hat iOS und macOS.

**Später, zum Veröffentlichen** (nicht jetzt): Store-Seite je Plattform, Datenschutz-URL und
„Keine Daten erfasst“, Altersfreigabe 4+, Demo-Konto auf einem öffentlichen Server für App
Review mit Prüfhinweisen, Screenshots (iPhone 6,9" 1320×2868, iPad 13" 2064×2752, Mac
2880×1800), und auf dem iPhone darf die Update-Zeile im Store-Build nicht auf die Release-Seite
zeigen.
