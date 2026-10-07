// Signs the disk image's apps with the Developer ID and has Apple notarize them, on a CI machine
// that builds nothing (installers.yml: `macos-sign-app`, `macos-sign-setup`). The builds
// (scripts/build-setup.mjs --part app / --part setup) leave everything ad-hoc signed; this
// makes it Apple's:
//
//   node scripts/macos-sign.mjs app <UwULock.app>
//       The extensions inside (the Safari extension), then the app: hardened runtime, secure
//       timestamp. Notarized and the ticket stapled to the app, before the setup packs it — the
//       setup carries it as data, which the notary service can't look into.
//   node scripts/macos-sign.mjs setup <folder>
//       <folder> holds "UwULock Setup.app" and UwULock-update-macos-universal (what the updater
//       runs). Both signed; the disk image made anew around the setup (as Tauri's: the setup
//       and a link to Applications), signed, notarized and stapled. Writes
//       UwULock-macos-universal.dmg into <folder>.
//
// From the environment: APPLE_SIGNING_IDENTITY (the "Developer ID Application" certificate in
// a keychain codesign reaches: scripts/apple-ci.sh developer-id), and the App Store Connect key
// for notarytool: ASC_KEY_ID, ASC_ISSUER_ID, ASC_KEY_PATH (scripts/apple-ci.sh api-key).
// docs/install.md.

import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(fileURLToPath(import.meta.url), '..', '..');
const env = process.env;

/** Entitlements per extension: app extensions are sandboxed, the app itself is not. */
const ENTITLEMENTS = {
  'UwULockSafari.appex': join(
    root,
    'apps/desktop/src-tauri/apple/SafariExtension/UwULockSafari-macOS.entitlements',
  ),
};

function fail(message) {
  console.error(`::error::${message}`);
  process.exit(1);
}

function run(command, args, options = {}) {
  return execFileSync(command, args, { stdio: 'inherit', ...options });
}

for (const name of ['APPLE_SIGNING_IDENTITY', 'ASC_KEY_ID', 'ASC_ISSUER_ID', 'ASC_KEY_PATH']) {
  if (!env[name]) fail(`${name} has to be set.`);
}

function sign(path, { entitlements, identifier } = {}) {
  run('codesign', [
    '--force',
    '--options',
    'runtime',
    '--timestamp',
    ...(entitlements ? ['--entitlements', entitlements] : []),
    ...(identifier ? ['--identifier', identifier] : []),
    '--sign',
    env.APPLE_SIGNING_IDENTITY,
    path,
  ]);
}

/** Apple's notary service, waited for; its log when it says no. */
function notarize(file) {
  const auth = [
    '--key',
    env.ASC_KEY_PATH,
    '--key-id',
    env.ASC_KEY_ID,
    '--issuer',
    env.ASC_ISSUER_ID,
  ];
  console.log(`\n▸ Notarizing ${basename(file)}`);
  // notarytool exits with an error when Apple says no; its answer is on stdout all the same.
  const submitted = spawnSync(
    'xcrun',
    [
      'notarytool',
      'submit',
      file,
      ...auth,
      '--wait',
      '--timeout',
      '45m',
      '--output-format',
      'json',
    ],
    { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] },
  );
  let result;
  try {
    result = JSON.parse(submitted.stdout);
  } catch {
    fail(`notarytool answered no JSON (exit ${submitted.status}): ${submitted.stdout}`);
  }
  console.log(`  ${result.id}: ${result.status} ${result.message ?? ''}`);
  if (result.status !== 'Accepted') {
    if (result.id)
      spawnSync('xcrun', ['notarytool', 'log', result.id, ...auth], { stdio: 'inherit' });
    fail(`Apple didn't notarize ${basename(file)}: ${result.status}`);
  }
}

function signApp(app) {
  if (!existsSync(join(app, 'Contents', 'Info.plist'))) fail(`${app} is no app.`);
  const plugins = join(app, 'Contents', 'PlugIns');
  // Inside out: the extensions, then the app, whose signature seals them.
  for (const name of existsSync(plugins) ? readdirSync(plugins) : []) {
    const entitlements = ENTITLEMENTS[name];
    if (!entitlements) fail(`No entitlements known for ${name}: add it to scripts/macos-sign.mjs.`);
    sign(join(plugins, name), { entitlements });
  }
  sign(app);
  run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', app]);
}

const [command, target] = process.argv.slice(2);
const scratch = mkdtempSync(join(tmpdir(), 'uwulock-sign-'));
try {
  if (command === 'app' && target) {
    const app = resolve(target);
    console.log(`▸ Signing ${basename(app)}`);
    signApp(app);
    const zip = join(scratch, 'UwULock.zip');
    run('ditto', ['-c', '-k', '--keepParent', app, zip]);
    notarize(zip);
    run('xcrun', ['stapler', 'staple', app]);
    run('xcrun', ['stapler', 'validate', app]);
    run('spctl', ['--assess', '--type', 'execute', '--verbose=2', app]);
  } else if (command === 'setup' && target) {
    const folder = resolve(target);
    const setup = join(folder, 'UwULock Setup.app');
    const update = join(folder, 'UwULock-update-macos-universal');
    for (const path of [setup, update]) if (!existsSync(path)) fail(`${path} is missing.`);

    console.log('▸ Signing the setup');
    signApp(setup);
    // The updater runs this one on its own, outside a bundle: it gets the setup's identifier.
    sign(update, { identifier: 'app.uwulock.setup' });
    run('codesign', ['--verify', '--strict', '--verbose=2', update]);

    console.log('\n▸ The disk image');
    const staging = join(scratch, 'image');
    run('mkdir', ['-p', staging]);
    run('ditto', [setup, join(staging, 'UwULock Setup.app')]);
    symlinkSync('/Applications', join(staging, 'Applications'));
    const dmg = join(folder, 'UwULock-macos-universal.dmg');
    rmSync(dmg, { force: true });
    run('hdiutil', [
      'create',
      '-volname',
      'UwULock Setup',
      '-srcfolder',
      staging,
      '-fs',
      'HFS+',
      '-format',
      'UDZO',
      '-ov',
      dmg,
    ]);
    run('codesign', ['--force', '--timestamp', '--sign', env.APPLE_SIGNING_IDENTITY, dmg]);
    notarize(dmg);
    run('xcrun', ['stapler', 'staple', dmg]);
    run('xcrun', ['stapler', 'validate', dmg]);
    run('spctl', [
      '--assess',
      '--type',
      'open',
      '--context',
      'context:primary-signature',
      '--verbose=2',
      dmg,
    ]);
  } else {
    fail('Usage: node scripts/macos-sign.mjs app <UwULock.app> | setup <folder>');
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
