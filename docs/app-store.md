# UwULock in the App Store (TestFlight)

[Deutsch weiter unten](#uwulock-im-app-store-testflight)

UwULock comes in two forms on Apple's systems. The **GitHub builds** stay what they were: the
DMG for the Mac (updates itself) and the unsigned IPA for sideloading on the iPhone
([mobile.md](mobile.md)). The **App Store builds** are the same app, signed by the developer
team and uploaded to App Store Connect, one app for iPhone, iPad and Mac (bundle ID
`app.uwulock`, universal purchase). For now they go to **TestFlight only**: nothing has been
submitted for review, and nothing in CI ever submits.

## What is different in the store builds

|                  | iPhone/iPad: sideload IPA     | iPhone/iPad: TestFlight                            | Mac: DMG (GitHub)                   | Mac: App Store                                  |
| ---------------- | ----------------------------- | -------------------------------------------------- | ----------------------------------- | ----------------------------------------------- |
| Bundle ID        | `app.uwulock`                 | `app.uwulock`                                      | `app.uwulock.desktop`               | `app.uwulock`                                   |
| Signed by        | whoever sideloads it          | Apple Distribution, App Store profile              | ad hoc (no Developer ID)            | Apple Distribution + Mac Installer Distribution |
| Passkeys         | extension inside, can't work  | AutoFill extension with App Group + Keychain group | extension left out                  | AutoFill extension (macOS 14+), sandboxed       |
| Updates          | new IPA from the release page | TestFlight / App Store                             | own updater (feature `self-update`) | the store; no updater compiled in               |
| Cargo features   | default                       | default (the same build as the sideload IPA)       | default (`self-update`)             | `--no-default-features --features store`        |
| Sandbox          | iOS's                         | iOS's                                              | no                                  | yes, `macos/Entitlements.mas.plist`             |
| Privacy manifest | `apple/PrivacyInfo.xcprivacy` | same                                               | —                                   | same, in `Contents/Resources`                   |

The TestFlight IPA is the sideload IPA of the same run, signed afterwards: one build for both.
That is also why the iPhone's settings still say new versions come from the release page; the
Mac App Store build knows it is one (`distribution` command, feature `store`) and says the store
updates it.

### The Mac App Store build in the sandbox

- **No updater, no setup**: `tauri-plugin-updater`, the feed and the setup hand-over are not
  compiled in (`#[cfg(self_update)]`, set by `build.rs` from the feature). `scripts/build-mas.mjs`
  fails if the feed's address is still in the program.
- **Entitlements** (`macos/Entitlements.mas.plist`): network client (the server, icons, live
  updates), user-selected files read/write (moving a vault in, importing a key), Downloads
  read/write (file requests, saved keys go there as in every build), the App Group
  `TEAMID.app.uwulock`, the Keychain groups `TEAMID.app.uwulock` + `TEAMID.app.uwulock.passkeys`,
  and the AutoFill entitlement (App Store Connect wants it on the extension's container too,
  ITMS-90729; the App ID `app.uwulock` has the capability for that).
  Nothing listens, nothing starts other programs.
- **Data** lives in the container, `~/Library/Containers/app.uwulock/Data/Library/Application
Support/app.uwulock`, not where the DMG keeps it. Switching between the two means signing in
  again.
- **Passkeys**: the App Group folder is found through the real home folder (`HOME` is the
  container in the sandbox, `passkeys/apple.rs`). The extension `app.uwulock.passkeys` now sits
  under an app whose ID it extends; in the DMG (`app.uwulock.desktop`) it doesn't, which is one
  more reason the DMG leaves it out.
- Nothing else needed a change: no tray, no autostart, the Linux uhid broker isn't built for
  macOS, the clipboard (NSPasteboard), the screen-lock check and the quit guard work sandboxed.

## How CI signs and uploads

| Platform    | Workflow → job           | Runs on                       | What it does                                                                                                                                                                                                                                       |
| ----------- | ------------------------ | ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| iPhone/iPad | `ios.yml` → `testflight` | `v*` tags, Run workflow       | takes the run's unsigned IPA (`App (iphone)`), sets the build number and the Keychain/App Group keys, embeds the App Store profiles, signs extension and app (`scripts/ios-sign.sh`), uploads with `altool`; artifact `UwULock-iOS-appstore-<sha>` |
| Mac         | `mas.yml` → `build`      | tags, Run workflow, PRs on it | builds the universal store app with the extension, unsigned, and checks it (`scripts/build-mas.mjs`)                                                                                                                                               |
| Mac         | `mas.yml` → `testflight` | `v*` tags, Run workflow       | signs extension and app with their profiles and entitlements, packs a signed `.pkg` (`build-mas.mjs --sign`), uploads it; artifact `UwULock-mas-<sha>`                                                                                             |

Both signing jobs run on a fresh runner that builds and installs nothing, with a keychain of their
own that is deleted at the end (`scripts/apple-ci.sh`). The build jobs hold no secret.

- **Version**: App Store Connect takes three numbers, so `0.5.0-beta.2` goes up as `0.5.0`.
- **Build number** (`CFBundleVersion`): `<run number>.<attempt>` of the workflow (`ios.yml`
  for the iPhone, `mas.yml` for the Mac). It grows with every run and every re-run, separately
  per platform, as App Store Connect wants. A new version starts no new count; that's fine.
- **Profiles**: `scripts/asc.mjs profiles` asks the App Store Connect API for
  `UwULock <iOS|macOS> App Store <bundle id>`, made for the certificate in the secret. A missing or
  invalid one (a capability changed, the certificate was renewed) is made anew in the same run.
- **Seeing the builds**: `node scripts/asc.mjs builds` lists the newest builds with their
  processing state (with `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_PATH` set). Processing takes
  5–30 minutes after the upload; Apple mails problems with a build.

Starting an upload by hand: Actions → **iOS** or **Mac App Store** → Run workflow → `main`.
With **upload** unticked, the run signs everything (profiles included) but uploads nothing: the
way to try a branch.

**Safari extension**: both store apps carry it (`PlugIns/UwULockSafari.appex`, bundle id
`app.uwulock.safari`, docs/extension.md), built from the run's `extension-safari` artifact.
`scripts/asc.mjs profiles` registers that bundle id when it is missing (platform UNIVERSAL) and
makes its profiles like the others'. The Safari extension needs no App Group or keychain group.

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
| `APPLE_DEVELOPER_ID_KEY`  | the disk image's signing key (PEM, `installers.yml`, release-notes/README.md)                 |
| `APPLE_DEVELOPER_ID_CERT` | its **Developer ID Application** certificate (PEM or base64 DER)                              |

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
   `ITSAppUsesNonExemptEncryption = false` in `Info.ios.plist` and `tauri.mas.conf.json` stops the
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
  build (App Review, guideline 2.5.2/2.3): build the store IPA with feature `store` or tell the
  sideload build apart at run time before the first review.
- **Guidelines to keep in mind**: 2.5.2 (no downloading code — the extension packs nothing), 3.1
  (no purchases), 4.8 (no third-party sign-in), 5.1.1 (privacy policy, account deletion: the account
  lives on the user's own server; say so in the review notes).

## Not tried yet

No Mac and no iPhone were at hand while this was set up: the builds were made, signed and
accepted by App Store Connect, but not started on a device. On the first TestFlight install:

- iPhone/iPad: sign in, unlock with Face ID, turn on UwULock under AutoFill & Passwords and
  sign in with a passkey in Safari (App Group and Keychain group work);
- Mac: the window comes up in the sandbox, signing in reaches the server, a file request lands
  in Downloads, a Bitwarden export can be moved in, the AutoFill extension shows up under
  System Settings → General → AutoFill & Passwords and offers passkeys;
- no update row with a channel on the Mac, no update check in Console.

---

# UwULock im App Store (TestFlight)

UwULock gibt es auf Apples Systemen zweimal. Die **GitHub-Builds** bleiben, wie sie sind: DMG
für den Mac (aktualisiert sich selbst) und die unsignierte IPA zum Sideloaden aufs iPhone. Die
**App-Store-Builds** sind dieselbe App, vom Entwicklerteam signiert und zu App Store Connect
hochgeladen — eine App für iPhone, iPad und Mac (`app.uwulock`). Vorerst landen sie **nur in
TestFlight**: Zur Prüfung eingereicht ist nichts, und CI reicht nie etwas ein.

- **iPhone/iPad**: `ios.yml` → Job `testflight` (bei `v*`-Tags und per Run workflow) signiert die
  unsignierte IPA desselben Laufs mit Apple-Distribution-Zertifikat und App-Store-Profilen (App und
  AutoFill-Erweiterung mit App Group und Keychain-Gruppe) und lädt sie hoch.
- **Mac**: `mas.yml` baut eine sandboxed Universal-App `app.uwulock` ohne Updater (Feature
  `store`) mit der Erweiterung, signiert sie auf einem frischen Runner, packt das `.pkg` und lädt
  es hoch.
- **Version** `0.5.0-beta.2` → `0.5.0`; **Build-Nummer** = Laufnummer.Versuch des Workflows, pro
  Plattform steigend.
- **Safari-Erweiterung**: beide Store-Apps tragen sie (`app.uwulock.safari`, aus dem Artefakt
  `extension-safari` desselben Laufs); `scripts/asc.mjs` legt die Bundle-ID bei Bedarf an. Run
  workflow ohne **upload** signiert nur, zum Ausprobieren eines Branches.
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
