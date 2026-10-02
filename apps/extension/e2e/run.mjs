// The extension end to end, in one Chromium session against one real UwULock Server:
//
//   node e2e/run.mjs            (after `pnpm build`)
//
// 1. a UwULock Server release (the Docker image, or UWULOCK_SERVER_BIN=<a binary>) on a free
//    port, an account registered by invitation — crypto done here the way Bitwarden's apps do it;
// 2. the built extension (dist/chromium) in Chromium: log in through the popup;
// 3. a login page: sign in by hand, the save bar offers to save it, save;
// 4. the same page again: the inline menu fills it — but not while the page lays a see-through
//    layer over it that lets clicks through (clickjacking);
// 5. a WebAuthn page: register a passkey in the vault, sign in with it, the page checks the
//    signature with WebCrypto;
// 6. with a server that has UwULock's extras (0.6 on): an extras key, an own icon and a file
//    request made the way the web vault makes them, then the icon in the vault list, the file
//    request in the settings, and an item shared as a Send.
//
// Needs Playwright's Chromium (PLAYWRIGHT_BROWSERS_PATH, or `playwright-core install chromium`)
// and Docker unless UWULOCK_SERVER_BIN is set. Or a server that already runs (plain http on
// localhost): UWULOCK_SERVER_URL=<its address> UWULOCK_SERVER_INVITE=<an invitation for
// nyu@example.com> — for this test in Playwright's image against a server built on the host.
// Screenshots of a failure go to e2e/shots/.

import { execFileSync, spawn } from 'node:child_process';
import {
  createCipheriv,
  createHmac,
  generateKeyPairSync,
  hkdfSync,
  pbkdf2Sync,
  randomBytes,
} from 'node:crypto';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';
import { startPages } from './pages.mjs';

const app = join(dirname(fileURLToPath(import.meta.url)), '..');
const shots = join(app, 'e2e/shots');
const IMAGE = `ghcr.io/minifyx/uwulock-server:${process.env.UWULOCK_SERVER_VERSION ?? '0.6.0-beta.1'}`;
const EMAIL = 'nyu@example.com';
const PASSWORD = 'correct horse battery staple';
const SITE_USER = 'nyu';
const SITE_PASSWORD = 'Sit3-Passw0rd!';
const TIMEOUT = 20_000;

const step = (text) => console.log(`▸ ${text}`);
const cleanups = [];

function freePort() {
  return new Promise((resolve) => {
    const server = createServer();
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

async function until(what, check, timeout = TIMEOUT) {
  const end = Date.now() + timeout;
  for (;;) {
    try {
      const value = await check();
      if (value) return value;
    } catch {
      // Not yet.
    }
    if (Date.now() > end) throw new Error(`Timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

// ── The server ────────────────────────────────────────────

async function startServer() {
  const port = await freePort();
  const env = {
    UWULOCK_LISTEN: `127.0.0.1:${port}`,
    UWULOCK_PUBLIC: `http://localhost:${port}`,
    UWULOCK_TLS: 'off',
    UWULOCK_UPDATE_CHECK: 'off',
    UWULOCK_LOGIN_ATTEMPTS: '200',
  };
  let invite;
  if (process.env.UWULOCK_SERVER_URL) {
    const token = /token=([^&\s]+)/.exec(process.env.UWULOCK_SERVER_INVITE ?? '')?.[1];
    if (!token) throw new Error('UWULOCK_SERVER_INVITE is no invitation link');
    return { url: process.env.UWULOCK_SERVER_URL, token: decodeURIComponent(token) };
  }
  if (process.env.UWULOCK_SERVER_BIN) {
    const data = mkdtempSync(join(tmpdir(), 'uwulock-e2e-data-'));
    cleanups.push(() => rmSync(data, { recursive: true, force: true }));
    const full = { ...process.env, ...env, UWULOCK_DATA: data };
    invite = execFileSync(process.env.UWULOCK_SERVER_BIN, ['invite', EMAIL], {
      env: full,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const child = spawn(process.env.UWULOCK_SERVER_BIN, ['serve'], { env: full, stdio: 'ignore' });
    cleanups.push(() => child.kill());
  } else {
    const args = ['run', '-d', '--rm', '-p', `127.0.0.1:${port}:${port}`];
    for (const [key, value] of Object.entries({ ...env, UWULOCK_LISTEN: `0.0.0.0:${port}` })) {
      args.push('-e', `${key}=${value}`);
    }
    const id = execFileSync('docker', [...args, IMAGE], { encoding: 'utf8' }).trim();
    cleanups.push(() => execFileSync('docker', ['stop', id], { stdio: 'ignore' }));
    await until('the server', async () => (await fetch(`http://127.0.0.1:${port}/alive`)).ok);
    invite = execFileSync('docker', ['exec', id, 'uwulock-server', 'invite', EMAIL], {
      encoding: 'utf8',
    });
  }
  const url = `http://localhost:${port}`;
  await until('the server', async () => (await fetch(`http://127.0.0.1:${port}/alive`)).ok);
  const token = /token=([^&\s]+)/.exec(invite)?.[1];
  if (!token) throw new Error(`No invitation link in: ${invite}`);
  return { url, token: decodeURIComponent(token) };
}

/** An EncString of type 2: AES-256-CBC under the first half of `key`, HMAC-SHA256 under the second. */
function encrypt(plain, key) {
  const iv = randomBytes(16);
  const cipher = createCipheriv('aes-256-cbc', key.subarray(0, 32), iv);
  const data = Buffer.concat([cipher.update(plain), cipher.final()]);
  const tag = createHmac('sha256', key.subarray(32))
    .update(Buffer.concat([iv, data]))
    .digest();
  return `2.${iv.toString('base64')}|${data.toString('base64')}|${tag.toString('base64')}`;
}

/**
 * The extras key's second wrap: type 2 under HKDF-SHA256 of the account's private key (PKCS#8
 * DER), as `uwulock_core::extras::private_wrap_key` makes it.
 */
function privateWrap(privateKeyDer, key) {
  const wrapKey = Buffer.from(
    hkdfSync('sha256', privateKeyDer, 'uwulock-extras-key-v1', 'private-key-wrap', 64),
  );
  return encrypt(key, wrapKey);
}

/**
 * Registers the account the way Bitwarden's apps do: PBKDF2 master key, HKDF, AES-CBC + HMAC,
 * and a key pair under the user key. Returns what the extras step needs.
 */
async function register(server) {
  // What servers ask for at least today (Bitwarden, and UwULock Server from 0.6).
  const iterations = 600_000;
  const master = pbkdf2Sync(PASSWORD, EMAIL, iterations, 32, 'sha256');
  const hash = pbkdf2Sync(master, PASSWORD, 1, 32, 'sha256').toString('base64');
  const expand = (info) =>
    createHmac('sha256', master)
      .update(Buffer.concat([Buffer.from(info), Buffer.from([1])]))
      .digest();
  const userKey = randomBytes(64);
  const key = encrypt(userKey, Buffer.concat([expand('enc'), expand('mac')]));
  const pair = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const publicKey = pair.publicKey.export({ type: 'spki', format: 'der' }).toString('base64');
  const privateKeyDer = pair.privateKey.export({ type: 'pkcs8', format: 'der' });
  const encryptedPrivateKey = encrypt(privateKeyDer, userKey);
  const response = await fetch(
    `${server.url.replace('localhost', '127.0.0.1')}/identity/accounts/register/finish`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        email: EMAIL,
        name: 'Nyu',
        masterPasswordHash: hash,
        key,
        kdf: 0,
        kdfIterations: iterations,
        keys: { publicKey, encryptedPrivateKey },
        emailVerificationToken: server.token,
      }),
    },
  );
  if (!response.ok)
    throw new Error(`Registering failed: ${response.status} ${await response.text()}`);
  return { hash, userKey, publicKey, privateKeyDer };
}

/** The account on the server's API, as another device: for setting the scene only. */
async function apiSession(server, account) {
  const base = server.url.replace('localhost', '127.0.0.1');
  const login = await fetch(`${base}/identity/connect/token`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      grant_type: 'password',
      username: EMAIL,
      password: account.hash,
      scope: 'api offline_access',
      client_id: 'web',
      deviceType: '9',
      deviceIdentifier: '6f3c1d2e-8a4b-4c5d-9e6f-7a8b9c0d1e2f',
      deviceName: 'e2e',
    }),
  });
  if (!login.ok) throw new Error(`API login failed: ${login.status} ${await login.text()}`);
  const { access_token: token } = await login.json();
  return async (method, path, body) => {
    const response = await fetch(`${base}${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        ...(body ? { 'Content-Type': 'application/json' } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    const text = await response.text();
    if (!response.ok) throw new Error(`${method} ${path}: ${response.status} ${text}`);
    return text ? JSON.parse(text) : null;
  };
}

/** A 1 × 1 PNG. */
const PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
  'base64',
);

// ── The browser ───────────────────────────────────────────

/** The built extension, with the test's servers allowed up front (no permission bubble to click). */
function extensionCopy() {
  const dir = mkdtempSync(join(tmpdir(), 'uwulock-e2e-ext-'));
  cleanups.push(() => rmSync(dir, { recursive: true, force: true }));
  cpSync(join(app, 'dist/chromium'), dir, { recursive: true });
  const manifest = JSON.parse(readFileSync(join(dir, 'manifest.json'), 'utf8'));
  manifest.host_permissions = ['http://localhost/*', 'http://127.0.0.1/*'];
  writeFileSync(join(dir, 'manifest.json'), JSON.stringify(manifest));
  return dir;
}

/** The accessibility tree's node for `role` and `name` (or a name starting with it). */
async function findAx(cdp, role, name, prefix = false) {
  const { nodes } = await cdp.send('Accessibility.getFullAXTree');
  return nodes.find(
    (n) =>
      !n.ignored &&
      n.role?.value === role &&
      (prefix ? String(n.name?.value ?? '').startsWith(name) : n.name?.value === name),
  );
}

/**
 * Clicks what the accessibility tree calls `name`: the extension draws its menu and bar in
 * closed shadow roots, which no selector reaches — like assistive technology, the test finds
 * them by role and name. It waits until the element has been there a moment, as the extension
 * ignores clicks on what just appeared (half a second).
 */
async function clickAx(page, role, name, { prefix = false } = {}) {
  const cdp = await page.context().newCDPSession(page);
  try {
    let seen = 0;
    const box = await until(`${role} “${name}”`, async () => {
      const node = await findAx(cdp, role, name, prefix);
      if (!node?.backendDOMNodeId) {
        seen = 0;
        return null;
      }
      seen ||= Date.now();
      if (Date.now() - seen < 800) return null;
      const { model } = await cdp.send('DOM.getBoxModel', { backendNodeId: node.backendDOMNodeId });
      return model.content;
    });
    await page.mouse.click((box[0] + box[4]) / 2, (box[1] + box[5]) / 2);
  } finally {
    await cdp.detach().catch(() => undefined);
  }
}

/** The inline menu's list: UwULock's own page, in a frame under the field. */
function menuFrame(page) {
  return page.frames().find((frame) => /^chrome-extension:\/\/[^/]+\/menu\.html/.test(frame.url()));
}

async function hasMenuOption(page, name) {
  const frame = menuFrame(page);
  if (!frame) return false;
  return (await frame.getByRole('option', { name: new RegExp(`^${name}`) }).count()) > 0;
}

/**
 * Clicks the list's entry starting with `name`, with the mouse, where the page shows it — after
 * it has been there a moment, as the menu ignores clicks on what just appeared.
 */
async function clickMenuOption(page, name) {
  let seen = 0;
  const box = await until(`the menu's “${name}”`, async () => {
    const option = menuFrame(page)?.getByRole('option', { name: new RegExp(`^${name}`) });
    const found = option && (await option.count()) > 0 ? await option.first().boundingBox() : null;
    if (!found) {
      seen = 0;
      return null;
    }
    seen ||= Date.now();
    return Date.now() - seen < 800 ? null : found;
  });
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}

async function main() {
  const pages = await startPages();
  cleanups.push(() => pages.close());
  step('UwULock Server, and an account');
  const server = await startServer();
  const account = await register(server);

  step('Chromium with the extension');
  const profile = mkdtempSync(join(tmpdir(), 'uwulock-e2e-profile-'));
  cleanups.push(() => rmSync(profile, { recursive: true, force: true }));
  const extension = extensionCopy();
  const context = await chromium.launchPersistentContext(profile, {
    channel: 'chromium',
    headless: process.env.HEADED ? false : true,
    locale: 'en-US',
    args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`],
  });
  cleanups.unshift(() => context.close());
  context.setDefaultTimeout(TIMEOUT);
  const worker = context.serviceWorkers()[0] ?? (await context.waitForEvent('serviceworker'));
  const id = new URL(worker.url()).host;
  let current = null;

  try {
    step('Log in through the popup');
    const popup = await context.newPage();
    current = popup;
    await popup.goto(`chrome-extension://${id}/popup.html`);
    if (process.env.UWULOCK_E2E_SHOTS) {
      mkdirSync(shots, { recursive: true });
      await popup.setViewportSize({ width: 380, height: 580 });
      await popup.locator('input[type=email]').waitFor();
      await popup.screenshot({ path: join(shots, 'ui-login.png') });
    }
    await popup.locator('input[autocomplete=url]').fill(server.url);
    await popup.locator('input[type=email]').fill(EMAIL);
    await popup.locator('input[autocomplete=current-password]').fill(PASSWORD);
    await popup.locator('button[type=submit]').click();
    await popup.locator('.popup-tabs').waitFor();

    step('Sign in on a page: the bar offers to save the login');
    const site = await context.newPage();
    current = site;
    await site.goto(`${pages.url}/login`);
    await site.locator('#username').fill(SITE_USER);
    await site.locator('#password').fill(SITE_PASSWORD);
    await site.locator('#submit').click();
    await site.locator('#welcome').waitFor();
    await clickAx(site, 'button', 'Save');
    await until('the saved login', async () => {
      await popup.reload();
      await popup.getByRole('button', { name: 'Vault' }).click();
      // The list loads after the tab shows: wait for it a moment before trying again.
      await popup.locator('.item-row', { hasText: 'localhost' }).first().waitFor({ timeout: 3000 });
      return true;
    });

    step('The same page again: the inline menu fills it');
    await site.goto(`${pages.url}/login`);
    await site.locator('#username').click();
    await clickAx(site, 'button', 'Open the UwULock menu');
    if (process.env.UWULOCK_E2E_SHOTS) {
      await until('the list', () => hasMenuOption(site, 'localhost'));
      await site.evaluate(() => new Promise((resolve) => setTimeout(resolve, 300)));
      await site.screenshot({ path: join(shots, 'ui-inline-menu.png') });
    }
    await clickMenuOption(site, 'localhost');
    await until(
      'the filled form',
      async () =>
        (await site.locator('#username').inputValue()) === SITE_USER &&
        (await site.locator('#password').inputValue()) === SITE_PASSWORD,
    );

    step('A see-through layer over the menu that lets clicks through: the menu takes none');
    await site.goto(`${pages.url}/login`);
    await site.locator('#username').click();
    // The page's decoy, in the top layer above any z-index, with `pointer-events: none`.
    await site.evaluate(() => {
      const layer = document.createElement('div');
      layer.id = 'decoy';
      layer.popover = 'manual';
      layer.textContent = 'Click twice to accept cookies';
      layer.style.cssText =
        'pointer-events:none;inset:0;width:100vw;height:100vh;margin:0;border:0;opacity:0.9';
      document.body.append(layer);
      layer.showPopover();
    });
    await clickAx(site, 'button', 'Open the UwULock menu');
    // The list would open on the click itself; after a round trip to the page it is there or not.
    await site.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)));
    if (await hasMenuOption(site, 'localhost')) {
      throw new Error('The inline menu took a click through a layer over it');
    }
    await site.evaluate(() => document.getElementById('decoy').hidePopover());
    await clickAx(site, 'button', 'Open the UwULock menu');
    await clickMenuOption(site, 'localhost');
    await until(
      'the form filled once the layer is gone',
      async () => (await site.locator('#password').inputValue()) === SITE_PASSWORD,
    );

    step('The same layer laid over the open list: its frame takes no pick');
    await site.goto(`${pages.url}/login`);
    await site.locator('#username').click();
    await clickAx(site, 'button', 'Open the UwULock menu');
    await until('the list', () => hasMenuOption(site, 'localhost'));
    await site.evaluate(() => {
      const layer = document.createElement('div');
      layer.id = 'decoy';
      layer.popover = 'manual';
      layer.textContent = 'Click twice to accept cookies';
      layer.style.cssText =
        'pointer-events:none;inset:0;width:100vw;height:100vh;margin:0;border:0;opacity:0.9';
      document.body.append(layer);
      layer.showPopover();
    });
    await clickMenuOption(site, 'localhost');
    // The pick goes to the background and back; give it the time a fill takes, then look.
    await site.evaluate(() => new Promise((resolve) => setTimeout(resolve, 500)));
    if ((await site.locator('#password').inputValue()) !== '') {
      throw new Error("The menu's frame took a pick through a layer over it");
    }
    await site.evaluate(() => document.getElementById('decoy').hidePopover());
    await clickMenuOption(site, 'localhost');
    await until(
      'the form filled from the frame once the layer is gone',
      async () => (await site.locator('#password').inputValue()) === SITE_PASSWORD,
    );

    step('A passkey: register it in the vault');
    await site.goto(`${pages.url}/webauthn`);
    const [prompt] = await Promise.all([
      context.waitForEvent('page'),
      site.locator('#register').click(),
    ]);
    current = prompt;
    await prompt.locator('button[type=submit]').click();
    current = site;
    await until('the registered passkey', async () =>
      (await site.locator('#result').textContent())?.startsWith('registered'),
    );

    step('…and sign in with it; the page checks the signature');
    const [again] = await Promise.all([
      context.waitForEvent('page'),
      site.locator('#signin').click(),
    ]);
    current = again;
    await again.locator('button[type=submit]').click();
    current = site;
    await until(
      'the signature check',
      async () => (await site.locator('#result').textContent()) === 'verified',
    );

    step('The passkey shows in its login’s details, with its site');
    await until('the passkey’s login', async () => {
      await popup.reload();
      await popup.getByRole('button', { name: 'Vault' }).click();
      await popup
        .locator('.item-row', { hasText: 'Passkey test' })
        .first()
        .click({ timeout: 3000 });
      return true;
    });
    await popup.locator('.passkey-row', { hasText: 'localhost' }).first().waitFor();
    await popup.getByRole('button', { name: 'Back' }).click();
    console.log('✓ log in, save, fill and passkeys all work');

    const info = await fetch(`${server.url.replace('localhost', '127.0.0.1')}/uwu/v1/info`)
      .then((r) => r.json())
      .catch(() => ({}));
    const features = new Set(info.features ?? []);
    if (features.has('own-icons') && features.has('file-requests')) {
      step("UwULock's extras, made the way the web vault makes them");
      const api = await apiSession(server, account);
      const extrasKey = randomBytes(64);
      await api('POST', '/uwu/v1/keys', {
        userKeyWrapped: encrypt(extrasKey, account.userKey),
        privateKeyWrapped: privateWrap(account.privateKeyDer, extrasKey),
      });
      const sync = await api('GET', '/api/sync');
      const saved = sync.ciphers.find((c) => c.type === 1);
      await api('PUT', `/uwu/v1/icons/own/${saved.id}`, {
        data: encrypt(PNG, extrasKey),
        keyType: 'extras',
      });
      const secret = randomBytes(16);
      const linkKey = Buffer.from(
        hkdfSync('sha256', secret, 'bitwarden-filerequest', 'filerequest', 64),
      );
      const publicInfo = {
        v: 1,
        title: 'Passport',
        note: null,
        publicKey: account.publicKey,
        owner: null,
      };
      await api('POST', '/uwu/v1/file-requests', {
        name: encrypt(Buffer.from('Passport for the bank'), extrasKey),
        linkSecret: encrypt(secret, extrasKey),
        publicInfo: encrypt(Buffer.from(JSON.stringify(publicInfo)), linkKey),
        passwordHash: null,
        expirationDate: new Date(Date.now() + 7 * 86_400_000).toISOString(),
        maxSubmissions: null,
        maxFiles: 2,
        maxFileBytes: 1_048_576,
        textAllowed: true,
        sendDomainId: null,
        disabled: false,
      });

      step('After a sync, the own icon shows in the vault list');
      // Neither the key nor the icon is news on Bitwarden's hub: a sync finds them. The popup
      // keeps what it asked for while it is open, so it opens again, as a person would.
      await popup.reload();
      await popup.getByRole('button', { name: 'Sync now' }).click();
      await until('the own icon', async () => {
        await popup.reload();
        await popup.getByRole('button', { name: 'Vault' }).click();
        const src = await popup
          .locator('.item-row', { hasText: 'localhost' })
          .first()
          .locator('img.item-icon')
          .getAttribute('src', { timeout: 2000 });
        return src?.startsWith('data:image/png;base64,');
      });

      step('The file request in the settings');
      await popup.getByRole('button', { name: 'Settings' }).click();
      await popup.getByRole('button', { name: 'Show' }).click();
      await popup.locator('.file-request-row', { hasText: 'Passport for the bank' }).waitFor();
      await popup.getByRole('button', { name: 'Back' }).click();

      step('An item shared as a Send');
      await popup.getByRole('button', { name: 'Vault' }).click();
      await popup.locator('.item-row', { hasText: 'localhost' }).first().click();
      await popup.getByRole('button', { name: 'Share as a Send' }).click();
      await popup.getByRole('button', { name: 'Create link' }).click();
      await popup.getByText('Link created ✧').waitFor();
      const link = await popup.locator('.share-link').textContent();
      if (!link?.startsWith(`${server.url}/#/send/`)) throw new Error(`the Send's link: ${link}`);
      const sends = await api('GET', '/api/sends');
      if (sends.data.length !== 1 || sends.data[0].authType !== 2)
        throw new Error(`the Sends: ${JSON.stringify(sends.data)}`);
      console.log("✓ UwULock's extras: own icon, file requests, sharing as a Send");
    } else {
      console.log("– this server has no extras (own icons, file requests): they aren't checked");
    }

    if (process.env.UWULOCK_E2E_SHOTS) {
      // Pictures of the popup, for looking at it: UWULOCK_E2E_SHOTS=1.
      mkdirSync(shots, { recursive: true });
      await popup.setViewportSize({ width: 380, height: 580 });
      await popup.reload();
      await popup.locator('.popup-tabs').waitFor();
      for (const [tab, name] of [
        ['Vault', 'vault'],
        ['Generator', 'generator'],
        ['Settings', 'settings'],
      ]) {
        await popup.getByRole('button', { name: tab }).click();
        await popup.screenshot({ path: join(shots, `ui-${name}.png`) });
      }
      await popup.getByRole('button', { name: 'Vault' }).click();
      await popup.locator('.item-row').first().click();
      await popup.locator('.detail-head').waitFor();
      await popup.screenshot({ path: join(shots, 'ui-item.png') });
      await popup.getByRole('button', { name: 'Back' }).click();
      await popup.getByRole('button', { name: 'This page' }).click();
      await popup.screenshot({ path: join(shots, 'ui-page.png') });
      await popup.getByRole('button', { name: 'Lock', exact: true }).click();
      await popup.locator('.lock-view').waitFor();
      await popup.screenshot({ path: join(shots, 'ui-lock.png') });
    }
  } catch (error) {
    mkdirSync(shots, { recursive: true });
    for (const [index, page] of context.pages().entries()) {
      await page.screenshot({ path: join(shots, `page-${index}.png`) }).catch(() => undefined);
    }
    if (current)
      console.error(
        `on ${current.url()}: ${await current
          .locator('#result, .form-error')
          .allTextContents()
          .catch(() => [])}`,
      );
    throw error;
  }
}

try {
  await main();
} catch (error) {
  console.error(error);
  process.exitCode = 1;
} finally {
  for (const cleanup of cleanups) {
    try {
      await cleanup();
    } catch {
      // Best effort.
    }
  }
}
