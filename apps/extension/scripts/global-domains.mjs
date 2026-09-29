// Bitwarden's global equivalent domains, as the extension ships them:
//
//   node scripts/global-domains.mjs [<commit of bitwarden/server>]
//
// Reads the list from Bitwarden's server at that commit — the groups in
// src/Core/Utilities/StaticStore.cs, their numbers in src/Core/Enums/GlobalEquivalentDomainsType.cs
// (AGPL-3.0, which GPL-3.0 code may be combined with) — and writes src/shared/global-domains.json.
// The extension matches with this list, never with the one a server sends, so a server can't make
// a bank's login fill on a site of its choosing (security review 0.3, CL-L11). Run it again, with
// a newer commit, to follow Bitwarden's changes; the file names where it came from.

import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const COMMIT = process.argv[2] ?? 'e1f32b96a33f0f5d63170690f0b83cce0b901d36';
const RAW = `https://raw.githubusercontent.com/bitwarden/server/${COMMIT}`;
const STORE = 'src/Core/Utilities/StaticStore.cs';
const ENUM = 'src/Core/Enums/GlobalEquivalentDomainsType.cs';
const out = join(dirname(fileURLToPath(import.meta.url)), '../src/shared/global-domains.json');

async function source(path) {
  const response = await fetch(`${RAW}/${path}`, { redirect: 'error' });
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
  return response.text();
}

const numbers = new Map(
  [...(await source(ENUM)).matchAll(/^\s*(\w+)\s*=\s*(\d+)/gm)].map(([, name, n]) => [
    name,
    Number(n),
  ]),
);

const groups = [];
for (const [, name, list] of (await source(STORE)).matchAll(
  /GlobalDomains\.Add\(GlobalEquivalentDomainsType\.(\w+),\s*new List<string>\s*\{([^}]*)\}\)/g,
)) {
  const type = numbers.get(name);
  if (type === undefined) throw new Error(`No number for ${name}`);
  const domains = [...list.matchAll(/"([^"]+)"/g)].map(([, domain]) => domain.toLowerCase());
  if (domains.length < 2) continue;
  groups.push({ type, name, domains });
}
if (groups.length === 0) throw new Error('No groups found: did the source change its shape?');
groups.sort((a, b) => a.type - b.type);

writeFileSync(
  out,
  `${JSON.stringify(
    {
      source: `https://github.com/bitwarden/server/blob/${COMMIT}/${STORE}`,
      license: 'AGPL-3.0 (Bitwarden server)',
      groups,
    },
    null,
    2,
  )}\n`,
);
console.log(`✓ ${groups.length} groups from bitwarden/server@${COMMIT.slice(0, 7)}`);
