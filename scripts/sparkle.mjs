// Sparkle's EdDSA signatures for the Mac's disk image (the native app's Direct build updates
// itself with Sparkle; scripts/release.mjs signs every disk image it publishes). docs/app-store.md.
//
//   node scripts/sparkle.mjs sign <file>             prints `<signature> <length>`
//   node scripts/sparkle.mjs verify <file> <signature>
//   node scripts/sparkle.mjs public-key              the public key of the private key in use
//
// The private key, first found wins:
//   SPARKLE_PRIVATE_KEY   the key as `generate_keys --account app.uwulock -x <file>` exports it
//                         (base64), e.g. as a CI secret
//   SPARKLE_KEY_FILE      a file holding that
//   the login keychain    on this Mac, through Sparkle's sign_update (account app.uwulock); macOS
//                         asks once whether sign_update may read it ("Always Allow")
// sign_update comes with the Sparkle package Xcode fetched (apps/apple/build/DerivedData*/
// SourcePackages/artifacts/sparkle/Sparkle/bin), or from SPARKLE_BIN (its folder).
//
// Every signature is checked against the public key the app carries (SPARKLE_PUBLIC_KEY in
// apps/apple/Config/Direct.xcconfig) before it is handed out: a key that doesn't match would
// publish an update no installed app accepts.

import { createPrivateKey, createPublicKey, sign, verify } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
export const SPARKLE_ACCOUNT = 'app.uwulock';
const PKCS8_ED25519 = Buffer.from('302e020100300506032b657004220420', 'hex');
const SPKI_ED25519 = Buffer.from('302a300506032b6570032100', 'hex');

/** The public key (base64) the Direct build carries in its Info.plist. */
export function appPublicKey() {
  const xcconfig = readFileSync(join(root, 'apps/apple/Config/Direct.xcconfig'), 'utf8');
  const match = xcconfig.match(/^SPARKLE_PUBLIC_KEY\s*=\s*(\S+)\s*$/m);
  if (!match) throw new Error('No SPARKLE_PUBLIC_KEY in apps/apple/Config/Direct.xcconfig');
  return match[1];
}

function publicKeyObject(base64) {
  const raw = Buffer.from(base64, 'base64');
  if (raw.length !== 32) throw new Error('A Sparkle public key has 32 bytes');
  return createPublicKey({ key: Buffer.concat([SPKI_ED25519, raw]), format: 'der', type: 'spki' });
}

/** The private key from SPARKLE_PRIVATE_KEY or SPARKLE_KEY_FILE, or null (then the keychain). */
function privateKeyObject() {
  let text = process.env.SPARKLE_PRIVATE_KEY;
  if (!text && process.env.SPARKLE_KEY_FILE)
    text = readFileSync(process.env.SPARKLE_KEY_FILE, 'utf8');
  if (!text) return null;
  const raw = Buffer.from(text.trim(), 'base64');
  // generate_keys exports the 32-byte seed; older versions wrote seed + public key (64 bytes).
  if (raw.length !== 32 && raw.length !== 64)
    throw new Error(`The Sparkle private key has ${raw.length} bytes, not 32 (generate_keys -x)`);
  return createPrivateKey({
    key: Buffer.concat([PKCS8_ED25519, raw.subarray(0, 32)]),
    format: 'der',
    type: 'pkcs8',
  });
}

function signUpdateBinary() {
  const candidates = [];
  if (process.env.SPARKLE_BIN) candidates.push(join(process.env.SPARKLE_BIN, 'sign_update'));
  const build = join(root, 'apps/apple/build');
  if (existsSync(build)) {
    for (const entry of readdirSync(build)) {
      if (entry.startsWith('DerivedData'))
        candidates.push(
          join(build, entry, 'SourcePackages/artifacts/sparkle/Sparkle/bin/sign_update'),
        );
    }
  }
  const found = candidates.find((path) => existsSync(path));
  if (!found)
    throw new Error(
      'No Sparkle key: set SPARKLE_PRIVATE_KEY or SPARKLE_KEY_FILE, or build the Direct app once ' +
        '(scripts/apple-build.sh direct) so sign_update is there, or point SPARKLE_BIN at it.',
    );
  return found;
}

/** The public key (base64) of the private key in use; null when it lives in the keychain. */
export function signingPublicKey() {
  const key = privateKeyObject();
  if (!key) return null;
  return createPublicKey(key)
    .export({ format: 'der', type: 'spki' })
    .subarray(12)
    .toString('base64');
}

export function verifyFile(file, signature, publicKey = appPublicKey()) {
  return verify(
    null,
    readFileSync(file),
    publicKeyObject(publicKey),
    Buffer.from(signature, 'base64'),
  );
}

/** Signs <file>: `{ signature, length }`, checked against the app's public key. */
export function signFile(file) {
  const length = statSync(file).size;
  const key = privateKeyObject();
  let signature;
  if (key) {
    signature = sign(null, readFileSync(file), key).toString('base64');
  } else {
    if (process.platform !== 'darwin')
      throw new Error('No SPARKLE_PRIVATE_KEY or SPARKLE_KEY_FILE, and no keychain off macOS');
    console.log('Sparkle: signing with the key in the login keychain (macOS may ask once)');
    signature = execFileSync(signUpdateBinary(), ['--account', SPARKLE_ACCOUNT, '-p', file], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'inherit'],
    }).trim();
  }
  if (!verifyFile(file, signature))
    throw new Error(
      `The Sparkle signature of ${file} doesn't match SPARKLE_PUBLIC_KEY in Direct.xcconfig: wrong key?`,
    );
  return { signature, length };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [command, file, signature] = process.argv.slice(2);
  try {
    if (command === 'sign' && file) {
      const result = signFile(file);
      console.log(`${result.signature} ${result.length}`);
    } else if (command === 'verify' && file && signature) {
      const ok = verifyFile(file, signature);
      console.log(ok ? 'valid' : 'INVALID');
      process.exit(ok ? 0 : 1);
    } else if (command === 'public-key') {
      const key = signingPublicKey();
      const app = appPublicKey();
      console.log(key ?? `(keychain account ${SPARKLE_ACCOUNT}) app: ${app}`);
      if (key && key !== app) {
        console.error(`Doesn't match the app's key ${app}`);
        process.exit(1);
      }
    } else {
      console.error(
        'Usage: node scripts/sparkle.mjs sign <file> | verify <file> <signature> | public-key',
      );
      process.exit(2);
    }
  } catch (error) {
    console.error(`::error::${error.message}`);
    process.exit(1);
  }
}
