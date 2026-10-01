// The password check and its review one login at a time, in a browser:
//
//   node apps/desktop/e2e/review.mjs [screenshot directory]
//
// Starts the app's page with Vite and plays Rust: a fake of Tauri's IPC
// answers every command the page sends (health_report with a report that has
// every kind of problem, health_ignore, health_save_password, …) and records
// what it was asked. Nothing reaches a server or the internet. It checks the
// report's groups, the stack moved by keys and by dragging, a problem ignored
// and undone, "later", a new password generated and saved, the change page
// opened — then takes screenshots at desktop size and at 390 px as an
// Android phone, light and dark (default: apps/desktop/e2e/shots/).
//
// Needs Playwright (`npm i -g playwright` or one in a parent node_modules)
// with its Chromium; any OS.

import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { createServer } from 'vite';

const here = dirname(fileURLToPath(import.meta.url));
const shots = process.argv[2] ?? join(here, 'shots');
mkdirSync(shots, { recursive: true });

const server = await createServer({
  root: join(here, '..'),
  configFile: join(here, '..', 'vite.config.ts'),
  logLevel: 'error',
  server: { port: 0, strictPort: false, host: '127.0.0.1' },
});
await server.listen();
const { port } = server.httpServer.address();
const origin = `http://127.0.0.1:${port}`;

// ── The fake Rust side, run inside the page ────────────────

function fakeTauri() {
  const calls = [];
  const listeners = new Map();
  let next = 1;
  const now = new Date('2026-10-01T09:00:00Z').getTime();
  const item = (id, name, subtitle, host) => ({
    id,
    kind: 'login',
    name,
    subtitle,
    host,
    favorite: false,
    folderId: null,
    organizationId: null,
    collectionIds: [],
    deleted: false,
    reprompt: false,
    viewPassword: true,
    hasTotp: false,
    hasPassword: true,
    hasUsername: true,
    broken: false,
    revisionDate: '2026-09-01T00:00:00Z',
  });
  const items = [
    item('c1', 'Shop', 'nyu@example.com', 'shop.example.com'),
    item('c2', 'Forum', 'nyu', 'forum.example.org'),
    item('c3', 'Router', 'admin', '192.0.2.1'),
    item('c4', 'Mail', 'nyu@example.com', 'mail.example.net'),
    item('c5', 'Bank', 'nyu', 'bank.example.com'),
  ];
  const finding = (id, extra) => {
    const summary = items.find((i) => i.id === id);
    return {
      id,
      name: summary.name,
      subtitle: summary.subtitle,
      bits: 80,
      weak: false,
      reused: 0,
      unsecured: false,
      breached: 0,
      breachSources: [],
      host: summary.host,
      uri: `https://${summary.host}/login`,
      passwordChanged: '2022-03-01T00:00:00Z',
      ...extra,
    };
  };
  const leak = {
    domain: 'example.org',
    title: 'Example Forum',
    date: '2024-05-01',
    added: '2024-06-01',
    records: 1200,
    passwords: true,
    dataClasses: ['Email addresses', 'Passwords'],
    sources: { hibp: 'ExampleForum', xon: 'ExampleForumLeak' },
  };
  const findings = [
    finding('c1', {
      breached: 3861493,
      breachSources: ['hibp', 'xon'],
      reused: 1,
      weak: true,
      bits: 37,
    }),
    finding('c2', { reused: 1, weak: true, bits: 37 }),
    finding('c3', {
      unsecured: true,
      uri: 'http://192.0.2.1/',
      host: '192.0.2.1',
      weak: true,
      bits: 44,
    }),
    finding('c4', {}),
    finding('c5', {}),
  ];
  const state = { ignored: [], renewed: [] };
  const problemsOf = (f) => {
    const out = [];
    if (f.breached > 0) out.push({ kind: 'breached', count: f.breached, sources: f.breachSources });
    if (f.id === 'c2') out.push({ kind: 'siteBreach', breach: leak });
    if (f.reused) out.push({ kind: 'reused', others: f.reused });
    if (f.weak) out.push({ kind: 'weak', bits: f.bits });
    if (f.unsecured) out.push({ kind: 'unsecured' });
    if (f.id === 'c4') out.push({ kind: 'twofa', documentation: 'https://example.net/help/2fa' });
    return out;
  };
  const view = () => {
    const shown = findings.map((f) =>
      state.renewed.includes(f.id)
        ? { ...f, breached: 0, breachSources: [], weak: false, reused: 0 }
        : f,
    );
    const ignored = (id, kind) => state.ignored.some((e) => e.itemId === id && e.kind === kind);
    return {
      uwu: true,
      switches: {
        hibp: true,
        xonPasswords: true,
        siteBreaches: true,
        emailCheck: true,
        changePassword: true,
      },
      report: {
        findings: shown,
        checked: shown.length,
        breachesChecked: true,
        breachesIncomplete: false,
      },
      checkedAt: '2026-10-01T08:30:00Z',
      siteBreaches: { c2: leak },
      siteSources: [
        { id: 'hibp', name: 'Have I Been Pwned', license: 'CC BY 4.0' },
        { id: 'xon', name: 'XposedOrNot', license: null },
      ],
      sitesFailed: false,
      twofa: [
        {
          itemId: 'c4',
          name: 'Mail',
          host: 'mail.example.net',
          entry: {
            domain: 'example.net',
            name: 'Example Mail',
            methods: ['totp'],
            documentation: 'https://example.net/help/2fa',
          },
        },
      ],
      twofaSource: { name: '2FA Directory', license: 'MIT, © 2factorauth and contributors' },
      twofaFailed: false,
      ignored: state.ignored,
      cards: shown
        .map((f) => ({ finding: f, problems: problemsOf(f).filter((p) => !ignored(f.id, p.kind)) }))
        .filter((c) => c.problems.length),
      emailOptIn: { optedIn: true, since: '2026-09-30T00:00:00Z' },
    };
  };
  const status = {
    state: 'unlocked',
    accountId: 'a1',
    label: 'Privat',
    email: 'nyu@example.com',
    name: 'Nyu',
    server: 'lock.example.com',
    serverKind: 'self-hosted',
    serverUrl: 'https://lock.example.com',
    lastSync: now,
    syncing: false,
    syncError: null,
    sessionExpired: false,
    live: 'realtime',
    hello: null,
    helloKind: null,
    accounts: [
      {
        id: 'a1',
        label: 'Privat',
        email: 'nyu@example.com',
        name: 'Nyu',
        server: 'lock.example.com',
        serverKind: 'self-hosted',
        unlocked: true,
        active: true,
        lastSync: now,
      },
    ],
  };
  const answers = {
    vault_status: () => status,
    uwu_status: () => ({
      uwu: true,
      features: [
        'vault',
        'hibp',
        'twofa-directory',
        'xon-passwords',
        'site-breaches',
        'change-password',
      ],
      travel: { enabled: false, hiddenCount: null },
      unseen: { securityNotices: 0, fileRequestSubmissions: 0 },
      organizations: [],
      sendDomains: [],
      reminders: {},
      masked: {},
      ownIcons: {},
      automaticIcons: false,
      limits: null,
      extrasKeyChanged: false,
    }),
    vault_items: () => items,
    vault_overview: () => ({ folders: [], collections: [], organizations: [], skipped: 0 }),
    vault_item: ({ id }) => ({
      summary: items.find((i) => i.id === id),
      locked: false,
      notes: null,
      login: {
        username: 'nyu',
        hasPassword: true,
        hasTotp: false,
        passwordRevisionDate: null,
        uris: [],
        passkeys: 0,
      },
      fields: [],
      passwordHistory: [],
      attachments: 0,
      creationDate: null,
    }),
    item_icons: () => ({}),
    update_status: () => null,
    health_report: () => view(),
    health_ignore: ({ itemId, kind, ignored }) => {
      state.ignored = state.ignored.filter((e) => !(e.itemId === itemId && e.kind === kind));
      if (ignored) state.ignored.push({ itemId, kind, since: '2026-10-01T09:00:00Z' });
      return state.ignored;
    },
    health_save_password: ({ itemId }) => {
      state.renewed.push(itemId);
      return null;
    },
    health_open_page: () => null,
    health_email_opt_in: () => ({ optedIn: true, since: '2026-09-30T00:00:00Z' }),
    health_check_emails: () => ({
      results: [{ email: 'nyu@example.com', status: 'found', breaches: ['Example Forum (2024)'] }],
      retryAfter: null,
    }),
    generate_password: () => ({ password: 'q7#Lm2!vR9$wZ4^kP8', bits: 110 }),
    'plugin:event|listen': ({ event, handler }) => {
      const id = next++;
      listeners.set(id, { event, handler });
      return id;
    },
  };
  window.__fake = { calls, state };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => undefined };
  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: 'main' },
      currentWebview: { label: 'main', windowLabel: 'main' },
    },
    transformCallback: (callback) => {
      const id = next++;
      window[`_${id}`] = callback;
      return id;
    },
    unregisterCallback: (id) => delete window[`_${id}`],
    convertFileSrc: (path) => path,
    invoke: async (cmd, args) => {
      calls.push({ cmd, args });
      const answer = answers[cmd];
      return answer ? answer(args ?? {}) : null;
    },
  };
}

// ── The run ────────────────────────────────────────────────

const problems = [];
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM || undefined,
});

const check = (ok, what) => {
  if (!ok) problems.push(what);
};

async function open({ theme, phone }) {
  const context = await browser.newContext({
    viewport: phone ? { width: 390, height: 844 } : { width: 1280, height: 800 },
    deviceScaleFactor: phone ? 2 : 1,
    hasTouch: phone,
    isMobile: phone,
    locale: 'de-DE',
    reducedMotion: 'reduce',
    userAgent: phone
      ? 'Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Mobile Safari/537.36'
      : undefined,
  });
  await context.addInitScript(fakeTauri);
  await context.addInitScript((t) => {
    window.localStorage.setItem(
      'uwulock.settings',
      JSON.stringify({ theme: t, language: 'de', motion: 'reduced' }),
    );
  }, theme);
  const page = await context.newPage();
  page.on('pageerror', (error) => problems.push(`page error: ${error.message}`));
  page.on('console', (message) => {
    if (message.type() === 'error') problems.push(`console: ${message.text()}`);
  });
  await page.goto(origin);
  await page.waitForSelector('[data-testid="nav-health"]', { state: 'attached' });
  return { context, page };
}

const calls = (page, cmd) =>
  page.evaluate(
    (c) => window.__fake.calls.filter((call) => call.cmd === c).map((call) => call.args),
    cmd,
  );

// Desktop, dark: everything once.
{
  const { context, page } = await open({ theme: 'dark', phone: false });
  await page.click('[data-testid="nav-health"]');
  await page.waitForSelector('[data-testid="group-breached"]');
  for (const group of [
    'group-breached',
    'group-site',
    'group-reused',
    'group-weak',
    'group-unsecured',
    'group-twofa',
    'email-check',
  ]) {
    check(await page.isVisible(`[data-testid="${group}"]`), `report: ${group} missing`);
  }
  await page.screenshot({ path: join(shots, 'report-desktop-dark.png'), fullPage: true });

  await page.click('[data-testid="review-start"]');
  const progress = () => page.textContent('[data-testid="review-progress"]');
  check((await progress()) === '1 von 4', `review starts at ${await progress()}`);
  check(
    (await page.textContent('[data-testid="review-card"] h3')) === 'Shop',
    'the breached login comes first',
  );
  await page.keyboard.press('ArrowRight');
  check((await progress()) === '2 von 4', `→ goes to ${await progress()}`);
  await page.keyboard.press('ArrowLeft');
  check((await progress()) === '1 von 4', '← goes back');
  // A drag to the left is the next card, a short one stays.
  const box = await page.locator('[data-testid="review-card"]').boundingBox();
  const drag = async (dx) => {
    await page.mouse.move(box.x + box.width / 2, box.y + 40);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + dx / 2, box.y + 42, { steps: 4 });
    await page.mouse.move(box.x + box.width / 2 + dx, box.y + 44, { steps: 4 });
    await page.mouse.up();
  };
  await drag(-40);
  check((await progress()) === '1 von 4', 'a short drag stays');
  await drag(-140);
  check((await progress()) === '2 von 4', `a swipe left goes on (${await progress()})`);
  await drag(140);
  check((await progress()) === '1 von 4', 'a swipe right goes back');
  await page.screenshot({ path: join(shots, 'review-desktop-dark.png'), fullPage: true });

  // Ignore the weak password of "Shop", then undo it right there.
  const weak = page.locator('.review-problem', { hasText: 'Schwach' });
  await weak.getByRole('button', { name: /Ignorieren/ }).click();
  await page.waitForSelector('.review-problem[data-state="ignored"]');
  const ignored = await calls(page, 'health_ignore');
  check(
    ignored.length === 1 &&
      ignored[0].itemId === 'c1' &&
      ignored[0].kind === 'weak' &&
      ignored[0].ignored === true,
    `ignore sent ${JSON.stringify(ignored)}`,
  );
  await weak.getByRole('button', { name: /Rückgängig/ }).click();
  await page.waitForSelector('.review-problem[data-state="ignored"]', { state: 'detached' });

  // Open the page, then a new password with the generator.
  await page.getByRole('button', { name: 'Seite öffnen & Passwort ändern' }).click();
  await page.waitForFunction(() => window.__fake.calls.some((c) => c.cmd === 'health_open_page'));
  check((await calls(page, 'health_open_page'))[0]?.itemId === 'c1', 'the change page of the card');
  await page.getByRole('button', { name: 'Neues Passwort erzeugen & speichern' }).click();
  await page.getByRole('button', { name: 'Übernehmen' }).click();
  await page.waitForSelector('.review-problem[data-state="solved"]');
  const saved = await calls(page, 'health_save_password');
  check(
    saved[0]?.itemId === 'c1' && saved[0]?.password === 'q7#Lm2!vR9$wZ4^kP8',
    `saved ${JSON.stringify(saved)}`,
  );
  await page.screenshot({ path: join(shots, 'review-renewed-desktop-dark.png'), fullPage: true });

  // Later: out of the stack for the session.
  await page.getByRole('button', { name: 'Später' }).click();
  check((await progress()) === '1 von 3', `later leaves ${await progress()}`);
  check((await page.textContent('[data-testid="review-card"] h3')) !== 'Shop', 'Shop is put off');

  // Ignored in the review shows up in the report, and comes back from there.
  await page
    .locator('.review-problem')
    .first()
    .getByRole('button', { name: /Ignorieren/ })
    .click();
  await page.getByRole('button', { name: 'Zum Bericht' }).first().click();
  await page.waitForSelector('[data-testid="ignored"]');
  await page.locator('[data-testid="ignored"]').getByRole('button', { name: 'Rückgängig' }).click();
  await page.waitForSelector('[data-testid="ignored"]', { state: 'detached' });
  await context.close();
}

// Desktop, light; the phone in both.
{
  const { context, page } = await open({ theme: 'light', phone: false });
  await page.click('[data-testid="nav-health"]');
  await page.waitForSelector('[data-testid="group-breached"]');
  await page.screenshot({ path: join(shots, 'report-desktop-light.png'), fullPage: true });
  await page.click('[data-testid="review-start"]');
  await page.waitForSelector('[data-testid="review-card"]');
  await page.screenshot({ path: join(shots, 'review-desktop-light.png'), fullPage: true });
  await context.close();
}
for (const theme of ['light', 'dark']) {
  const { context, page } = await open({ theme, phone: true });
  check(
    (await page.getAttribute('html', 'data-platform')) === 'android',
    'the phone is an Android',
  );
  await page.getByRole('button', { name: 'Ordner und Typen' }).click();
  await page.click('[data-testid="nav-health"]');
  await page.waitForSelector('[data-testid="group-breached"]');
  // The drawer is gone once it slid out (its visibility follows the slide).
  await page.waitForFunction(
    () => getComputedStyle(document.querySelector('.sidebar')).visibility === 'hidden',
  );
  await page.screenshot({ path: join(shots, `report-phone-${theme}.png`), fullPage: true });
  await page.click('[data-testid="review-start"]');
  await page.waitForSelector('[data-testid="review-card"]');
  // A swipe across the card, as a finger would.
  const box = await page.locator('[data-testid="review-card"]').boundingBox();
  await page.mouse.move(box.x + box.width - 20, box.y + 30);
  await page.mouse.down();
  await page.mouse.move(box.x + 20, box.y + 34, { steps: 8 });
  await page.mouse.up();
  const progress = await page.textContent('[data-testid="review-progress"]');
  check(progress === '2 von 4', `phone swipe: ${progress}`);
  await page.mouse.move(box.x + 20, box.y + 30);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width - 20, box.y + 34, { steps: 8 });
  await page.mouse.up();
  await page.screenshot({ path: join(shots, `review-phone-${theme}.png`), fullPage: true });
  await context.close();
}

await browser.close();
await server.close();

if (problems.length) {
  console.error(problems.map((p) => `✗ ${p}`).join('\n'));
  process.exit(1);
}
console.log(`✓ review e2e passed; screenshots in ${shots}`);
