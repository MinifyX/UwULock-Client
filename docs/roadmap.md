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

## 0.2.x · Next

- [x] Notifications from the server (Bitwarden's WebSocket hub, UwULock
      Server's realtime channel) instead of polling
- [x] Unlock with Windows Hello, lock when the system locks or sleeps
- [ ] Touch ID — needs an Apple Developer ID signature (keychain items that
      only open with a finger); not possible for an unsigned app

## 0.3 · The suite (now)

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

- UwUSSH takes SSH keys from UwULock, UwURDP takes logins, UwUMail account
  passwords — through a local, authenticated channel, one confirmation per
  use
- Autotype into other windows (Ctrl+Alt+A), a quick-search window from the
  tray
- Import from Bitwarden JSON, KeePass, browser CSV; encrypted export
- Attachments, sends and passkeys in the desktop app

## Later · A server of its own

- UwULock server: Bitwarden-compatible API, so the official apps keep
  working, with UwUSSH-Server's setup (one binary, Docker, SQLite, pinned
  certificate)
- [x] Sharing with family or a small team (UwULock Server 0.6: families; the
      web vault manages them, the apps show them)
- The UwUSuite website gets UwULock's card, downloads and release notes
