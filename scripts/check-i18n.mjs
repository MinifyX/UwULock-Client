// Every German string the apps translate has an English one.
//
//   node scripts/check-i18n.mjs
//
// Finds the string literals passed to t() and N_() in the desktop app and the
// browser extension and checks each app's English catalogue
// (apps/<app>/src/i18n/en/*.json) has each of them. Also reports a German
// string with two different English translations in different catalogue files.
// The extension shows a few of the desktop app's components, so its strings
// may also come from the desktop catalogue. Exits non-zero on a problem.

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

const files = (dir) =>
  readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === 'i18n' || name === 'wasm' ? [] : files(path);
    return /\.(ts|tsx)$/.test(name) ? [path] : [];
  });

const problems = [];

function catalogue(dir) {
  const english = new Map();
  for (const name of readdirSync(dir).filter((n) => n.endsWith('.json'))) {
    const entries = JSON.parse(readFileSync(join(dir, name), 'utf8'));
    for (const [german, translated] of Object.entries(entries)) {
      if (typeof translated !== 'string' || !translated.trim()) {
        problems.push(
          `${relative(root, dir)}/${name}: empty translation for ${JSON.stringify(german)}`,
        );
        continue;
      }
      const known = english.get(german);
      if (known && known.text !== translated) {
        problems.push(
          `${JSON.stringify(german)} is ${JSON.stringify(known.text)} in ${known.file} but ${JSON.stringify(translated)} in ${name}`,
        );
      }
      english.set(german, { text: translated, file: name });
    }
  }
  return english;
}

// t('…'), t("…"), t(`…`) without ${}, and the same for N_().
const call = /\b(?:t|N_)\(\s*(?:'((?:\\.|[^'\\])*)'|"((?:\\.|[^"\\])*)"|`((?:\\.|[^`\\$])*)`)/g;
const unescape = (text) =>
  text.replace(/\\(u\{[0-9a-fA-F]+\}|u[0-9a-fA-F]{4}|.)/g, (_, escape) => {
    if (escape.startsWith('u{')) return String.fromCodePoint(parseInt(escape.slice(2, -1), 16));
    if (escape.startsWith('u') && escape.length === 5)
      return String.fromCharCode(parseInt(escape.slice(1), 16));
    return { n: '\n', t: '\t' }[escape] ?? escape;
  });

const desktop = catalogue(join(root, 'apps/desktop/src/i18n/en'));
const extension = catalogue(join(root, 'apps/extension/src/i18n/en'));
const APPS = [
  { name: 'desktop', sources: join(root, 'apps/desktop/src'), english: desktop },
  { name: 'extension', sources: join(root, 'apps/extension/src'), english: extension },
];

let used = 0;
const seen = new Map(APPS.map((app) => [app.name, new Set()]));
for (const app of APPS) {
  for (const file of files(app.sources)) {
    const text = readFileSync(file, 'utf8');
    for (const match of text.matchAll(call)) {
      const german = unescape(match[1] ?? match[2] ?? match[3] ?? '');
      used += 1;
      seen.get(app.name).add(german);
      if (!app.english.has(german)) {
        const line = text.slice(0, match.index).split('\n').length;
        problems.push(`${relative(root, file)}:${line} has no English: ${JSON.stringify(german)}`);
      }
    }
  }
}

for (const app of APPS) {
  const unused = [...app.english.keys()].filter((german) => !seen.get(app.name).has(german));
  if (unused.length) {
    console.warn(`${unused.length} English entries of the ${app.name} are not used any more:`);
    for (const german of unused.slice(0, 20)) console.warn(`  ${JSON.stringify(german)}`);
  }
}

if (problems.length) {
  console.error(problems.join('\n'));
  console.error(`\n✗ ${problems.length} translation problems`);
  process.exit(1);
}
const strings = [...seen.values()].reduce((sum, set) => sum + set.size, 0);
console.log(`✓ ${strings} strings, ${used} uses, all with English`);
