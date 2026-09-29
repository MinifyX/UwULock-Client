// Somebody without an account answers a file request in the web vault's own page, in a real
// browser: for the live check (scripts/live-server.sh with LIVE_BROWSER=1), so that what the web
// vault's uploader encrypts is what uwulock-core opens.
//
//   node scripts/live-upload.mjs <UwULock-Server checkout> <link> <password> <file name> <contents> <message>
//
// Playwright comes from the server's scripts/e2e; CHROMIUM and CHROMIUM_ARGS as there.

import { createRequire } from 'node:module';
import { join } from 'node:path';

const [serverDir, link, password, fileName, contents, message] = process.argv.slice(2);
if (!message) {
  console.error(
    'usage: live-upload.mjs <server checkout> <link> <password> <file name> <contents> <message>',
  );
  process.exit(2);
}
const { chromium } = createRequire(join(serverDir, 'scripts/e2e/package.json'))('playwright');

const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM || undefined,
  args: process.env.CHROMIUM_ARGS ? process.env.CHROMIUM_ARGS.split(' ') : [],
});
try {
  const context = await browser.newContext({ ignoreHTTPSErrors: true, locale: 'de-DE' });
  const page = await context.newPage();
  const problems = [];
  page.on('pageerror', (error) => problems.push(error.message));
  await page.goto(link);
  await page.getByRole('heading', { name: 'Passwort nötig' }).waitFor({ timeout: 30000 });
  await page.locator('input[type=password]').fill(password);
  await page.getByRole('button', { name: 'Öffnen' }).click();
  await page.getByRole('button', { name: 'Verschlüsselt senden' }).waitFor({ timeout: 30000 });
  await page
    .locator('input[type=file]')
    .setInputFiles([
      { name: fileName, mimeType: 'application/octet-stream', buffer: Buffer.from(contents) },
    ]);
  await page.getByLabel('Nachricht (freiwillig)').fill(message);
  await page.getByLabel('Dein Name (freiwillig)').fill('Mika');
  await page.getByRole('button', { name: 'Verschlüsselt senden' }).click();
  await page.getByRole('heading', { name: 'Angekommen ✧' }).waitFor({ timeout: 30000 });
  if (problems.length) throw new Error(problems.join('\n'));
  console.log('uploaded');
} finally {
  await browser.close();
}
