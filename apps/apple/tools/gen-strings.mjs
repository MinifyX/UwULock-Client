#!/usr/bin/env node
// Writes UwULock/Resources/Localizable.xcstrings: German source strings (the keys, as in the
// desktop app) with English from apps/desktop/src/i18n/en/*.json, plus the few strings only the
// native app has (NATIVE_ONLY below). The catalog is generated: edit this file, not the catalog.
//
// It looks at every string literal in the app's and UwUDesign's Swift sources and keeps those
// that have an English translation; German-looking literals without one are listed so they can be
// added to NATIVE_ONLY. Run: node apps/apple/tools/gen-strings.mjs
import { readFileSync, readdirSync, writeFileSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const apple = join(dirname(fileURLToPath(import.meta.url)), '..');
const enDir = join(apple, '../desktop/src/i18n/en');
const catalogPath = join(apple, 'UwULock/Resources/Localizable.xcstrings');

/** Strings the desktop catalogue doesn't have. */
const NATIVE_ONLY = {
  'Hier ist noch nichts.': 'Nothing here yet.',
  'Sobald dein Tresor geladen ist, stehen die Einträge hier.':
    'Your items will show up here once your vault has loaded.',
  'Kommt bald.': 'Coming soon.',
  'Weniger Bewegung': 'Reduce motion',
  'UwULock beenden': 'Quit UwULock',
  'Einstellungen …': 'Settings…',
  'Nach Updates suchen': 'Check for Updates…',
  'Nur auf diesem Gerät. UwU Sans ist die Schrift aller UwU-Apps.':
    'Only on this device. UwU Sans is the typeface of all UwU apps.',
  'Mit Face ID entsperren': 'Unlock with Face ID',
  'Mit Touch ID entsperren': 'Unlock with Touch ID',
  'Dein Tresor ist gesperrt': 'Your vault is locked',
  'Wähle links einen Eintrag.': 'Pick an item on the left.',
  'Nichts ausgewählt': 'Nothing selected',
  'Über UwULock': 'About UwULock',
  'Kleiner': 'Smaller',
  'Größer': 'Larger',
  'Sehr groß': 'Largest',
  'Textgröße': 'Text size',
  'Schrift': 'Font',
  'Prüfung': 'Check',
};

/** Literals that are not user-facing text (names, keys) and need no warning. */
const NOT_TEXT = /^(UwU|UwU Sans|UwULockPhase|SUFeedURL|Fonts|Lock|Manrope|Rubik|DM Sans|System|Regular|Light|Medium|Bold|ExtraBold|ExtraLight|SemiBold|Sends|Generator|Logins|Extras|WLAN|SSH \(UwUSSH\)|Remote Desktop \(UwURDP\)|Already registered|UwUDesign: .*|.*[@/].*|.*\\\(.*|[A-Za-z0-9]+-[A-Za-z0-9_]+)$/;

const en = {};
for (const file of readdirSync(enDir).filter((f) => f.endsWith('.json')).sort()) {
  Object.assign(en, JSON.parse(readFileSync(join(enDir, file), 'utf8')));
}
Object.assign(en, NATIVE_ONLY);

function swiftFiles(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === 'Generated' ? [] : swiftFiles(path);
    return name.endsWith('.swift') ? [path] : [];
  });
}

const literals = new Set();
for (const file of [...swiftFiles(join(apple, 'UwULock')), ...swiftFiles(join(apple, 'Packages/UwUDesign/Sources'))]) {
  for (const [, before, text] of readFileSync(file, 'utf8').matchAll(/(verbatim: )?"((?:[^"\\\n]|\\.)*)"/g)) {
    if (!before && text) literals.add(text);
  }
}

const strings = {};
const missing = [];
for (const key of [...literals].sort((a, b) => a.localeCompare(b, 'de'))) {
  const english = en[key];
  if (english === undefined) {
    if (/^[A-ZÄÖÜ]/.test(key) && !NOT_TEXT.test(key)) missing.push(key);
    continue;
  }
  strings[key] = { localizations: { en: { stringUnit: { state: 'translated', value: english } } } };
}

writeFileSync(catalogPath, `${JSON.stringify({ sourceLanguage: 'de', strings, version: '1.0' }, null, 2)}\n`);
console.log(`${Object.keys(strings).length} strings → ${catalogPath}`);
if (missing.length) console.log(`No English yet (add to NATIVE_ONLY):\n  ${missing.join('\n  ')}`);
