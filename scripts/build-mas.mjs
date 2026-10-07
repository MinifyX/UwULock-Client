// Builds UwULock for the Mac App Store: a universal app (Apple silicon and
// Intel) with the AutoFill extension for passkeys and the Safari extension,
// and signs it into the installer package App Store Connect takes.
// docs/app-store.md.
//
//   node scripts/build-mas.mjs            build, unsigned
//   node scripts/build-mas.mjs --sign <UwULock.app>
//                                         sign an app this script built
//                                         (elsewhere) and package it
//
// The build is the app without its updater (`--no-default-features --features
// store`, src-tauri/Cargo.toml) with tauri.mas.conf.json merged over
// tauri.conf.json: bundle id app.uwulock — the iPhone app's, so both are one
// app in App Store Connect and the extension app.uwulock.passkeys fits under
// it, as does the Safari extension app.uwulock.safari (scripts/macos-safari.sh,
// from apps/extension/dist/safari, which has to be built first) — the privacy
// manifest, no update feed. Both halves are built side by
// side and joined with lipo, the way scripts/build-setup.mjs builds the disk
// image (and with the same Cargo caches). Nothing is signed here; the App Group
// and Keychain group paths need the team's id at build time all the same:
//
//   UWULOCK_APPLE_TEAM_ID   the ten-character team id
//
// Signing (`--sign`), all from the environment:
//
//   APPLE_SIGNING_IDENTITY     the "Apple Distribution" certificate (name or SHA-1)
//   APPLE_INSTALLER_IDENTITY   the "3rd Party Mac Developer Installer" certificate
//   APPLE_TEAM_ID              the team id
//   MAS_PROFILES               folder with app.uwulock.provisionprofile,
//                              app.uwulock.passkeys.provisionprofile and
//                              app.uwulock.safari.provisionprofile
//                              (node scripts/asc.mjs profiles macos …)
//   MAS_BUILD_NUMBER           CFBundleVersion; must grow with every upload
//
// What comes out: target/universal-apple-darwin/release/bundle/macos/UwULock.app,
// and with --sign target/release/UwULock-<version>-<build>-mas-universal.pkg.

import { execFileSync, spawn } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const root = join(fileURLToPath(import.meta.url), '..', '..');
const tauriDir = join(root, 'apps/desktop/src-tauri');
const BUNDLE_ID = 'app.uwulock';
const EXTENSION_ID = 'app.uwulock.passkeys';
const SAFARI_ID = 'app.uwulock.safari';
const BINARY = 'uwulock-desktop';

function fail(message) {
  console.error(`\n✗ ${message}`);
  process.exit(1);
}

function run(command, args, env = {}) {
  execFileSync(command, args, { cwd: root, stdio: 'inherit', env: { ...process.env, ...env } });
}

function runAsync(command, args, env = {}) {
  return new Promise((done, failed) => {
    const child = spawn(command, args, {
      cwd: root,
      stdio: 'inherit',
      env: { ...process.env, ...env },
    });
    child.on('error', failed);
    child.on('exit', (code) =>
      code === 0 ? done() : failed(new Error(`${command} ${args.join(' ')} exited with ${code}`)),
    );
  });
}

let options;
try {
  ({ values: options } = parseArgs({ options: { sign: { type: 'string' } } }));
} catch (error) {
  fail(`${error.message}\n  Usage: node scripts/build-mas.mjs [--sign <UwULock.app>]`);
}
if (process.platform !== 'darwin') fail('The Mac App Store build needs a Mac.');

// The update-signing key has no business near this build: it has no updater,
// and the build runs every build script in the dependency tree.
delete process.env.TAURI_SIGNING_PRIVATE_KEY;
delete process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD;

const conf = JSON.parse(readFileSync(join(tauriDir, 'tauri.conf.json'), 'utf8'));
/** App Store Connect wants three numbers: 0.5.0-beta.2 goes up as 0.5.0. */
const marketingVersion = conf.version.replace(/[-+].*$/, '');
const env = process.env;
const scratch = mkdtempSync(join(tmpdir(), 'uwulock-mas-'));

try {
  if (options.sign) await sign(resolve(options.sign));
  else await check(await build());
} finally {
  rmSync(scratch, { recursive: true, force: true });
}

async function build() {
  const team = env.UWULOCK_APPLE_TEAM_ID;
  if (!team) fail('UWULOCK_APPLE_TEAM_ID has to be set: the App Group paths carry the team id.');

  console.log(`\n▸ The pages`);
  run('pnpm', ['--filter', '@uwulock/desktop', 'build']);

  console.log('\n▸ The AutoFill extension');
  run('bash', ['scripts/macos-passkeys.sh', conf.version, scratch]);

  console.log('\n▸ The Safari extension');
  run('bash', ['scripts/macos-safari.sh', conf.version, scratch, SAFARI_ID]);

  // What differs per build, as a file: quoting JSON on a command line is
  // different in every shell.
  const overrides = join(scratch, 'tauri.mas.build.json');
  writeFileSync(
    overrides,
    JSON.stringify({
      version: marketingVersion,
      build: { beforeBuildCommand: null },
      bundle: {
        macOS: {
          files: {
            'PlugIns/UwULockPasskeys.appex': join(scratch, 'UwULockPasskeys.appex'),
            'PlugIns/UwULockSafari.appex': join(scratch, 'UwULockSafari.appex'),
          },
        },
      },
    }),
  );
  const config = ['--config', join(tauriDir, 'tauri.mas.conf.json'), '--config', overrides];
  const cargo = ['--features', 'store', '--', '--no-default-features'];

  console.log(`\n▸ UwULock ${marketingVersion} for the Mac App Store, both halves`);
  const tauri = (...args) => ['--filter', '@uwulock/desktop', 'tauri', ...args];
  const intel = join(root, 'target', 'x86_64-build');
  await Promise.all([
    runAsync(
      'pnpm',
      tauri('build', '--no-bundle', '--target', 'aarch64-apple-darwin', ...config, ...cargo),
    ),
    runAsync(
      'pnpm',
      tauri('build', '--no-bundle', '--target', 'x86_64-apple-darwin', ...config, ...cargo),
      { CARGO_TARGET_DIR: intel },
    ),
  ]);
  const release = join(root, 'target', 'universal-apple-darwin', 'release');
  mkdirSync(release, { recursive: true });
  run('lipo', [
    '-create',
    '-output',
    join(release, BINARY),
    join(root, 'target', 'aarch64-apple-darwin', 'release', BINARY),
    join(intel, 'x86_64-apple-darwin', 'release', BINARY),
  ]);
  const bundle = join(release, 'bundle', 'macos', `${conf.productName}.app`);
  rmSync(bundle, { recursive: true, force: true });
  run('pnpm', tauri('bundle', '--bundles', 'app', '--target', 'universal-apple-darwin', ...config));
  if (!existsSync(bundle)) fail(`The build left no ${bundle}.`);
  return bundle;
}

function plist(file, key) {
  try {
    return execFileSync('/usr/libexec/PlistBuddy', ['-c', `Print :${key}`, file], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
  } catch {
    return null;
  }
}

function archs(file) {
  return execFileSync('lipo', ['-archs', file], { encoding: 'utf8' }).trim().split(/\s+/);
}

/** What App Store Connect or App Review would otherwise be the first to notice. */
function check(app) {
  const contents = join(app, 'Contents');
  const info = join(contents, 'Info.plist');
  const [exe] = readdirSync(join(contents, 'MacOS'));
  const found = archs(join(contents, 'MacOS', exe));
  if (!found.includes('arm64') || !found.includes('x86_64'))
    fail(`${exe} carries ${found.join(' ')}, not arm64 and x86_64.`);

  const expect = {
    CFBundleIdentifier: BUNDLE_ID,
    CFBundleShortVersionString: marketingVersion,
    LSApplicationCategoryType: 'public.app-category.utilities',
  };
  for (const [key, value] of Object.entries(expect)) {
    if (plist(info, key) !== value)
      fail(`Info.plist: ${key} is ${plist(info, key)}, not ${value}.`);
  }
  if (!plist(info, 'LSMinimumSystemVersion')) fail('Info.plist has no LSMinimumSystemVersion.');
  if (!existsSync(join(contents, 'Resources', 'PrivacyInfo.xcprivacy')))
    fail('The privacy manifest is missing.');

  const appex = join(contents, 'PlugIns', 'UwULockPasskeys.appex');
  if (!existsSync(appex)) fail('The AutoFill extension is missing.');
  if (plist(join(appex, 'Contents', 'Info.plist'), 'CFBundleIdentifier') !== EXTENSION_ID)
    fail(`The extension isn't ${EXTENSION_ID}.`);
  const appexArchs = archs(join(appex, 'Contents', 'MacOS', 'UwULockPasskeys'));
  if (!appexArchs.includes('arm64') || !appexArchs.includes('x86_64'))
    fail(`The extension carries ${appexArchs.join(' ')}, not arm64 and x86_64.`);

  const safari = join(contents, 'PlugIns', 'UwULockSafari.appex');
  if (!existsSync(safari)) fail('The Safari extension is missing.');
  if (plist(join(safari, 'Contents', 'Info.plist'), 'CFBundleIdentifier') !== SAFARI_ID)
    fail(`The Safari extension isn't ${SAFARI_ID}.`);
  if (!existsSync(join(safari, 'Contents', 'Resources', 'manifest.json')))
    fail('The Safari extension carries no manifest.json: was apps/extension built?');
  const safariArchs = archs(join(safari, 'Contents', 'MacOS', 'UwULockSafari'));
  if (!safariArchs.includes('arm64') || !safariArchs.includes('x86_64'))
    fail(`The Safari extension carries ${safariArchs.join(' ')}, not arm64 and x86_64.`);

  // The updater must be gone, not merely unused: its feed in the program would
  // be a way to update outside the store as far as App Review can tell.
  const binary = readFileSync(join(contents, 'MacOS', exe));
  if (binary.includes('raw.githubusercontent.com/MinifyX/UwULock-Client/updates'))
    fail('The update feed is still in the program: was it built with --no-default-features?');
  const team = env.UWULOCK_APPLE_TEAM_ID ?? env.APPLE_TEAM_ID;
  if (team && !binary.includes(team))
    fail(`The program doesn't know the team ${team}: built without UWULOCK_APPLE_TEAM_ID?`);
  console.log(
    `  ${exe}: ${found.join(' ')}, ${BUNDLE_ID} ${marketingVersion}, both extensions inside`,
  );
}

function sign(app) {
  for (const name of [
    'APPLE_SIGNING_IDENTITY',
    'APPLE_INSTALLER_IDENTITY',
    'APPLE_TEAM_ID',
    'MAS_PROFILES',
    'MAS_BUILD_NUMBER',
  ]) {
    if (!env[name]) fail(`--sign needs ${name}.`);
  }
  if (!/^\d+(\.\d+){0,2}$/.test(env.MAS_BUILD_NUMBER))
    fail(`MAS_BUILD_NUMBER "${env.MAS_BUILD_NUMBER}" is not one to three numbers.`);
  const team = env.APPLE_TEAM_ID;
  check(app);

  const contents = join(app, 'Contents');
  const appex = join(contents, 'PlugIns', 'UwULockPasskeys.appex');
  const safari = join(contents, 'PlugIns', 'UwULockSafari.appex');
  for (const info of [
    join(contents, 'Info.plist'),
    join(appex, 'Contents', 'Info.plist'),
    join(safari, 'Contents', 'Info.plist'),
  ]) {
    run('/usr/libexec/PlistBuddy', ['-c', `Set :CFBundleVersion ${env.MAS_BUILD_NUMBER}`, info]);
    run('/usr/libexec/PlistBuddy', [
      '-c',
      `Set :CFBundleShortVersionString ${marketingVersion}`,
      info,
    ]);
  }

  // The profiles inside, the entitlements completed with the team.
  copyFileSync(
    join(env.MAS_PROFILES, `${BUNDLE_ID}.provisionprofile`),
    join(contents, 'embedded.provisionprofile'),
  );
  copyFileSync(
    join(env.MAS_PROFILES, `${EXTENSION_ID}.provisionprofile`),
    join(appex, 'Contents', 'embedded.provisionprofile'),
  );
  copyFileSync(
    join(env.MAS_PROFILES, `${SAFARI_ID}.provisionprofile`),
    join(safari, 'Contents', 'embedded.provisionprofile'),
  );
  const entitlements = (source, id) => {
    const file = join(scratch, `${id}.entitlements`);
    writeFileSync(file, readFileSync(source, 'utf8').replaceAll('TEAMID', team));
    for (const [key, value] of [
      ['com.apple.application-identifier', `${team}.${id}`],
      ['com.apple.developer.team-identifier', team],
    ]) {
      run('/usr/libexec/PlistBuddy', ['-c', `Add :${key} string ${value}`, file]);
    }
    return file;
  };
  const appEntitlements = entitlements(
    join(tauriDir, 'macos', 'Entitlements.mas.plist'),
    BUNDLE_ID,
  );
  const appexEntitlements = entitlements(
    join(tauriDir, 'apple', 'PasskeyProvider', 'UwULockPasskeys-macOS.entitlements'),
    EXTENSION_ID,
  );
  const safariEntitlements = entitlements(
    join(tauriDir, 'apple', 'SafariExtension', 'UwULockSafari-macOS.entitlements'),
    SAFARI_ID,
  );

  console.log('\n▸ Signing');
  const codesign = (entitlementsFile, target) =>
    run('codesign', [
      '--force',
      '--timestamp',
      '--options',
      'runtime',
      '--entitlements',
      entitlementsFile,
      '--sign',
      env.APPLE_SIGNING_IDENTITY,
      target,
    ]);
  // Inside out: the extensions, then the app, whose signature seals them.
  codesign(appexEntitlements, appex);
  codesign(safariEntitlements, safari);
  codesign(appEntitlements, app);
  run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', app]);
  run('codesign', ['-d', '--entitlements', '-', '--xml', app]);
  console.log();

  console.log('\n▸ Packaging');
  const out = join(root, 'target', 'release');
  mkdirSync(out, { recursive: true });
  const pkg = join(out, `UwULock-${marketingVersion}-${env.MAS_BUILD_NUMBER}-mas-universal.pkg`);
  rmSync(pkg, { force: true });
  run('productbuild', [
    '--component',
    app,
    '/Applications',
    '--sign',
    env.APPLE_INSTALLER_IDENTITY,
    pkg,
  ]);
  run('pkgutil', ['--check-signature', pkg]);
  console.log(`\n✧ ${pkg}`);
}
