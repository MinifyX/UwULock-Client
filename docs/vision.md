# Vision

UwULock is where my passwords live, in the same family as the tools I open all
day — UwUMail, UwUSSH, UwURDP, UwUNotes.

## What it is for

1. **Today: a better window onto a vault I already have.** My Vaultwarden
   keeps working exactly as it does; UwULock is a client for it, and for
   Bitwarden's cloud too, with the same encryption as the official apps.
2. **Tomorrow: a vault the suite shares.** UwUSSH keys, UwURDP logins and
   UwUMail accounts come out of one place instead of three vaults.
3. **Some day: a server of its own.** A UwULock server — one binary, one
   Docker image, one SQLite file, like UwUSSH's — for people who want the UwU
   way end to end. Vaultwarden stays a first-class choice, not a legacy mode.

## What it will never do

- **Send the master password anywhere.** Only its hash leaves the device, and
  only at login.
- **Put secrets into the web view unasked.** A password reaches the page when
  someone asks to see it, and not before.
- **Telemetry, accounts with me, a subscription.**
- **Favicons or any lookup that tells a third party which sites are in the
  vault.**
- **Be playful about security.** A wrong master password, a refused login, a
  certificate problem: plain words, no kaomoji, no Nyu.
