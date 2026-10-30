# UwULock-Client

Passwortmanager der UwUSuite (Tauri 2): Client für Vaultwarden/Bitwarden, v0.7.0-beta.1. Allgemeine Befehle/CI-Regeln siehe `~/Projects/CLAUDE.md`.

## Layout

- `crates/uwulock-bitwarden`: Protokoll + Krypto (Bitwarden-kompatibel)
- `crates/uwulock-core`: gemeinsame Kernlogik
- `crates/uwulock-authenticator`: Passkeys; `crates/uwulock-uhid-broker`: UHID-Broker
- `crates/uwulock-wasm`: Krypto für die Browser-Extension (wasm-bindgen)
- `crates/tauri-plugin-uwulock-mobile`: Mobile-Plugin (iOS/Android)
- `apps/desktop` (React/Vite + `src-tauri`), `apps/setup` (Installer-UI + `src-tauri`), `apps/extension` (Browser-Extension)
- `scripts/`: Release-, Signier-, Mobile- und Smoke-Skripte; `packaging/aur`, `brand/`

## Befehle

- Repo-Lint: `pnpm lint` (`prettier --check .` + `node scripts/check-i18n.mjs`; i18n-Check ist billig und läuft einzeln)
- Desktop-Tests: `pnpm --filter @uwulock/desktop test` (`node --test`); Extension: `pnpm --filter @uwulock/extension test` (vitest)
- Extension-WASM: `node apps/extension/scripts/build-wasm.mjs` (braucht Target `wasm32-unknown-unknown` + passendes wasm-bindgen), `build:all` baut WASM + Extension
- e2e-Skripte: `apps/desktop/e2e`, `apps/extension/e2e` (Browser nur per Docker, siehe `~/Projects/CLAUDE.md`)
- Dev-UIs: `.claude/launch.json` (`uwulock-desktop-ui` Port 1420, `uwulock-setup-ui` Port 1430)

## Fallen

- Nie von Hand bearbeiten: `Cargo.lock`, `pnpm-lock.yaml`, `apps/*/src-tauri/gen/schemas/`, `apps/*/src-tauri/gen/apple/` (gitignored, generiert)
- `@uwusuite/design` kommt als Release-Tarball-URL (aktuell 1.8.1 in `apps/desktop/package.json`); UI-Änderungen zuerst in UwUSuite-Design
- Versionen müssen synchron sein: `Cargo.toml` (Workspace), `tauri.conf.json` von `apps/desktop` und `apps/setup`, alle `package.json`

## Plan/Stand

- Einstieg: `KONZEPT.md`, `docs/roadmap.md`, `docs/architecture.md`; weitere Doku in `docs/` (passkeys, extension, mobile, install …)
- Release-Notes: `release-notes/<version>.json` (`de`/`en`), Format in `release-notes/README.md`

## Release

- Version überall setzen, `release-notes/<version>.json` anlegen, committen, Tag `v<version>` pushen: `.github/workflows/installers.yml` baut alle Setups unsigniert
- Danach `pnpm release` (`scripts/release.mjs`): holt CI-Artefakte, signiert den Updater lokal (Schlüssel via `TAURI_SIGNING_PRIVATE_KEY*` oder `UWULOCK_UPDATE_KEY_DIR`), veröffentlicht. Braucht sauberen Tree, gepushten Tag, `gh`

## Security

- Kritische Pfade: `crates/uwulock-bitwarden` (Protokoll/Krypto), `crates/uwulock-core`, `crates/uwulock-authenticator` (Passkeys), `crates/uwulock-uhid-broker`, `crates/uwulock-wasm`, Extension-Messaging in `apps/extension/src`
- Vor jedem Feature-Release den globalen Subagent `security-reviewer` über den Bereich seit dem letzten Tag laufen lassen; Ergebnis nach `docs/security-review-<version>.md` (Schema der vorhandenen: `security-review-0.5.md`; Befunde aus `security-review-2026-09.md` abgleichen)
