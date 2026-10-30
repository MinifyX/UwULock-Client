# Native Apple app (iOS + macOS) — plan and status

Branch `feat/native-apple`. Replaces the Tauri builds on iOS and macOS (DMG,
Mac App Store, TestFlight, IPA). Windows, Linux and Android stay Tauri.

## Decisions (2026-10-30)

- SwiftUI + Rust: the Tauri-free app logic moves into `crates/uwulock-app`;
  `crates/uwulock-ffi` exposes it to Swift via UniFFI as an XCFramework.
  The Tauri app keeps working on top of `uwulock-app` (thin command wrappers).
- Full feature parity with the Tauri app (all 128 commands) in the first native version.
- Minimum OS: iOS 27, macOS 27. Liquid Glass everywhere, no fallbacks.
- Mac: three columns like Apple's Passwords app (sidebar → list → detail),
  menu bar item. Updates outside the store via Sparkle.
- Design: native controls with Liquid Glass, UwUSuite tokens (light/dark/high
  contrast), UwU Sans, Lucide icons, wordmark, Nyu on empty/lock screens.
- Seamless migration: same bundle IDs (`app.uwulock` iOS/MAS,
  `app.uwulock.desktop` DMG), same data folder and format, biometric keys stay valid.
- Project: `apps/apple`, generated with XcodeGen (`project.yml`), targets: app
  (iOS+macOS), AutoFill/passkey credential provider extension, Safari web extension.

## Milestones

1. [x] `crates/uwulock-app`: app logic out of `apps/desktop/src-tauri`, `Host` trait for
       events/clipboard/biometrics/opening links; Tauri app compiles and tests pass on it.
2. [x] `crates/uwulock-ffi` (UniFFI) + `scripts/apple-xcframework.sh`; Swift side in
       `apps/apple/UwULock/Core` (typed commands, models, `AppleHost`), app starts on the
       core's status.
3. [x] `apps/apple` skeleton: XcodeGen, design system (tokens, font, icons), app shell.
4. [x] Screens: login/2FA/SSO, lock/biometrics, vault, detail, editor, generator, settings.
       iPhone: tabs with the search tab (field pinned under the title); iPad/Mac: three columns.
5. [x] Screens: sends, health, import/move, extras, suite, wifi, file requests, masked mail, versions.
6. [x] Extensions: credential provider (passwords + passkeys), Safari web extension.
       `apps/apple/CredentialProvider` (UwULockPasskeys, UwUDesign sheet, de/en catalog,
       `UwULockPasskeysTests`), `apps/apple/SafariExtension` (UwULockSafari; web extension files
       from `apps/extension/dist/safari` by a build phase). Old sources in
       `apps/desktop/src-tauri/apple` stay until the Tauri Apple builds go (milestone 7).
7. [ ] Build/CI/release: IPA, DMG (signed, Sparkle), MAS/TestFlight; Tauri mac/iOS paths removed.
       Done: `.github/workflows/apple.yml` (PR/main: cached xcframework, xcodegen, iOS Simulator,
       macOS and Direct builds, `UwULockPasskeysTests`; tags: archive → sign/TestFlight/notarize →
       migration setup), `scripts/apple-release.sh`, Sparkle signing + appcasts in `pnpm release`
       (`scripts/sparkle.mjs`, `release-feeds.mjs`), old `ios.yml`, `mas.yml`, the macOS jobs of
       `installers.yml` and their scripts removed. Verified locally: unsigned iOS archive + IPA,
       Direct archive with team entitlements, ad-hoc DMG (starts), migration setup with the native
       payload, actionlint. Release configuration compiles (iOS, macOS, Direct). Still open: no signed run through
       CI/TestFlight/notarization yet; register the Developer ID identifiers
       `app.uwulock.desktop(.passkeys)` with AutoFill + App Groups.
8. [ ] Docs (architecture, mobile, app-store, install), security review.
