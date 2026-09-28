# Roadmap

My wish list, roughly in order, without dates.

## 0.1 · Beta — reading (released)

- Log in to Vaultwarden and Bitwarden (self-hosted, .com, .eu), PBKDF2 and
  Argon2id, two-step login with authenticator, email and YubiKey OTP
- Browse, search, copy, reveal, one-time codes, password generator
- Auto-lock, clipboard clearing, offline copy, periodic sync
- Installer, signed updates, German and English

## 0.2 · Writing (now)

- [x] Create, edit, delete and restore items of every kind, folders,
      favourites, move items between folders
- [x] Password history kept on change, the generator right in the password
      field
- [x] Several accounts side by side (private and work), one open at a time

## 0.2.x · Next

- [ ] Notifications from the server (Bitwarden's WebSocket hub) instead of
      polling
- [ ] Unlock with Windows Hello / Touch ID, lock when the system locks or
      sleeps

## 0.3 · The suite

- [x] A browser extension for Chromium and Firefox, from one code base:
      login and unlock (PIN too), the page's logins first, filling from the
      field, the context menu and Ctrl+Shift+L, saving and updating what was
      signed in with, passkeys in Bitwarden's format, live sync — straight to
      UwULock Server, Vaultwarden or Bitwarden
- [ ] The extension with UwULock Server's own extras: masked addresses, the
      server's icons, file requests, sharing an item as a Send

- UwUSSH takes SSH keys from UwULock, UwURDP takes logins, UwUMail account
  passwords — through a local, authenticated channel, one confirmation per
  use
- Autotype into other windows (Ctrl+Alt+A), a quick-search window from the
  tray
- Import from Bitwarden JSON, KeePass, browser CSV; encrypted export
- Attachments, sends, passkeys

## Later · A server of its own

- UwULock server: Bitwarden-compatible API, so the official apps keep
  working, with UwUSSH-Server's setup (one binary, Docker, SQLite, pinned
  certificate)
- Sharing with family or a small team
- The UwUSuite website gets UwULock's card, downloads and release notes
