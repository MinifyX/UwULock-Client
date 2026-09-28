# Moving from Bitwarden to UwULock

Bitwarden's export leaves attachments, Sends and organisations out. The desktop app moves a
whole vault instead: from Bitwarden's cloud (bitwarden.com or bitwarden.eu), a self-hosted
Bitwarden or a Vaultwarden, into an account on a UwULock Server.

## How

1. Add your UwULock account to the app (if it isn't there yet), switch to it and unlock it.
2. Settings → Account → **Move from Bitwarden**.
3. Log in to the account you are moving from: server, email, master password, and the two-step
   code or Bitwarden's new-device code if it asks. This login is only kept in memory while the
   dialog is open; nothing of it is written to disk, and nothing changes in the old account.
4. Look at the preview: how many folders, organisations, collections, items, attachments and
   Sends will move, and what can't move (below).
5. **Move.** The dialog shows the progress. **Stop** ends it after the object it is on.

Everything is decrypted on your device only and encrypted again for UwULock: every item gets a
new item key, every attachment a new key, every Send a new key (so a new link), every
organisation a new key. The servers never see anything in the clear.

## What moves

- Folders.
- Items of every kind — logins, notes, cards, identities, SSH keys — with passkeys, custom
  fields, password history, favourites and the master password re-prompt.
- Attachments.
- Sends, text and file, with their deletion and expiry dates, their maximum number of accesses,
  and whether they are hidden or disabled. They get new links; the old links keep pointing at the
  old server.
- Organisations. If the UwULock Server offers families and you may create one, each organisation
  becomes a family with its collections, and its items go into the matching collections.
  Otherwise its items go into a personal folder named after it.

## What doesn't

- **Send passwords.** The server only keeps a hash of a Send's password, so it can't be carried
  over. Such Sends arrive without a password (and with a new link nobody has yet); set a new
  password when you share them again.
- **File Sends with a password, disabled or used up** can't be fetched and stay behind. Fetching
  a file Send counts as one access at the old server.
- **Sends only for certain email addresses** arrive without that restriction.
- **Expired Sends** and **items in the trash** stay behind.
- **Organisation members.** Invite them again in UwULock's web vault; they log in and are
  confirmed as usual.
- **Organisation items you may only read** (or whose passwords are hidden from you) stay in the
  organisation; whoever manages it can move them.
- **Files larger than the UwULock Server allows.**

## Doing it again

The app keeps a journal of what moved (old id → new id, nothing else), sealed under your UwULock
account's key in its data folder (`accounts/<id>/move-journal.json`). So:

- A move that was stopped, lost its connection or had the app closed continues where it stopped.
- A second move later brings over only what is new in the old account.
- Something you deleted in UwULock after the move is moved again on the next run.

Changes to items that had already moved are not carried over again.
