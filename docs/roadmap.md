# Roadmap

My wish list, roughly in order, without dates.

## 0.1 · Beta — reading (released)

- Log in to Vaultwarden and Bitwarden (self-hosted, .com, .eu), PBKDF2 and
  Argon2id, two-step login with authenticator, email and YubiKey OTP
- Browse, search, copy, reveal, one-time codes, password generator
- Auto-lock, clipboard clearing, offline copy, periodic sync
- Installer, signed updates, German and English

## 0.2 · Writing (released)

- [x] Create, edit, delete and restore items of every kind, folders,
      favourites, move items between folders
- [x] Password history kept on change, the generator right in the password
      field
- [x] Several accounts side by side (private and work), one open at a time

## 0.2.x · Next (shipped with 0.3.0-beta.1)

- [x] Notifications from the server (Bitwarden's WebSocket hub, UwULock
      Server's realtime channel) instead of polling
- [x] Unlock with Windows Hello, lock when the system locks or sleeps
- [ ] Touch ID — needs an Apple Developer ID signature (keychain items that
      only open with a finger); not possible for an unsigned app

## 0.3 · The suite (0.3.0-beta.2 released)

- [x] A browser extension for Chromium and Firefox, from one code base:
      login and unlock (PIN too), the page's logins first, filling from the
      field, the context menu and Ctrl+Shift+L, saving and updating what was
      signed in with, passkeys in Bitwarden's format, live sync — straight to
      UwULock Server, Vaultwarden or Bitwarden
- [x] The extension with UwULock Server's own extras: masked addresses, the
      server's icons, file requests, sharing an item as a Send
- [x] UwULock Server 0.6 in the desktop app: delta sync and its realtime
      channel, own icons, entry versions, renewal reminders, file requests,
      masked addresses, sharing an item as a Send (also only for given
      addresses, on a send domain), families, travel mode
- [x] Moving from Bitwarden (cloud or self-hosted) to UwULock Server, with a
      preview first and a restart that picks up where it stopped
- [x] The suite vault's keys and transport for UwUSSH and UwURDP in
      `uwulock-bitwarden`
- [x] Security review before the release
      ([security-review-0.3.md](security-review-0.3.md)); its Low and Info
      findings fixed in 0.3.0-beta.2
- [x] Follow UwULock Server's feature switches: what the admin switched off
      disappears from the app and the extension (0.3.0-beta.2)

Still to come:

- UwUSSH takes SSH keys from UwULock, UwURDP takes logins, UwUMail account
  passwords — through a local, authenticated channel, one confirmation per
  use
- Autotype into other windows (Ctrl+Alt+A), a quick-search window from the
  tray
- Import from Bitwarden JSON, KeePass, browser CSV; encrypted export
- Attachments, sends and passkeys in the desktop app

## 0.4 · Wi-Fi networks and phones (0.4.0-beta.1 released)

- [x] Wi-Fi networks as an item type of their own, the same contract as UwULock
      Server's web vault ([wifi.md](wifi.md)): a secure note with a marker field,
      so Bitwarden's apps keep showing a note. Editor, details, filter and a QR
      code in the desktop app; view, copy and QR code in the extension (never
      filled)
- [x] "Connect" on Android: the system's add-network sheet (11+) or a
      suggestion (10), Enterprise with a domain and the system's CAs, an
      explanation plus Wi-Fi settings with the password copied otherwise. Not
      on iOS: the entitlement doesn't survive sideloading ([wifi.md](wifi.md))
- [x] UwULock for Android (APK, signed) and the iPhone (unsigned IPA for
      sideloading) from the same app: vault, search, copy, one-time codes,
      generator, in a phone layout with a drawer, full-screen dialogs and
      Android's back button ([mobile.md](mobile.md))
- [x] Unlock with fingerprint or face: Android Keystore, iOS Keychain
- [x] Lock after a minute in the background; no screenshots on Android
- [x] CI: Android build, signing and emulator smoke test; iOS simulator build,
      unsigned IPA and simulator smoke test; both in `pnpm release`
- [x] The password check in the app, on the computer and the phone: the
      report (breached, site breach after the last change, reused, weak, no
      https, 2FA possible) and the review one card at a time — swipe or ← →,
      open the change-password page, generate and save a new password (the old
      one into the history), later, ignore. The rules live in uwulock-core
      (`health`); ignore list, report and consent are the web vault's
      ([uwu-extras.md](uwu-extras.md#password-check))
- [x] Security review before the release
      ([security-review-0.4.md](security-review-0.4.md))

Later on phones:

- Filling logins into other apps: an Android autofill service and an iOS
  password provider (credential provider extension; needs a signed app with
  the AutoFill entitlement)
- Passkeys on the phone

## Later · A server of its own

- UwULock server: Bitwarden-compatible API, so the official apps keep
  working, with UwUSSH-Server's setup (one binary, Docker, SQLite, pinned
  certificate)
- [x] Sharing with family or a small team (UwULock Server 0.6: families; the
      web vault manages them, the apps show them)
- The UwUSuite website gets UwULock's card, downloads and release notes
