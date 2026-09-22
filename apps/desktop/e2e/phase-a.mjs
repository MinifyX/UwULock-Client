// Phase A: the app against the toy Vaultwarden, as a user would use it.
//
//   node apps/desktop/e2e/phase-a.mjs http://127.0.0.1:8097
//
// Expects the app running with its DevTools port open and a fresh data
// folder, and the toy server with two-step login on (see run.mjs).

import { execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { check, connect, failed, sleep } from './cdp.mjs';

const server = process.argv[2] ?? 'http://127.0.0.1:8097';
const shots = join(dirname(fileURLToPath(import.meta.url)), 'shots');
const page = await connect();
const shot = (name) => page.screenshot(join(shots, `${name}.png`));
const clipboard = () =>
  execFileSync('powershell', ['-NoProfile', '-Command', 'Get-Clipboard'], {
    encoding: 'utf8',
  }).trim();

try {
  // ── Log in, with an email code ─────────────────────────
  await page.waitFor(`document.querySelector('.welcome')`, { what: 'login screen' });
  await shot('a0-login');
  await page.fill('input[autocomplete=url]', server);
  await page.fill('input[type=email]', 'nyu@uwu.local');
  await page.fill('.password-input input', 'uwu-nyu-nyu-nyu');
  await page.key('Enter');
  await page.waitFor(`document.querySelector('.code-input')`, {
    timeout: 30_000,
    what: 'two-step login',
  });
  check('two-step login is asked for', true);
  await page.click('.segmented button', 'E-Mail');
  await page.click('button', 'Code senden');
  await sleep(300);
  await page.fill('.code-input', '000000');
  await page.key('Enter');
  await page.waitFor(`document.querySelector('.form-error')`, { what: 'wrong code error' });
  check('a wrong code is refused', (await page.text('.form-error')).includes('nicht angenommen'));
  await shot('a1-two-step');
  await page.fill('.code-input', '123456');
  await page.key('Enter');
  await page.waitFor(`document.querySelector('.vault')`, { timeout: 30_000, what: 'vault' });
  await sleep(500);
  await shot('a2-vault');

  const count = await page.eval(`document.querySelectorAll('.item-row').length`);
  check('all eight live items are listed', count === 8, `${count}`);

  // ── An item with a one-time code ───────────────────────
  await page.click('.item-row', 'GitHub');
  await page.waitFor(
    `/\\d{3} \\d{3}/.test(document.querySelector('.totp-code')?.textContent ?? '')`,
    {
      what: 'one-time code',
    },
  );
  check('the one-time code shows', true);
  await page.click('.icon-button[aria-label="Passwort zeigen"]');
  await sleep(300);
  check(
    'the password reveals',
    (await page.text('.detail-row .colored')).includes('hunter2-but-longer!'),
  );
  await page.click('.icon-button[aria-label="Passwort kopieren"]');
  await sleep(400);
  check('the password is on the clipboard', clipboard() === 'hunter2-but-longer!');
  await shot('a3-item');

  // ── Search ─────────────────────────────────────────────
  await page.fill('.search', 'katz');
  await sleep(300);
  const found = await page.text('.item-row .item-name');
  check('search finds the card', found === 'Katzenfutter-Karte', found);
  await page.fill('.search', '');

  // ── Re-prompt ──────────────────────────────────────────
  await page.click('.item-row', 'Vaultwarden Admin');
  await page.waitFor(`document.querySelector('.reprompt')`, { what: 're-prompt' });
  await page.fill('.reprompt .password-input input', 'uwu-nyu-nyu-nyu');
  await page.key('Enter');
  await page.waitFor(`!document.querySelector('.reprompt')`, {
    timeout: 30_000,
    what: 're-prompt passed',
  });
  check('a re-prompted item opens with the master password', true);

  // ── Lock and unlock ────────────────────────────────────
  await page.click('.titlebar-action[aria-label="Sperren"]');
  await page.waitFor(`document.querySelector('.lock')`, { what: 'lock screen' });
  check('locking shows the lock screen', true);
  await page.fill('.lock .password-input input', 'not-the-password');
  await page.key('Enter');
  await page.waitFor(`document.querySelector('.lock .form-error')`, {
    timeout: 30_000,
    what: 'error',
  });
  check('a wrong master password is refused', (await page.text('.form-error')).includes('falsch'));
  await shot('a4-locked');
  await page.fill('.lock .password-input input', 'uwu-nyu-nyu-nyu');
  await page.key('Enter');
  await page.waitFor(`document.querySelector('.vault')`, { timeout: 30_000, what: 'vault again' });
  check('unlocking opens the vault again', true);

  // ── Log out ────────────────────────────────────────────
  await page.click('.titlebar-action[aria-label="Sperren"]');
  await page.waitFor(`document.querySelector('.lock')`, { what: 'lock screen' });
  await page.click('.link-button', 'Abmelden');
  await page.click('.modal button.danger', 'Abmelden');
  await page.waitFor(`document.querySelector('.welcome')`, { what: 'login screen after logout' });
  check('logging out goes back to the login', true);
} catch (error) {
  check('phase A ran through', false, String(error));
  await shot('a-failed').catch(() => undefined);
} finally {
  page.close();
}
process.exit(failed() ? 1 : 0);
