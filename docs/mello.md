# Mello link

Mello (MelloAI-Desktop, the UwUSuite's desktop maid) can mention the vault:
whether it is locked and how healthy it is. For that, UwULock offers a tiny
read-only endpoint on this computer. It reports **counts only**, never names,
addresses, usernames, notes or passwords. Code: `crates/uwulock-app/src/mello.rs`.

## Switching it on

Off by default. Settings → Security → "Mello verbinden" (Tauri desktop app
and the native macOS app; not on Android or iOS). The first switch on creates
the pairing token; Settings shows it for copying and can make a new one
("Neuer Token"), which unpairs Mello. Switching off stops the listener at once,
forgets the token and deletes `mello-token`; switching on again makes a new
token, so Mello has to be paired again.

The sandboxed macOS app needs `com.apple.security.network.server` for the
listener (`apps/apple/Config/UwULock-macOS.entitlements`).

Files in UwULock's data directory:

| File          | Content                                                              |
| ------------- | -------------------------------------------------------------------- |
| `mello.json`  | `{"enabled": bool, "port": 8748}`; the port can only be changed here |
| `mello-token` | 64 hex characters (32 random bytes), mode 0600                       |

## The request

```
GET /v1/status HTTP/1.1
Host: 127.0.0.1:8748
Authorization: Bearer <token>
```

Answer (`Content-Type: application/json`, `Cache-Control: no-store`):

```json
{ "locked": false, "entries": 42, "weak": 3, "reused": 4 }
```

| Field     | Meaning                                                                                |
| --------- | -------------------------------------------------------------------------------------- |
| `locked`  | `true` when no vault is open (also: no account)                                        |
| `entries` | items in the open vault, without the trash; `null` while locked                        |
| `weak`    | logins whose password is weak (under 50 bits, as in Vault health); `null` while locked |
| `reused`  | logins whose password another login shares; `null` while locked                        |

Reading the status is not activity: polling never keeps the vault unlocked.

## Status codes

Checked in this order; the vault is only looked at after the token matched.

| Code | When                                                                                                         |
| ---- | ------------------------------------------------------------------------------------------------------------ |
| 400  | malformed request, not HTTP/1.x, two `Host` or `Authorization` headers                                       |
| 403  | an `Origin` header (a browser page), or `Host` isn't `127.0.0.1:<port>` / `localhost:<port>` (DNS rebinding) |
| 404  | any path but `/v1/status`                                                                                    |
| 405  | any method but `GET`                                                                                         |
| 401  | no or a wrong token (`WWW-Authenticate: Bearer`)                                                             |
| 431  | request head over 8 KiB                                                                                      |
| 200  | the counts                                                                                                   |

Every answer closes the connection.

## Security

- Listens on `127.0.0.1` only; peers that aren't loopback are dropped.
- The token is compared in constant time (on SHA-256 digests).
- At most four connections at once; a head has 5 seconds to arrive.
- Other local programs can reach the port, so they too need the token; they
  learn nothing more than the four values above.
- Mello keeps the token in the OS keychain and hides UwULock entirely in
  streamer mode (MelloAI-Desktop, `crates/mello-integrations/src/uwulock.rs`).
- Known and accepted: each poll counts weak and reused passwords from a short
  lived copy of the login metadata (as Vault health does); four slow clients
  can hold all connection slots for up to 5 seconds. Review:
  `docs/security-review-mello.md`.
