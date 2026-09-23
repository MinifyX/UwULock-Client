// Phase B: writing. The app against two toy Vaultwardens, as a user would
// use it.
//
//   node apps/desktop/e2e/phase-b.mjs http://127.0.0.1:8097 http://127.0.0.1:8098
//
// Makes an item, edits it without ever seeing its password, generates a new
// one, adds a hidden field, moves it into a folder, throws it away and gets it
// back — then adds the second account and shows that the two vaults stay
// apart. Expects the app running with its DevTools port open and a fresh data
// folder (see run.mjs); the first server asks for a two-step code, the second
// doesn't.

import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { check, connect, failed, sleep } from './cdp.mjs';

const first = process.argv[2] ?? 'http://127.0.0.1:8097';
const second = process.argv[3] ?? 'http://127.0.0.1:8098';
const shots = join(dirname(fileURLToPath(import.meta.url)), 'shots');
const page = await connect();
const shot = (name) => page.screenshot(join(shots, `${name}.png`));

/** The input of the editor field called `label`, as a selector. */
async function field(label, scope = '.editor') {
  const found = await page.eval(`(() => {
    for (const marked of document.querySelectorAll('[data-e2e]')) marked.removeAttribute('data-e2e');
    for (const box of document.querySelectorAll(${JSON.stringify(`${scope} .field`)})) {
      const span = box.querySelector('span');
      if (span && span.textContent.trim() === ${JSON.stringify(label)}) {
        const input = box.querySelector('input, textarea, select');
        if (!input) return false;
        input.setAttribute('data-e2e', 'field');
        return true;
      }
    }
    return false;
  })()`);
  if (!found) throw new Error(`no editor field called "${label}"`);
  return '[data-e2e=field]';
}

const type = async (label, text) => page.fill(await field(label), text);

/** A <select> can't be typed into; React watches its change event. */
async function pick(label, value) {
  const selector = await field(label);
  await page.eval(`(() => {
    const el = document.querySelector('${selector}');
    const set = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set;
    set.call(el, ${JSON.stringify(value)});
    el.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await sleep(120);
}

const openEditor = async () =>
  page.waitFor(`document.querySelector('#item-editor')`, {
    what: 'the editor',
  });
const save = async () => {
  await page.click('.modal-footer button.primary', 'Speichern');
  await page.waitFor(`!document.querySelector('#item-editor')`, {
    timeout: 20_000,
    what: 'the editor to close',
  });
  await sleep(400);
};

/** The password of the item on screen, revealed. */
async function shownPassword() {
  await page.click('.icon-button[aria-label="Passwort zeigen"]');
  await sleep(250);
  const value = await page.text('.detail-row .colored');
  await page.click('.icon-button[aria-label="Passwort verbergen"]');
  return value;
}

const names = () =>
  page.eval(`[...document.querySelectorAll('.item-name')].map((e) => e.textContent)`);
const count = (selector) => page.eval(`document.querySelectorAll('${selector}').length`);

async function logIn(server, { twoFactor }) {
  await page.waitFor(`document.querySelector('.welcome')`, { what: 'login screen' });
  await page.fill('input[autocomplete=url]', server);
  await page.fill('input[type=email]', 'nyu@uwu.local');
  await page.fill('.password-input input', 'uwu-nyu-nyu-nyu');
  await page.key('Enter');
  if (twoFactor) {
    await page.waitFor(`document.querySelector('.code-input')`, {
      timeout: 30_000,
      what: 'two-step login',
    });
    await page.click('.segmented button', 'E-Mail');
    await page.fill('.code-input', '123456');
    await page.key('Enter');
  }
  await page.waitFor(`document.querySelector('.vault')`, { timeout: 40_000, what: 'vault' });
  await sleep(600);
}

try {
  await logIn(first, { twoFactor: true });

  // ── A new item ─────────────────────────────────────────
  await page.click('.new-item');
  await page.click('.context-menu button', 'Login');
  await openEditor();
  await type('Name', 'Kater-Konto');
  await type('Benutzername', 'nyu');
  await type('Passwort', 'erstes-Passwort!');
  await page.fill('.editor-list input[placeholder="https://…"]', 'https://kater.example');
  await shot('b1-editor');
  await save();
  const listed = await names();
  check('a new item is in the list', listed.includes('Kater-Konto'), listed.join(', '));
  check(
    'the new item is the one on screen',
    (await page.text('.detail-title h2')).includes('Kater-Konto'),
  );
  check('its password came through', (await shownPassword()) === 'erstes-Passwort!');

  // ── Editing without seeing the password ────────────────
  await page.click('.detail-tools button.primary', 'Bearbeiten');
  await openEditor();
  const kept = await page.text('.field-hint');
  check('the editor leaves the password alone', kept.includes('Bleibt, wie es ist.'), kept);
  await type('Name', 'Kater-Konto (privat)');
  await save();
  check('the new name is in the list', (await names()).includes('Kater-Konto (privat)'));
  check('the untouched password is unchanged', (await shownPassword()) === 'erstes-Passwort!');

  // ── The generator, into the field ──────────────────────
  await page.click('.detail-tools button.primary', 'Bearbeiten');
  await openEditor();
  await page.click('.field-actions .icon-button[aria-label="Passwort-Generator"]');
  await page.waitFor(`document.querySelector('.generated')`, { what: 'the generator' });
  await page.click('.modal-footer button.primary', 'Übernehmen');
  await sleep(200);
  await save();
  const generated = await shownPassword();
  check(
    'the generated password was saved',
    generated.length >= 5 && generated !== 'erstes-Passwort!',
    generated,
  );

  // ── A hidden field ─────────────────────────────────────
  await page.click('.detail-tools button.primary', 'Bearbeiten');
  await openEditor();
  await page.click('.add-kinds button', 'Versteckt');
  await page.fill('.editor-list input[placeholder="Feldname"]', 'Notfall-PIN');
  await page.fill('.editor-row .field input.mono', '4711');
  await save();
  await page.waitFor(
    `[...document.querySelectorAll('.detail-label')].some((e) => e.textContent === 'Notfall-PIN')`,
    {
      what: 'the new field',
    },
  );
  await page.click('.icon-button[aria-label="Notfall-PIN zeigen"]');
  await sleep(250);
  check(
    'a hidden field keeps its value',
    (await page.text('.detail-row .colored')).includes('4711'),
  );

  // ── Favourite, folder ──────────────────────────────────
  await page.click('.detail-tools .icon-button[aria-label="Zu Favoriten"]');
  await sleep(600);
  const favourites = await page.eval(
    `document.querySelector('.nav-list .nav-row:nth-child(1)')?.parentElement?.parentElement?.children?.length ?? 0`,
  );
  check('the sidebar still stands after the favourite', favourites > 0);
  check(
    'it is a favourite now',
    await page.eval(
      `!!document.querySelector('.detail-tools .icon-button[aria-label="Favorit entfernen"]')`,
    ),
  );

  await page.click('.nav-heading .icon-button');
  await page.waitFor(`document.querySelector('.modal .field input')`, {
    what: 'the folder dialog',
  });
  await page.fill('.modal .field input', 'Kater');
  await page.click('.modal-footer button.primary', 'Anlegen');
  await sleep(700);
  check('the folder is in the sidebar', (await page.text('.nav-label')).includes('Kater'));

  await page.click('.detail-tools button.primary', 'Bearbeiten');
  await openEditor();
  const folderId = await page.eval(`(() => {
    const options = [...document.querySelectorAll('#item-editor select option')];
    return options.find((o) => o.textContent === 'Kater')?.value ?? '';
  })()`);
  await pick('Ordner', folderId);
  await save();
  await page.click('.nav-row', 'Kater');
  await sleep(400);
  const inFolder = await names();
  check(
    'the item moved into the folder',
    inFolder.length === 1 && inFolder[0] === 'Kater-Konto (privat)',
    inFolder.join(', '),
  );
  await page.click('.nav-row', 'Alle Einträge');
  await sleep(300);

  // ── Trash, back, and gone ──────────────────────────────
  await page.click('.item-row', 'Kater-Konto (privat)');
  await sleep(200);
  await page.click('.detail-tools .icon-button[aria-label="In den Papierkorb"]');
  await page.click('.modal button.danger', 'In den Papierkorb');
  await sleep(800);
  check('the item left the list', !(await names()).includes('Kater-Konto (privat)'));
  await page.click('.nav-trash .nav-row');
  await sleep(400);
  check('it is in the trash', (await names()).includes('Kater-Konto (privat)'));
  await shot('b2-trash');
  // The sample vault has a trashed item of its own, and the list picks the
  // first one: say which one this is about.
  await page.click('.item-row', 'Kater-Konto (privat)');
  await sleep(300);
  await page.click('.detail-tools button', 'Wiederherstellen');
  await sleep(900);
  await page.click('.nav-row', 'Alle Einträge');
  await sleep(500);
  const restored = await names();
  check('restoring brings it back', restored.includes('Kater-Konto (privat)'), restored.join(', '));

  // ── A second account ───────────────────────────────────
  const before = await count('.item-row');
  await page.click('.account-switch');
  await page.click('.context-menu button', 'Konto hinzufügen');
  await logIn(second, { twoFactor: false });
  check(
    'the second account opened its own vault',
    (await count('.item-row')) === 8,
    `${await count('.item-row')}`,
  );
  check('its vault is a different one', !(await names()).includes('Kater-Konto (privat)'));

  await page.click('.new-item');
  await page.click('.context-menu button', 'Login');
  await openEditor();
  await type('Name', 'Arbeit-Konto');
  await save();
  check('an item can be made in the second account', (await names()).includes('Arbeit-Konto'));

  await page.click('.account-switch');
  await shot('b3-accounts');
  const menu = await page.text('.context-menu button');
  check(
    'the switcher lists the other account',
    menu.includes('Konto hinzufügen') && menu.includes(':8097'),
    menu,
  );
  await page.click('.context-menu button', ':8097');
  await sleep(900);
  const back = await names();
  check(
    'switching back shows the first vault',
    back.includes('Kater-Konto (privat)') && !back.includes('Arbeit-Konto'),
    back.join(', '),
  );
  check(
    'nothing was lost on the way',
    (await count('.item-row')) === before,
    `${await count('.item-row')}`,
  );

  // ── Gone for good ──────────────────────────────────────
  await page.click('.item-row', 'Kater-Konto (privat)');
  await sleep(200);
  await page.click('.detail-tools .icon-button[aria-label="In den Papierkorb"]');
  await page.click('.modal button.danger', 'In den Papierkorb');
  await sleep(800);
  await page.click('.nav-trash .nav-row');
  await sleep(400);
  await page.click('.item-row', 'Kater-Konto (privat)');
  await sleep(300);
  await page.click('.detail-tools button', 'Endgültig löschen');
  await page.click('.modal button.danger', 'Endgültig löschen');
  await sleep(900);
  const trash = await names();
  check(
    'deleting for good takes it out of the trash',
    !trash.includes('Kater-Konto (privat)') && trash.includes('Altes Forum'),
    trash.join(', '),
  );
  await page.click('.nav-row', 'Alle Einträge');
  await sleep(300);
  check('and the item is gone', !(await names()).includes('Kater-Konto (privat)'));
} catch (error) {
  check('phase B ran through', false, String(error));
  await shot('b-failed').catch(() => undefined);
} finally {
  page.close();
}
process.exit(failed() ? 1 : 0);
