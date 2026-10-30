# Security review, Mello link (October 2026)

The review of the Mello link, done on 30 October 2026 before the first commit: the endpoint
`crates/uwulock-app/src/mello.rs`, its settings in the Tauri app (`MelloSettings.tsx`, commands)
and the native macOS app (`SecuritySettings.swift`, FFI dispatch), and the client in
MelloAI-Desktop (`crates/mello-integrations/src/uwulock.rs`, `brain.rs`, the chat). Contract:
`docs/mello.md`.

Severity as in 0.4. No Critical or High findings. The Medium and Low findings are fixed; the Info
findings are accepted below.

| ID  | Severity | Finding                                                                                                                                                         | Status                                                                                                                                             |
| --- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| M-1 | Low      | Mello client: the 4 KiB body cap was checked after the whole answer had been buffered, so another local program on the port could make Mello hold a large body. | **Fixed.** Content-Length is checked first, then the body is read chunk by chunk and dropped past 4 KiB. Test: `a_huge_answer_is_cut_off`.         |
| M-2 | Medium   | Mello chat: the history sent to the LLM could still carry vault counts from turns before streamer mode was switched on.                                         | **Fixed.** While streaming, the chat only sends turns since streaming began. A poll still in flight when streaming starts is dropped by the brain. |
| M-3 | Low      | `mello.rs`: switching on and off (or renewing) concurrently could leave a listener running with the setting off, or answering with a stale token.               | **Fixed.** Start, switch and renew run under one `switching` lock; `token()` answers nothing unless the setting is on.                             |
| M-4 | Low      | `mello.rs`: switching off kept the token, so a program that had once read it was paired again as soon as the link came back.                                    | **Fixed.** Off forgets the token and deletes `mello-token`; on makes a new one. Test: `off_by_default_and_off_unpairs`.                            |
| M-5 | Low      | The sandboxed macOS app couldn't listen without `com.apple.security.network.server`; the link would silently not work there.                                    | **Fixed.** Entitlement added with a comment.                                                                                                       |
| M-6 | Info     | Each poll builds the weak/reused counts from a short-lived copy of the login metadata (as Vault health does).                                                   | **Accepted.** Same data and code path as Vault health; nothing leaves the process except the counts.                                               |
| M-7 | Info     | Four slow local clients can hold all connection slots for up to 5 seconds (head timeout).                                                                       | **Accepted.** Loopback only, no data exposed; Mello retries on the next poll.                                                                      |
| M-8 | Info     | The pairing token passes through the settings webview (copy button).                                                                                            | **Accepted.** The webview already handles the master password and all secrets.                                                                     |
