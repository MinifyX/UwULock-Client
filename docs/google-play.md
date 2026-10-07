# UwULock on Google Play

Status: internal testing only (owner's own devices). No review, no store listing yet.

## Build

`.github/workflows/play.yml`, started by hand for a release tag:

```sh
gh workflow run play.yml -f tag=v0.6.0-beta.2
# once, for Play App Signing with UwULock's own key:
gh workflow run play.yml -f tag=v0.6.0-beta.2 -f pepk_public_key="$(cat encryption_public_key.pem)"
```

The artifact `UwULock-Play-<tag>` holds `UwULock-<version>-<code>.aab` (arm64, armv7, x86_64),
signed with the release key (the same certificate as the GitHub APK, so both install over
each other), and with `pepk_public_key` also `uwulock-play-signing-key.zip`: the release key
encrypted for Google. Built without the key, signed on a fresh runner, like `android.yml`.

## One-time setup in the Play Console

1. Create the app: UwULock, German, app, free.
2. Testing → Internal testing → Testers: create a list with your own Google account, copy
   the opt-in link.
3. Create a release. At "App integrity", choose *Use a different key* → *Export and upload a
   key from Java keystore*, download `encryption_public_key.pem`, run the workflow with it,
   upload `uwulock-play-signing-key.zip` there. Then upload the `.aab`.
4. Open the opt-in link on the phone, install from Play.

Internal testing has no review; the app-content declarations (privacy, data safety, rating)
are only needed for closed testing and production. A personal developer account needs a
closed test with 12 testers for 14 days before production.

## Later

- Play build without the "new versions on GitHub" page (`store` feature), like the Mac App Store.
- Upload from CI through the Play Developer API (service account), beta tags → closed testing.
- Store listing, privacy URL, data safety, demo account for the reviewers.
