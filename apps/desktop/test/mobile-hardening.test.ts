// What keeps the phone apps safe lives in Android and iOS files no test runs on this machine
// (the Kotlin and Swift plugin, the manifest, the capabilities). These checks read them and
// fail when one of the properties of docs/security-review-0.4.md goes missing:
//
//   node --test apps/desktop/test/
//
// They look at the source text on purpose: cheap, and they catch a property dropped by a
// refactor or a regenerated file. The behaviour itself is covered by the emulator and
// simulator smoke tests in CI.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const root = new URL('../../../', import.meta.url);
const read = (path: string) => readFileSync(new URL(path, root), 'utf8');
/** The text without `//` line comments, so a property only mentioned in a comment doesn't count. */
const code = (path: string) =>
  read(path)
    .split('\n')
    .map((line) => line.replace(/^\s*\/\/.*$/, ''))
    .join('\n');

const PLUGIN = 'crates/tauri-plugin-uwulock-mobile';
const KOTLIN = `${PLUGIN}/android/src/main/java/UwuLockMobilePlugin.kt`;
const SWIFT = `${PLUGIN}/ios/Sources/UwuLockMobilePlugin.swift`;
const ANDROID = 'apps/desktop/src-tauri/gen/android/app/src/main';

test('Android: no backups, no screenshots, no cleartext in release builds', () => {
  const manifest = read(`${ANDROID}/AndroidManifest.xml`);
  assert.match(manifest, /android:allowBackup="false"/);
  assert.match(manifest, /android:fullBackupContent="false"/);
  assert.match(manifest, /android:dataExtractionRules="@xml\/data_extraction_rules"/);
  const rules = read(`${ANDROID}/res/xml/data_extraction_rules.xml`);
  for (const section of ['cloud-backup', 'device-transfer']) {
    const body = rules.split(`<${section}>`)[1]?.split(`</${section}>`)[0] ?? '';
    for (const domain of ['root', 'file', 'database', 'sharedpref', 'external'])
      assert.match(body, new RegExp(`<exclude domain="${domain}" />`), `${section}: ${domain}`);
  }
  // Only the launcher is exported, and nothing opens UwULock from a link.
  assert.equal(manifest.match(/android:exported="true"/g)?.length, 1);
  assert.doesNotMatch(manifest, /android:scheme=|BROWSABLE|<provider|<service|<receiver/);
  assert.match(code(`${ANDROID}/java/app/uwulock/MainActivity.kt`), /FLAG_SECURE/);
  const gradle = read('apps/desktop/src-tauri/gen/android/app/build.gradle.kts');
  assert.match(gradle, /defaultConfig \{[^}]*usesCleartextTraffic"\] = "false"/s);
});

test('Android: the unlock key needs a strong biometric for every use and dies with a new one', () => {
  const kotlin = code(KOTLIN);
  assert.match(kotlin, /setUserAuthenticationRequired\(true\)/);
  assert.match(kotlin, /setInvalidatedByBiometricEnrollment\(true\)/);
  assert.match(
    kotlin,
    /setUserAuthenticationParameters\(0, KeyProperties\.AUTH_BIOMETRIC_STRONG\)/,
  );
  assert.match(kotlin, /setAllowedAuthenticators\(BIOMETRIC_STRONG\)/);
  assert.match(kotlin, /BiometricPrompt\.CryptoObject\(cipher\)/);
  assert.doesNotMatch(kotlin, /DEVICE_CREDENTIAL/);
});

test('Android: copies are marked sensitive', () => {
  assert.match(code(KOTLIN), /ClipDescription\.EXTRA_IS_SENSITIVE/);
});

test("Android: a Wi-Fi network's password only goes to the system's own sheet", () => {
  const kotlin = code(KOTLIN);
  const connect = kotlin.split('fun connectWifi')[1]?.split('@ActivityCallback')[0] ?? '';
  assert.match(connect, /systemActivity\(Intent\(Settings\.ACTION_WIFI_ADD_NETWORKS\)\)/);
  assert.match(connect, /\.setComponent\(sheet\)/);
  // The intent with the network is built only after the system's sheet was found.
  assert.ok(
    connect.indexOf('setComponent(sheet)') < connect.indexOf('EXTRA_WIFI_NETWORK_LIST'),
    'the component is set before the network goes in',
  );
  assert.match(kotlin, /MATCH_SYSTEM_ONLY/);
  assert.match(kotlin, /ApplicationInfo\.FLAG_SYSTEM/);
  assert.match(
    read(`${PLUGIN}/android/src/main/AndroidManifest.xml`),
    /<queries>\s*<intent>\s*<action android:name="android\.settings\.WIFI_ADD_NETWORKS" \/>/,
  );
});

test('iOS: Face ID key on this device only, cover in the app switcher, no backups', () => {
  const swift = code(SWIFT);
  assert.match(swift, /kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly/);
  assert.match(swift, /\.biometryCurrentSet/);
  assert.match(swift, /kSecAttrSynchronizable as String: false/);
  assert.match(swift, /\.localOnly: true/);
  assert.match(swift, /UIApplication\.willResignActiveNotification/);
  assert.match(swift, /isExcludedFromBackup = true/);
});

test('the phone plugin is reachable from Rust only, and window controls only on computers', () => {
  assert.match(read(`${PLUGIN}/build.rs`), /const COMMANDS: &\[&str\] = &\[\];/);
  const caps = ['default.json', 'desktop.json', 'macos.json'].map((name) =>
    JSON.parse(read(`apps/desktop/src-tauri/capabilities/${name}`)),
  );
  for (const cap of caps) {
    for (const permission of cap.permissions as string[])
      assert.ok(!permission.startsWith('uwulock-mobile:'), permission);
  }
  const [all, desktop, mac] = caps;
  assert.deepEqual(all.permissions, ['core:default']);
  assert.equal(all.platforms, undefined);
  // The app's own title bar on Windows and Linux; macOS draws its own and gets the menu bar.
  assert.deepEqual(desktop.platforms, ['linux', 'windows']);
  assert.ok((desktop.permissions as string[]).every((p) => p.startsWith('core:window:')));
  assert.deepEqual(mac.platforms, ['macOS']);
  assert.ok(
    (mac.permissions as string[]).every(
      (p) => p.startsWith('core:window:') || p === 'core:menu:default',
    ),
  );
});
