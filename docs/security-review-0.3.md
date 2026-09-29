# Security review, 0.3 (September 2026)

The second security review of UwULock, done before 0.3.0-beta.1 on 29 September 2026. It covers
everything new since 0.2.0-beta.3 (main at `326d59d`, plus PR #6): the new crypto in
`crates/uwulock-core` (extras key, file requests, Sends, passkeys), `crates/uwulock-wasm`, the
network code in `crates/uwulock-bitwarden`, the desktop app, and the new browser extension. It also
covers the UwULock sync in UwUSSH and UwURDP (branch `sync-uwulock`), because those apps open a
UwULock account with `uwulock-core` and keep their secrets under its keys. The review ran together
with UwULock Server 0.6's; the server's findings are in its own `docs/security-review-0.6.md`
(in [UwULock-Server](https://github.com/MinifyX/UwULock-Server/blob/main/docs/security-review-0.6.md)).

Everything below was established by reading the code first. The first review's fixes were checked
again where the new code touches them; they hold, and what is still open from it is at the end.
The first review is [security-review-2026-09.md](security-review-2026-09.md).
A finding several reviewers reported is counted once.

## The trust boundary

As before: the interesting attacker is whoever controls what comes back from the server: the
server itself, a proxy in front of it, or another member of a shared organisation. For the UwU
extras the bar is higher than for Bitwarden's own data. The extras key belongs to the account
alone, so the server must never be able to read it or choose it.

The extension adds a second attacker: **the web page** it runs in. Its menu and save bar live in
the page's own document, so anything the page can draw, style or script is hostile, and so is a
frame inside it.

Severity: **High** is a secret leaving the device, or the server able to read or silently swap a
secret. **Medium** needs a hostile server, a hostile org member or a malicious page, but is real.
**Low** is defence in depth, resource exhaustion, or a policy only the client enforces. **Info** is
worth knowing and nothing to do. The ids (CL-… for UwULock, SA-… for UwUSSH and UwURDP) stay the
same from here on; "reported as" names the reviewers' own numbers.

## Fixed

| Id    | Severity | Where                                                                                  | What                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | Status                                    |
| ----- | -------- | -------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- |
| CL-H1 | High     | Extras key, `uwulock-core` (`extras.rs`), desktop, extension, web vault, UwUSSH/UwURDP | **The server could choose the extras key.** The extras key had two wraps: one under the user key, and an RSA wrap for the account's public key for after an official client rotated the user key. When the user wrap was missing, every client took the RSA wrap. But the server has the public key, so it could make that wrap for a key of its own. The desktop and the suite apps then wrapped it under the user key for good, and everything sealed afterwards was the server's to read: file-request link secrets (with those the server could rewrite a request's details so uploaders encrypt for its key), labels, own icons, and in UwUSSH and UwURDP a space key it chose, and with it every host password and private key the app pushed. One crafted answer to `GET /uwu/v1/keys` was enough, and nothing warned. The second wrap is now `privateKeyWrapped`: the extras key under HKDF-SHA256 of the account's private key (PKCS#8 DER, salt `uwulock-extras-key-v1`, info `private-key-wrap`). Only the key pair's owner can make it, and official rotations keep the pair. `resolve` never takes an RSA wrap, checks that both wraps hold the same key when it has the private key, and adds the private wrap to older keys (`PUT /uwu/v1/keys/private-wrap`). The desktop keeps the key's id and says once when it changes; a file request whose details encrypt for a key that isn't the owner's shows no link (`PublicInfo::is_for`), in the desktop and the extension. The server never offers the RSA wrap any more. Test vectors were made with Python's `cryptography`. (Reported as R5-1, R6-1, R4-1. `1735008`, `f4b9bea`; UwULock Server `security-0.6-extras`) | Fixed (PR #8, server security-0.6-extras) |
| CL-M1 | Medium   | Extension, login (`session.ts`, `uwulock-wasm`)                                        | **The first review's L1 fix wasn't in the extension.** Every login trusted the server's prelogin, down to PBKDF2 with 5 000 rounds, or PBKDF2 where the account uses Argon2id, and sent the hash made with it. A session the server ended also removed the account with its KDF, so a hostile server could force a fresh login whenever it liked and collect a hash about 120 times cheaper to guess from. The extension now keeps the KDF each account's login accepted, by identity endpoint and address, in `storage.local`. A prelogin asking for less is refused before anything is derived or sent, by the desktop app's rule, exported from `uwulock-wasm` as `kdfIsWeakerThan`. A session end keeps it and logging out drops it; after a refusal the popup offers to forget it, for whoever lowered the KDF on purpose. (Reported as R5-2, R6-2. `d829dae`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | Fixed (PR #7)                             |
| CL-M2 | Medium   | Extension, inline menu and save bar (`content/ui.ts`)                                  | **A page could make somebody click the extension's UI through a decoy.** The menu and the save bar sit in a closed shadow root in the page. The check looked only at the host's and `<html>`'s visibility and opacity, so the published DOM clickjacking techniques got through: a layer with `pointer-events: none` above the menu (in the top layer, or after our host), `clip-path`, `mask` or a filter on `<html>` or our host, colours made see-through, or a frame made transparent by its parent. The save bar had no minimum display time at all. Cards and addresses were also listed to every frame, ad frames from other origins included, and filled into any frame that asked, so two tricked clicks gave a page a card with its code. Every click on the menu, its button and the save bar now goes through one guard: the element was shown, unchanged and in place for 500 ms; a real pointer went down on it then, and the click lands on it; at the press, hit tests at its middle and near its corners find it, nothing is open in the top layer, nothing rendered comes after our host, and there is no faint or blurry filter, clip, mask, blend mode or 3D context. On Chromium, IntersectionObserver v2 must have seen it whole for 300 ms, which also covers a transparent frame. Our host's styles are put back when the page changes them. In a frame from another origin on a browser without visibility tracking (Firefox) the menu only opens UwULock's window. Cards and addresses are listed only to the page and frames of its own origin, and filled into nothing else. (Reported as R6-3. `140d2d8`; see CL-L13 for what remains on Firefox)           | Fixed (PR #7)                             |

## Not fixed: Low and Info

These are listed for the record and stay as they are in 0.3.0-beta.1. Each has a suggested
follow-up.

Desktop and `uwulock-core`:

- **CL-L1 Low, comparing versions shows an old hidden value in clear.** Custom fields are compared
  by index, and whether a value is secret comes from the current item alone. A field that was
  hidden in a version and is text now (a deleted "PIN" moves the next field up) reaches the page in
  clear and shows without the eye. The re-prompt still holds. (Reported as R5-3.) _Open (accepted
  for 0.3.0-beta.1)._ Follow-up: secret if either side is, with a test for the shift.
- **CL-L2 Low, locking leaves a prepared move in memory.** After a move was prepared, the source
  vault (decrypted), both user keys and the source's tokens stay in memory through auto-lock and
  logout. Nothing can use them while locked; only memory access (a dump, swap) reaches them.
  (Reported as R5-4.) _Open (accepted for 0.3.0-beta.1)._ Follow-up: clear the move on lock,
  session end and logout.
- **CL-L3 Low, "Share as Send" hands out passwords an org member may not see.** The first review's
  L7 (`viewPassword: false` is ignored) stayed on the device; a Send takes it off it. Official
  Bitwarden clients have no such path, and the move already skips such items. (Reported as R5-5.)
  _Open (accepted for 0.3.0-beta.1)._ Follow-up: refuse password, TOTP, hidden fields and card
  numbers and codes in a Send when `viewPassword` is false.
- **CL-L4 Low, files from file requests have no Mark-of-the-Web.** Anyone can upload to a file
  request; the owner saves the file into Downloads with the uploader's name and extension, without
  `Zone.Identifier` on Windows or `com.apple.quarantine` on macOS, so SmartScreen and Protected View
  don't apply. (Reported as R5-6.) _Open (accepted for 0.3.0-beta.1)._ Follow-up: set both, and
  warn before saving an executable type.
- **CL-L5 Low, a device's start page can send the icon fetch anywhere.** Device icons are meant
  only for local addresses, but the icon links a local page names are fetched wherever they point,
  with certificate checks off. Nothing secret is sent; a third party learns this person has this
  device, their address and the time. The local check is by name only. (Reported as R5-7.)
  _Open (accepted for 0.3.0-beta.1)._ Follow-up: fetch only local candidates; optionally resolve
  and require local addresses.
- **CL-L6 Low, Windows Hello: one signature opens the vault for good.** The challenge is fixed and
  public (`uwulock-hello-v1:<account id>`) and the signature deterministic, so one signature
  obtained by another process as the same user decrypts `helloUserKey` offline, also after a
  password change. Same-user malware is mostly out of scope. (Reported as R5-8.) _Open (accepted for
  0.3.0-beta.1)._ Follow-up: a random challenge per enrolment kept under DPAPI; document that
  turning Hello off and on again is the recovery.
- **CL-L7 Low, new downloads have no size limit.** The first review's L3 grows: attachments and Send
  files during a move (the source server names URL and size), file-request files, and automatic
  icons are read whole, so a hostile server can exhaust the app's memory on every icon view. The
  move's `download` also fetches any https address the source server names, without a session.
  (Reported as R5-9.) _Open (accepted for 0.3.0-beta.1)._ Follow-up: one reader with a ceiling
  (256 KiB for icons, the file limit for files, a few MiB for JSON), and downloads only from the
  source server's host.

Extension:

- **CL-L8 Low, a compromised renderer can ask for fills.** The pick in the inline menu happens in
  the page's process, so the background cannot tell a real pick from a forged one. Since CL-M2's fix,
  cards and addresses go only to the page's own origin, which keeps an ad frame from asking for them;
  a compromised renderer of the site itself still gets its matching logins, cards and addresses
  without a click. (Reported as R6-7.) _Partly fixed with CL-M2 (PR #7); the rest open (accepted
  for 0.3.0-beta.1)._ Follow-up: draw the menu's list in an extension page in a frame and answer a
  pick there with a one-time token, like the popup's offer.
- **CL-L9 Low, a PIN kept after a restart is a short offline guess.** With "keep after restart" the
  user key sits on disk under a PIN of four characters or more: a copy of the profile opens in
  minutes. The five tries are counted in `storage.session`, so a browser restart resets them, and
  tries at once can pass the count. Bitwarden offers the same option. (Reported as R6-8.)
  _Open (accepted for 0.3.0-beta.1)._ Follow-up: count in `storage.local`, one try at a time, and
  six characters and a warning for "after restart".
- **CL-L10 Low, accounts are keyed by the token's unchecked `sub`.** A hostile server that knows the
  user id of an account on another server (an org member can) replaces that account's entry, which
  then inherits its protected user key and its remember-me token; the token then goes to the
  hostile server at its next login. (Reported as R6-9.) _Open (accepted for 0.3.0-beta.1)._
  Follow-up: key accounts by identity endpoint and `sub`, and never inherit across servers.
- **CL-L11 Low, address matching.** "Starts with" is a plain prefix
  (`https://bank.example` matches `https://bank.example.evil.test/`), as in Bitwarden; a shared
  item's regex runs unanchored on every page load and frame, so an org member can stall everybody's
  extension; equivalent domains come unencrypted from the server, so a hostile server can make a
  bank's login fill on its own site, as with Bitwarden's clients. (Reported as R6-10.)
  _Open (accepted for 0.3.0-beta.1)._ Follow-up: same origin before the prefix, a length cap and
  top-frame-only for regexes, and Bitwarden's global list shipped with the extension.
- **CL-L12 Low, a pending save isn't bound to an account.** A login typed while one account was
  open is saved into whichever account is open when the person answers. (Reported as R6-11.)
  _Open (accepted for 0.3.0-beta.1)._ Follow-up: keep the account id with every pending save and
  ask again when it differs.
- **CL-L13 Low, Firefox: a popover in a closed shadow root isn't seen.** Residual of CL-M2. Firefox
  has no IntersectionObserver v2, so the guard looks for open popovers and modal dialogs itself; a
  popover the page opens inside a closed shadow root of its own, in the top frame, is invisible to
  that search and can lie over the menu. The other checks (500 ms, the hit tests, the pointer on the
  element) still apply, and Chromium is covered by the visibility tracking. (Found while fixing
  CL-M2.) _Open (accepted for 0.3.0-beta.1)._ Follow-up: IntersectionObserver v2 when Firefox has
  it; until then, consider refusing clicks while the page has any element in the top layer that
  the hit tests can't account for.

Info:

- **CL-I1 Info, automatic icons tell the server which hosts are in the vault.** In the desktop app
  and the extension, as Bitwarden's icon service does; `showIcons` turns them off. It belongs in the
  privacy notes, since the server otherwise sees no addresses. (Reported as notes in R5 and R6.)
  _Open (accepted for 0.3.0-beta.1)._ Follow-up: one sentence in the docs.
- **CL-I2 Info, the extension's anonymous requests follow redirects and read without a limit.**
  The same as the first review's L3 in the desktop: a 307 or 308 from a hostile server would send
  the login form again elsewhere. (Reported as a note in R6.) _Open (accepted for 0.3.0-beta.1)._
  Follow-up: `redirect: 'error'` and a size cap.
- **CL-I3 Info, the extension asks the re-prompt once per unlock.** Answered once, it holds until
  the vault locks, inline-menu fills included; Bitwarden asks every time. (Reported as a note in
  R6.) _Open (accepted for 0.3.0-beta.1)._ Follow-up: decide whether to ask per fill.
- **CL-I4 Info, the extension doesn't lock with the system.** There is no `idle` permission; the
  timeout counts sleep. (Reported as a note in R6.) _Open (accepted for 0.3.0-beta.1)._ Follow-up:
  lock on `idle` state `locked`.
- **CL-I5 Info, a moved Send loses its password or addresses.** It gets a new link that only the
  owner knows, and the preview says so. (Reported as a note in R5.) _Open (accepted for
  0.3.0-beta.1)._ Nothing to do.
- **CL-I6 Info, the clock-gap check may never fire on Windows.** `Instant` probably keeps counting
  through sleep there, so auto-lock counts the sleep and the lock screen after waking locks the
  vault anyway. (Reported as a note in R5.) _Open (accepted for 0.3.0-beta.1)._ Nothing to do.
- **CL-I7 Info, the file limit is multiplied without saturating.** `m * 1024 * 1024` on a value the
  page gives wraps in a release build when the server names no limit. (Reported as a cleanup note
  in R5, PR #6.) _Open (accepted for 0.3.0-beta.1)._ Follow-up: `saturating_mul`.

## UwUSSH and UwURDP

Both apps share the UwULock sync code apart from names, so every finding holds for both. They sign
in to a UwULock account, open its extras key with `uwulock-core`, and keep their hosts in a suite
space under a key wrapped by it; that is why CL-H1 reached them.

| Id    | Severity | Where                                            | What                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | Status                      |
| ----- | -------- | ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------- |
| SA-M1 | Medium   | Sign-in, two-step login (`lock.rs`)              | **The remember-me token went to whatever server was typed in.** It was kept once per device and sent with the next sign-in, with the device id, to any server address. A look-alike server that answers prelogin with the real server's KDF gets the right hash too, and can replay all three to skip the second step. It is now kept per account, by the normalized server address and the lower-case email, sent only on an exact match, never carried to another server or account, and forgotten at sign-out. (Reported as R6-4. UwUSSH `3745de1`, same in UwURDP)                                                                                                                                                                | Fixed (UwUSSH/UwURDP PR #3) |
| SA-M2 | Medium   | Sign-in, key derivation (`api.rs`, `account.rs`) | **The key derivation could be lowered at a new sign-in.** The same as CL-M1: prelogin was trusted at every sign-in and every request for an email code, down to the floors. The KDF a sign-in used is now kept per account and a weaker one is refused before anything is derived or sent, by the desktop app's rule (`Kdf::is_weaker_than`). Signing out forgets it. (Reported as R6-5. UwUSSH `3c95757`, same in UwURDP)                                                                                                                                                                                                                                                                                                            | Fixed (UwUSSH/UwURDP PR #3) |
| SA-M3 | Medium   | Suite space (`lock.rs`, `join_space`)            | **A different space was taken without asking.** A sign-in joined whatever space the server listed and pushed everything the device holds into it. So a server that kept the space from before a rekey, whose key a lost device still has, could list it again and roll the rekey back; and it was the step that turned CL-H1 into a silent push of every host secret. Each account now remembers the space this device used and the ones it left. A sign-in that finds another space, or none, stops before anything is taken or written, the page explains both readings, and only a sign-in the person agreed to takes exactly that space. A space the device left is refused. (Reported as R6-6. UwUSSH `2257db8`, same in UwURDP) | Fixed (UwUSSH/UwURDP PR #3) |

CL-H1 is fixed in `uwulock-core`; both apps pin it at `44078ce` (UwULock-Client main with PR #8),
send the private wrap to older keys, and passed their live tests against the fixed server.

Not fixed:

- **SA-L1 Low, a server can exhaust memory during the move and over the live socket.** The move
  keeps up to 1 000 pages of about 17 MiB each, blobs included, twice; the socket takes
  tungstenite's defaults (64 MiB messages) where the contract says about 4 KiB. (Reported as R6-13.)
  _Open (accepted for 0.6.0-beta.1)._ Follow-up: keep only headers and cap the totals at the
  quotas; `max_message_size` of about 64 KiB.
- **SA-L2 Low, the move's check trusts the server's headers.** Made-up headers pass the last step,
  the device forgets UwUSync, and the last device may be told it can delete the UwUSync account:
  data lost for other devices, nothing disclosed. (Reported as R6-14.) _Open (accepted for
  0.6.0-beta.1)._ Follow-up: open every record read back under the target key and count failures
  as missing.
- **SA-I1 Info, TLS trusts only the webpki roots.** A UwULock Server behind a private or LAN CA
  can't be reached; there is no insecure fallback, and the comment claiming the system's roots is
  wrong. (Reported as R6-15, "Low/Info"; counted as Info, since it fails closed.) _Open (accepted
  for 0.6.0-beta.1)._ Follow-up: the system's roots, or the existing pinned configuration for LAN
  servers, and a correct comment.
- A suite push names no space; a race after a rekey can lose an edit, not reveal it. That is
  UwULock Server's SV-L10 (reported as R2-6, R6-12) and is counted there.

## Still open from the first review

- **L2** is unchanged: a delta's `profile.key` still replaces the stored key unchecked, and the
  re-prompt checks the typed password against the key the server gave.
- **L3** is unchanged and grows with the new downloads (CL-L7) and the extension (CL-I2).
- **L7** stays on the device except through "Share as Send" (CL-L3).
- **L4** is addressed: the desktop now locks with the system (logind's sleep and lock signals and
  the screen saver on Linux, the input desktop on Windows, the session on macOS) and with a clock-gap
  check; see CL-I6 for Windows.

## Checked and fine

The point of saying so: these were looked at and held.

**The new crypto.** Every new module decrypts through the same checked path: IV and MAC lengths
checked, the MAC compared in constant time before AES, RSA and AES types not swappable, type 0
only for the pre-2019 user key. A user wrap of the extras key that fails its MAC is an error, never
a fallback. Sends and file requests derive their keys with separate HKDF labels from 16-byte
secrets from the OS; a file request's password is salted with the link secret, so the server can't
test guesses. The owner's public key for a file request comes from the account's own private key.
Submission and file keys are fresh each time, and file names are cleaned of separators, reserved
names and bidi overrides. Passkeys are P-256 with RFC 6979, and every stored value is under the item
key.

**The desktop app.** The capabilities and the CSP are unchanged. Every new command takes ids only,
checks them against the open vault and enforces the re-prompt; every `/uwu/v1` path is escaped;
uploads go only to the API host, so the token never leaves; the move journal holds ids only, sealed
under the target's user key, and every moved item, attachment and Send gets a fresh key. Windows
Hello falls back to the master password, drops a key sealed before a rotation, and is removed at
logout. The first review's M2 fix still holds.

**Live updates and delta sync.** rustls with the bundled and the system's roots, `ws://` only on
loopback, no redirects, 1 MiB messages, a depth-limited MessagePack reader. A delta can only touch
its own account, and the loop stops after 1 000 pages.

**The extension.** No external messaging and no pages a site can frame; every decision about a
page, a frame or an address uses what the browser says about the sender, never the message. Logins
go only to a frame whose own address matches, offers are random, bound to tab and item, and live 15
seconds. The passkey provider takes the origin from the browser, refuses public suffixes and IPs,
builds `clientDataJSON` itself and sets UV only after the master password. The user key lives in
WASM memory and `storage.session` (trusted contexts only), and locking clears both. The popup
renders server text as text, and the CSP is strict. The clipboard is cleared by an alarm that
survives a worker restart. Automatic icons come only from the account's own host, PNG only, 256 KiB
at most.

**UwUSSH and UwURDP.** Records are XChaCha20-Poly1305 with the label, id, kind, space, clock and the
deleted flag in the AAD, so the server can't read, swap, forge deletions or rewrite clocks. https
only (http only to loopback), no redirects, every body capped, SQL parameterised, tokens sealed by
the system, and no secrets in logs. The move re-seals on the device and never sends UwUSync's
secrets or plain text.
