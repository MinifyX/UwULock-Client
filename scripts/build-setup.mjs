// Builds UwULock's downloads for the system it runs on: on Windows and macOS
// the app packed into the setup with Nyu in it, on Linux the app as .deb,
// .rpm and a portable folder.
//
//   pnpm build:setup                       for this machine
//   pnpm build:setup --target <triple>     another architecture (macOS:
//                                          universal-apple-darwin for both
//                                          Intel and Apple silicon, as the
//                                          release has it)
//
// Everything lands in target/installers/, under the names the release
// publishes — no version in them, so a link to the newest release's file
// stays the same forever:
//
//   Windows  UwULock-windows-x64-setup.exe           what people run, and what updates run
//            UwULock-windows-arm64-setup.exe         the same for ARM
//   macOS    UwULock-macos-universal.dmg             what people open
//            UwULock-update-macos-universal          the setup program inside it, for updates
//            (both made anew, signed and notarized, by scripts/macos-sign.mjs
//            in CI when there is a Developer ID)
//   Linux    UwULock-linux-<arch>.deb                the app for apt/dpkg, updates itself
//            UwULock-linux-<arch>.rpm                the app for dnf/zypper/rpm, updates itself
//            UwULock-linux-<arch>-portable.tar.gz    unpack and run, no updates
//            UwULock-update-linux-x64.AppImage       x64 only: the setup, for copies an
//                                                   earlier setup installed; not a download
//
// Nothing here signs anything: `pnpm release` signs what the updater runs,
// under the versioned names installed apps expect, on the machine that holds
// the key. The builds below never see it. Apple's Developer ID signature for
// macOS is scripts/macos-sign.mjs, on machines that build nothing.

import { execFileSync, execSync, spawn } from 'node:child_process';
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const run = (command, env = {}) =>
  execSync(command, { cwd: root, stdio: 'inherit', env: { ...process.env, ...env } });
/** `run`, but side by side with others. */
const runAsync = (command, env = {}) =>
  new Promise((resolve, reject) => {
    const child = spawn(command, {
      cwd: root,
      stdio: 'inherit',
      shell: true,
      env: { ...process.env, ...env },
    });
    child.on('error', reject);
    child.on('exit', (code) =>
      code === 0 ? resolve() : reject(new Error(`${command} exited with ${code}`)),
    );
  });

// Keep the update-signing key out of the builds: they run hundreds of
// third-party build scripts (Cargo build.rs, npm) that would otherwise see it.
delete process.env.TAURI_SIGNING_PRIVATE_KEY;
delete process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD;

const fail = (message) => {
  console.error(`\n✗ ${message}`);
  process.exit(1);
};

// Linux: `--part packages` builds just the .deb and .rpm, `--part setup`
// everything else (AppImage, portable folder, setup), so CI can build the two
// on separate machines at the same time. macOS: `--part app` builds UwULock.app
// alone, into target/installers/macos/, and `--part setup --payload <folder
// with UwULock.app>` the setup around it (and the disk image), so CI can sign
// the app in between (installers.yml). Without it, everything.
const partIndex = process.argv.indexOf('--part');
const part = partIndex > 0 ? process.argv[partIndex + 1] : 'all';
if (!['all', 'packages', 'setup', 'app'].includes(part)) fail(`Unknown --part ${part}`);
if (part === 'app' && process.platform !== 'darwin') fail('--part app is for macOS.');
const payloadIndex = process.argv.indexOf('--payload');
const payloadDir = payloadIndex > 0 ? resolve(process.argv[payloadIndex + 1]) : undefined;
if (process.platform === 'darwin' && part === 'setup' && !payloadDir)
  fail('--part setup on macOS needs --payload <folder with UwULock.app>.');
const targetIndex = process.argv.indexOf('--target');
const target = targetIndex > 0 ? process.argv[targetIndex + 1] : undefined;
const targetArg = target ? ` --target ${target}` : '';

const { version } = JSON.parse(
  readFileSync(join(root, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8'),
);
{
  const setup = JSON.parse(
    readFileSync(join(root, 'apps/setup/src-tauri/tauri.conf.json'), 'utf8'),
  );
  if (setup.version !== version) fail(`apps/setup says ${setup.version}, the app says ${version}.`);
}

const release = join(root, 'target', ...(target ? [target] : []), 'release');
const bundles = join(release, 'bundle');
const out = join(root, 'target', 'installers');
mkdirSync(out, { recursive: true });

/** The architecture as release file names say it. */
function arch() {
  if (target === 'universal-apple-darwin') return 'universal';
  const name = target ?? `${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}-host`;
  return name.startsWith('aarch64') ? 'arm64' : 'x64';
}

/** A Tauri config override, as a file: quoting JSON on a command line is a trap on every shell. */
function configFile(name, config) {
  const path = join(mkdtempSync(join(tmpdir(), 'uwulock-build-')), `${name}.json`);
  writeFileSync(path, JSON.stringify(config));
  return path;
}

function only(dir, test, what) {
  const found = existsSync(dir) ? readdirSync(dir).filter(test) : [];
  if (found.length !== 1) fail(`Expected one ${what} in ${dir}, found ${found.length}.`);
  return join(dir, found[0]);
}

const produced = [];

/**
 * A universal macOS build of one app, both halves at once. Tauri's own
 * `--target universal-apple-darwin` builds one architecture after the other,
 * and most of each build is the final optimisation of the whole program (one
 * codegen unit, fat LTO), which runs on a single core. Side by side they take
 * little longer than one. The Intel half gets a target folder of its own, as
 * two Cargo builds can't share one. Then the two programs are joined with
 * `lipo`, as Tauri does, and bundled from there with the same configuration.
 */
async function universalMac(pkg, binary, bundles, config, env = {}) {
  // The pages once, before both builds: `pnpm build` twice at the same time
  // would write into the same folder.
  run(`pnpm --filter ${pkg} build`);
  const noHook = configFile(`${binary}-universal`, {
    ...config,
    build: { beforeBuildCommand: null },
  });
  const intel = join(root, 'target', 'x86_64-build');
  await Promise.all([
    runAsync(
      `pnpm --filter ${pkg} tauri build --no-bundle --target aarch64-apple-darwin --config "${noHook}"`,
      env,
    ),
    runAsync(
      `pnpm --filter ${pkg} tauri build --no-bundle --target x86_64-apple-darwin --config "${noHook}"`,
      { ...env, CARGO_TARGET_DIR: intel },
    ),
  ]);
  mkdirSync(release, { recursive: true });
  execFileSync(
    'lipo',
    [
      '-create',
      '-output',
      join(release, binary),
      join(root, 'target', 'aarch64-apple-darwin', 'release', binary),
      join(intel, 'x86_64-apple-darwin', 'release', binary),
    ],
    { stdio: 'inherit' },
  );
  run(
    `pnpm --filter ${pkg} tauri bundle --bundles ${bundles} --target universal-apple-darwin --config "${noHook}"`,
    env,
  );
}

if (process.platform === 'win32') {
  console.log(`\n▸ Building UwULock ${version}`);
  run(`pnpm --filter @uwulock/desktop tauri build --no-bundle${targetArg}`);
  const app = join(release, 'uwulock-desktop.exe');
  if (!existsSync(app)) fail(`Missing ${app}`);

  console.log('\n▸ Packing it into the setup');
  run(`pnpm --filter @uwulock/setup tauri build --no-bundle${targetArg}`, {
    UWULOCK_SETUP_PAYLOAD: app,
  });
  const setup = join(out, `UwULock-windows-${arch()}-setup.exe`);
  copyFileSync(join(release, 'uwulock-setup.exe'), setup);
  produced.push(setup);
} else if (process.platform === 'darwin') {
  // Ad-hoc signed: a sealed bundle, which Apple silicon insists on and which
  // keeps the app intact through the setup. With a Developer ID, CI signs and
  // notarizes on machines of their own, between the parts below
  // (scripts/macos-sign.mjs, installers.yml).
  const macOS = { signingIdentity: '-', minimumSystemVersion: '11.0' };
  const universal = target === 'universal-apple-darwin';
  // What the parts hand on: target/installers/macos/UwULock.app (`--part app`),
  // UwULock Setup.app next to it (`--part setup`).
  const handOver = join(out, 'macos');

  let apps = payloadDir;
  const extensions = mkdtempSync(join(tmpdir(), 'uwulock-appex-'));
  if (part !== 'setup') {
    // The AutoFill extension for passkeys: always built, so its Swift stays
    // sound; inside the app only in a build signed with an Apple developer
    // team, the only one where it can reach the vault (docs/passkeys.md).
    console.log('\n▸ Building the AutoFill extension');
    run(`bash scripts/macos-passkeys.sh "${version}" "${extensions}"`);
    // The Safari extension, always (docs/extension.md). Its id starts with the
    // app's, app.uwulock.desktop. Its files come from apps/extension/dist/safari.
    console.log('\n▸ Building the Safari extension');
    run(`bash scripts/macos-safari.sh "${version}" "${extensions}" app.uwulock.desktop.safari`, {
      APPLE_SIGNING_IDENTITY: '',
    });
    const files = { 'PlugIns/UwULockSafari.appex': join(extensions, 'UwULockSafari.appex') };
    if (process.env.UWULOCK_APPLE_TEAM_ID)
      files['PlugIns/UwULockPasskeys.appex'] = join(extensions, 'UwULockPasskeys.appex');

    console.log(`\n▸ Building UwULock ${version}`);
    const appConfig = { bundle: { macOS: { ...macOS, files } } };
    if (universal) {
      await universalMac('@uwulock/desktop', 'uwulock-desktop', 'app', appConfig);
    } else {
      run(
        `pnpm --filter @uwulock/desktop tauri build --bundles app${targetArg} --config "${configFile('app', appConfig)}"`,
      );
    }
    apps = mkdtempSync(join(tmpdir(), 'uwulock-apps-'));
    execFileSync('ditto', [join(bundles, 'macos', 'UwULock.app'), join(apps, 'UwULock.app')]);
    if (part === 'app') {
      rmSync(handOver, { recursive: true, force: true });
      mkdirSync(handOver, { recursive: true });
      execFileSync('ditto', [join(apps, 'UwULock.app'), join(handOver, 'UwULock.app')]);
      produced.push(join(handOver, 'UwULock.app'));
    }
  }

  if (part !== 'app') {
    if (!existsSync(join(apps, 'UwULock.app'))) fail(`No UwULock.app in ${apps}.`);
    console.log('\n▸ Packing it into the setup');
    // The disk image as Tauri makes it, and the setup app on its own for a
    // signed one (scripts/macos-sign.mjs makes that disk image anew).
    const setupConfig = { bundle: { active: true, targets: ['app', 'dmg'], macOS } };
    if (universal) {
      await universalMac('@uwulock/setup', 'uwulock-setup', 'app,dmg', setupConfig, {
        UWULOCK_SETUP_PAYLOAD: apps,
      });
    } else {
      run(
        `pnpm --filter @uwulock/setup tauri build --bundles app,dmg${targetArg} --config "${configFile('setup', setupConfig)}"`,
        {
          UWULOCK_SETUP_PAYLOAD: apps,
        },
      );
    }
    const dmg = join(out, `UwULock-macos-${arch()}.dmg`);
    copyFileSync(
      only(join(bundles, 'dmg'), (name) => name.endsWith('.dmg'), 'disk image'),
      dmg,
    );
    const update = join(out, `UwULock-update-macos-${arch()}`);
    copyFileSync(join(release, 'uwulock-setup'), update);
    produced.push(dmg, update);
    if (part === 'setup') {
      mkdirSync(handOver, { recursive: true });
      const setupApp = join(handOver, 'UwULock Setup.app');
      rmSync(setupApp, { recursive: true, force: true });
      execFileSync('ditto', [join(bundles, 'macos', 'UwULock Setup.app'), setupApp]);
      produced.push(setupApp);
    }
  }
  if (apps !== payloadDir) rmSync(apps, { recursive: true, force: true });
  rmSync(extensions, { recursive: true, force: true });
} else {
  // The AppImage, unpacked, is both the portable folder and what the setup
  // installs: it runs from its AppDir, brings its own WebKit and needs no FUSE.
  const apps = mkdtempSync(join(tmpdir(), 'uwulock-apps-'));
  const unpackAppImage = (image, name) => {
    const work = mkdtempSync(join(tmpdir(), 'uwulock-appimage-'));
    execFileSync('chmod', ['+x', image]);
    execFileSync(image, ['--appimage-extract'], { cwd: work, stdio: 'ignore' });
    renameSync(join(work, 'squashfs-root'), join(apps, name));
    rmSync(work, { recursive: true, force: true });
  };

  if (part !== 'packages') {
    console.log(`\n▸ Building UwULock ${version}`);
    run(`pnpm --filter @uwulock/desktop tauri build --bundles appimage${targetArg}`);
    unpackAppImage(
      only(join(bundles, 'appimage'), (name) => name.endsWith('.AppImage'), 'AppImage of UwULock'),
      'UwULock',
    );
    // The next build bundles into the same folders.
    rmSync(join(bundles, 'appimage'), { recursive: true, force: true });
  }

  // The packages install system-wide, to /usr, as package `uwulock`. Tauri
  // names the package after productName in kebab case, which would make
  // "UwULock" `uw-u-lock`; the menu entry keeps saying UwULock through the
  // template in tauri.conf.json. The app looks for `uwulock` when it checks that
  // dpkg or rpm owns it (apps/desktop/src-tauri/src/updates.rs).
  //
  // The packages also bring the root helper for the virtual security key
  // (crates/uwulock-uhid-broker, docs/passkeys.md), built here: Tauri bundles
  // only the app's own binary.
  if (part !== 'setup') {
    console.log('\n▸ Building the security key helper');
    run(`cargo build --release --locked -p uwulock-uhid-broker${targetArg}`);
    const broker = join(release, 'uwulock-uhid-broker');
    if (!existsSync(broker)) fail(`Missing ${broker}`);
    const brokerFiles = { files: { '/usr/lib/uwulock/uwulock-uhid-broker': broker } };
    console.log(`\n▸ Packaging UwULock ${version} as .deb and .rpm`);
    run(
      `pnpm --filter @uwulock/desktop tauri build --bundles deb,rpm${targetArg} --config "${configFile(
        'packages',
        {
          productName: 'uwulock',
          bundle: { linux: { deb: brokerFiles, rpm: brokerFiles } },
        },
      )}"`,
    );
    const deb = join(out, `UwULock-linux-${arch()}.deb`);
    copyFileSync(
      only(join(bundles, 'deb'), (name) => name.endsWith('.deb'), 'deb'),
      deb,
    );
    const rpm = join(out, `UwULock-linux-${arch()}.rpm`);
    copyFileSync(
      only(join(bundles, 'rpm'), (name) => name.endsWith('.rpm'), 'rpm'),
      rpm,
    );
    produced.push(deb, rpm);
  }

  if (part !== 'packages') {
    // One folder, UwULock/, that runs where it lands. tar, not zip: the AppDir
    // needs its modes and symlinks.
    console.log('\n▸ Packing the portable folder');
    const staging = mkdtempSync(join(tmpdir(), 'uwulock-portable-'));
    const folder = join(staging, 'UwULock');
    execFileSync('cp', ['-a', join(apps, 'UwULock'), folder]);
    const launcher = join(folder, 'uwulock');
    writeFileSync(
      launcher,
      '#!/bin/sh\n# Starts UwULock from this folder.\nhere=$(dirname "$(readlink -f "$0")")\nexec "$here/AppRun" "$@"\n',
    );
    chmodSync(launcher, 0o755);
    writeFileSync(
      join(folder, 'README.txt'),
      [
        `UwULock ${version}, portable`,
        '',
        'Start it with ./uwulock (or ./AppRun) in this folder. Nothing is installed:',
        'the folder can live anywhere and be deleted when you are done.',
        '',
        'This copy does not update itself. For updates, install the .deb or .rpm',
        '(or the AUR package uwulock-bin) instead, or fetch the newest',
        'UwULock-linux-<arch>-portable.tar.gz from',
        'https://github.com/MinifyX/UwULock-Client/releases/latest',
        '',
      ].join('\n'),
    );
    const portable = join(out, `UwULock-linux-${arch()}-portable.tar.gz`);
    execFileSync('tar', [
      '--owner=0',
      '--group=0',
      '--numeric-owner',
      '-czf',
      portable,
      '-C',
      staging,
      'UwULock',
    ]);
    rmSync(staging, { recursive: true, force: true });
    produced.push(portable);

    // Copies the setup installed (~/.local/share/uwulock) update by running the
    // next setup. UwUSSH only ever shipped a Linux setup for x64, and UwULock
    // keeps the same shape.
    if (arch() === 'x64') {
      console.log('\n▸ Packing it into the setup, for the updater');
      const setupConfig = configFile('setup', { bundle: { active: true, targets: ['appimage'] } });
      run(
        `pnpm --filter @uwulock/setup tauri build --bundles appimage${targetArg} --config "${setupConfig}"`,
        { UWULOCK_SETUP_PAYLOAD: apps },
      );
      const image = join(out, `UwULock-update-linux-${arch()}.AppImage`);
      copyFileSync(
        only(
          join(bundles, 'appimage'),
          (name) => name.endsWith('.AppImage'),
          'AppImage of the setup',
        ),
        image,
      );
      execFileSync('chmod', ['+x', image]);
      produced.push(image);
    }
  }
  rmSync(apps, { recursive: true, force: true });
}

console.log('\n▸ Nothing is signed for the updater here; `pnpm release` does that.');
for (const file of produced) console.log(`✧ ${file}`);
