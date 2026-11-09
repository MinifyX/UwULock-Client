# Release notes

One file per version, named after it: `0.1.0.json`, `0.1.0-beta.1.json`. `pnpm release` refuses to run
without it. The text appears under "Was ist neu?" in UwULock's update hint and on the GitHub release page.

```json
{
  "de": "- Kurze, verständliche Punkte\n- Was Leute merken, nicht wie es gebaut ist",
  "en": "- Short, plain points\n- What people notice, not how it's built"
}
```

## Releasing a version

1. Set the version in `Cargo.toml` (workspace), the `tauri.conf.json` of `apps/desktop` and `apps/setup`,
   and the `package.json` files.
2. Add `release-notes/<version>.json`.
3. Commit, tag `v<version>` and push both. The tag starts `.github/workflows/installers.yml`, which
   checks the workspace on Linux and builds every Windows and Linux setup — unsigned (CI holds no
   update key) — and `.github/workflows/apple.yml`, which builds the native Apple app: the iPhone IPA
   (unsigned, plus the App Store build to TestFlight), the Mac App Store `.pkg` (TestFlight), the
   macOS disk image and the migration setup that moves the Tauri app on a Mac over to it. The disk
   image is signed with the Developer ID and notarized when the secrets `APPLE_DEVELOPER_ID_KEY` (the
   private key, PEM) and `APPLE_DEVELOPER_ID_CERT` (the "Developer ID Application" certificate, PEM
   or base64 DER) are set, next to the App Store Connect key (`ASC_*`, docs/app-store.md). Without
   them it stays ad-hoc signed, its `UwULock-macos.json` says `ad-hoc`, and `pnpm release` leaves the
   "notarized" line out of the notes.
4. Run `pnpm release` on the Mac with the signing keys (the Tauri update key, and the Sparkle key in
   the login keychain, docs/app-store.md), with the tag checked out.

`pnpm release` waits for the tag's CI run and downloads what it built: the Windows setups for x64
(`UwULock-windows-x64-setup.exe`) and ARM, the universal macOS disk image (signed for Sparkle here,
into `appcast-stable.xml` / `appcast-beta.xml`), the migration setup, the iPhone IPA, and
for Linux x64 and arm64 the `.deb`, `.rpm` and portable `.tar.gz`, plus the x64 setup AppImage for
copies the old Linux setup installed, and the browser extension (`UwULock-extension-chromium.zip`,
`UwULock-extension-firefox.xpi`, built by `extension.yml`, which the tag's run calls). With `--build-windows` it builds the Windows x64 setup on this
machine instead (`pnpm build:setup`, on Windows only), with `--no-build` it takes the one already in
`target/installers`. It signs every file the updater runs —
each as a copy under the versioned name installed apps check for (`UwULock-Setup-<version>.exe`,
`UwULock-<version>-linux-x86_64.deb`, …), while the release carries the same bytes under names without a
version — checks each signature against the key in `tauri.conf.json`, creates the GitHub release with
all of it and a `SHA256SUMS.txt`, and writes the update feeds (Windows, macOS, the Linux setup and the
Linux packages) to the `updates` branch. It waits until everything is online and checks it, then writes
the AUR package `uwulock-bin` (`scripts/aur.mjs`) to `target/aur/uwulock-bin` — or commits and pushes
it from the checkout `UWULOCK_AUR_DIR` points at. CI's `aur.yml` pushes it too once the secret
`AUR_SSH_PRIVATE_KEY` is set. If CI can't build for some reason, `pnpm release --windows-only`
builds Windows x64 on this machine (on Windows) and publishes it alone.

It needs the update signing key, either as `TAURI_SIGNING_PRIVATE_KEY` +
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` or as a folder with `uwulock-update.key` and `PASSWORT.txt` in
`UWULOCK_UPDATE_KEY_DIR` (default: `Documents\UwULock-Update-Schluessel`). The key never goes into this
repository, and the app and setup builds never see it: only the signing step does. Losing it means
installed apps can't take updates any more, so keep a copy somewhere safe.

## Channels

Versions with a suffix (`-beta.1`) go to the Beta feed only; plain versions go to Stable and Beta. An
installed beta starts on the Beta channel, everything else on Stable; Settings → Updates switches.
