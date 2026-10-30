// Gets the app ready for an Android build (iOS has the native app, apps/apple):
//
//   node scripts/mobile.mjs prepare   (pnpm mobile:prepare)
//
// The app's library is only an rlib, so desktop builds and tests link one
// library less (apps/desktop/src-tauri/Cargo.toml). Android loads it as a
// shared library (cdylib): this adds it, in the working tree only. The CI
// workflows run it before every Android build; locally, run it once before
// `pnpm tauri android dev`, and `git checkout apps/desktop/src-tauri/Cargo.toml`
// afterwards — never commit the change.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const manifest = join(root, 'apps/desktop/src-tauri/Cargo.toml');
const DESKTOP = 'crate-type = ["rlib"]';
const MOBILE = 'crate-type = ["cdylib", "rlib"]';

const command = process.argv[2];
if (command !== 'prepare') {
  console.error('Usage: node scripts/mobile.mjs prepare');
  process.exit(2);
}

const text = readFileSync(manifest, 'utf8');
if (text.includes(MOBILE)) {
  console.log('The app library is already built for phones too.');
} else if (text.includes(DESKTOP)) {
  writeFileSync(manifest, text.replace(DESKTOP, MOBILE));
  console.log('The app library is now also built as cdylib (Android).');
} else {
  console.error(
    `No ${DESKTOP} in ${manifest}: the [lib] section changed, update scripts/mobile.mjs.`,
  );
  process.exit(1);
}
