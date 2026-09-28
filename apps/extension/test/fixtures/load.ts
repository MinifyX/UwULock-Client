import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/** Puts `fixtures/<name>.html` into the document's body. */
export function load(name: string) {
  document.body.innerHTML = readFileSync(join(__dirname, `${name}.html`), 'utf8');
}
