// Builds the extension's crypto (crates/uwulock-wasm) for the browser, into
// src/wasm/pkg:
//
//   node scripts/build-wasm.mjs
//
// Needs the wasm32-unknown-unknown target (rustup target add
// wasm32-unknown-unknown) and wasm-bindgen in the version
// crates/uwulock-wasm/Cargo.toml pins (cargo install wasm-bindgen-cli
// --version <it>).

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const extension = join(dirname(fileURLToPath(import.meta.url)), '..');
const root = join(extension, '../..');
const crate = join(root, 'crates/uwulock-wasm');
const run = (command, args, cwd) => execFileSync(command, args, { cwd, stdio: 'inherit' });

const pinned = readFileSync(join(crate, 'Cargo.toml'), 'utf8').match(
  /wasm-bindgen = "=([^"]+)"/,
)?.[1];
const installed = execFileSync('wasm-bindgen', ['--version'], { encoding: 'utf8' })
  .trim()
  .split(' ')[1];
if (pinned && installed !== pinned) {
  console.error(`wasm-bindgen ${installed} is installed, but crates/uwulock-wasm needs ${pinned}:`);
  console.error(`  cargo install wasm-bindgen-cli --version ${pinned} --locked`);
  process.exit(1);
}

run(
  'cargo',
  ['build', '-p', 'uwulock-wasm', '--release', '--locked', '--target', 'wasm32-unknown-unknown'],
  root,
);
// Where cargo put it: the workspace's target, unless CARGO_TARGET_DIR says otherwise.
const target = resolve(root, process.env.CARGO_TARGET_DIR || 'target');
run(
  'wasm-bindgen',
  [
    '--target',
    'web',
    '--out-dir',
    join(extension, 'src/wasm/pkg'),
    '--out-name',
    'core',
    join(target, 'wasm32-unknown-unknown/release/uwulock_wasm.wasm'),
  ],
  root,
);
