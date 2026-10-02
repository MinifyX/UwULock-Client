// @vitest-environment node
/**
 * The background's UwULock extras against a fake server (a mocked `fetch`) and a fake
 * WebAssembly module: which requests go where, with what, and what comes back — masked
 * addresses, sharing an item as a Send, file requests, icons, and the contract's error codes.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Account } from '../src/background/store';

// ── Fakes ─────────────────────────────────────────────────

const local: Record<string, unknown> = {};

vi.mock('../src/shared/browser', () => ({
  ext: {
    storage: {
      local: {
        get: async (key: string) => ({ [key]: local[key] }),
        set: async (values: Record<string, unknown>) => Object.assign(local, values),
      },
    },
    runtime: { sendMessage: async () => undefined },
    tabs: { query: async () => [], sendMessage: async () => undefined },
  },
  isFirefox: false,
}));

const copied: string[] = [];
vi.mock('../src/background/clipboard', () => ({
  copy: async (text: string) => {
    copied.push(text);
  },
}));

type Summary = { id: string; host: string | null; organizationId: string | null };
let items: Summary[] = [];
let extrasState = 'open';
const core = {
  openExtras: vi.fn((_keys: string) => JSON.stringify({ state: extrasState })),
  openIcons: vi.fn((icons: string) =>
    JSON.stringify(
      (JSON.parse(icons) as { cipherId: string; data: string }[]).map((icon) => ({
        cipherId: icon.cipherId,
        png: `png-of-${icon.data}`,
      })),
    ),
  ),
  fileRequestLabels: vi.fn((list: string) =>
    JSON.stringify(
      (JSON.parse(list) as { id: string; name: string | null }[]).map((r) => ({
        id: r.id,
        label: r.name ? `label of ${r.name}` : null,
      })),
    ),
  ),
  fileRequestLink: vi.fn(
    (request: string, base: string, sendDomain: boolean) =>
      `${base}|${JSON.parse(request).accessId}|${sendDomain}`,
  ),
  shareableFields: vi.fn(() => JSON.stringify([{ name: 'username' }, { name: 'password' }])),
  sealShare: vi.fn((_id: string, _options: string) =>
    JSON.stringify({ type: 0, name: '2.name', key: '2.seed', text: { text: '2.text' } }),
  ),
  sendLink: vi.fn(
    (key: string, accessId: string, base: string, sendDomain: boolean) =>
      `${base}|${accessId}|${key}|${sendDomain}`,
  ),
  items: vi.fn(() => JSON.stringify(items)),
  open: vi.fn((_text: string) => undefined),
  autofillIndex: vi.fn(() => '[]'),
  deletePasskey: vi.fn((_id: string, _index: number, _credentialId?: string) =>
    JSON.stringify({ cipher: { type: 1, name: '2.name', login: { fido2Credentials: [] } } }),
  ),
};

// The vault's live connection isn't under test here.
vi.mock('../src/background/live', () => ({ start: () => undefined, stop: () => undefined }));

vi.mock('../src/background/wasm', () => ({
  call: async (work: (c: typeof core) => unknown) => work(core),
  callJson: async (work: (c: typeof core) => string) => JSON.parse(work(core)),
}));

const { ApiError, failure } = await import('../src/background/http');
const extras = await import('../src/background/extras');
const { icons, iconHost } = await import('../src/background/icons');
const vault = await import('../src/background/vault');
const { uwuInfo, hasFeature } = await import('../src/background/uwu');

type Call = { method: string; url: string; body: unknown; auth: string | null };
let calls: Call[] = [];
type Route = (call: Call) => Response | undefined;
let routes: Route[] = [];

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function route(method: string, url: string | RegExp, answer: (call: Call) => Response) {
  routes.push((call) =>
    call.method === method && (typeof url === 'string' ? call.url === url : url.test(call.url))
      ? answer(call)
      : undefined,
  );
}

vi.stubGlobal('fetch', async (input: string, init: RequestInit = {}) => {
  const headers = (init.headers ?? {}) as Record<string, string>;
  const call: Call = {
    method: init.method ?? 'GET',
    url: String(input),
    body: typeof init.body === 'string' ? JSON.parse(init.body) : null,
    auth: headers.Authorization ?? null,
  };
  calls.push(call);
  for (const r of routes) {
    const answer = r(call);
    if (answer) return answer;
  }
  return json({ message: 'Not found.', object: 'error', code: 'not_found' }, 404);
});

const WEB = 'https://lock.example.com';

function account(patch: Partial<Account> = {}): Account {
  return {
    id: 'u1',
    userId: 'u1',
    email: 'nyu@example.com',
    name: null,
    server: { kind: 'self-hosted', url: WEB },
    kdf: '{}',
    protectedKey: '2.x',
    accessToken: 'token',
    refreshToken: 'refresh',
    expiresAt: Date.now() + 3_600_000,
    lastSync: null,
    rememberToken: null,
    pinProtected: null,
    uwu: {
      version: '0.6.0',
      features: ['masked-addresses', 'own-icons', 'icons', 'file-requests', 'send-domains'],
      icons: { automatic: true, url: `${WEB}/icons` },
      sendDomains: [{ id: 'd1', url: 'https://send.example.com' }],
    },
    ...patch,
  };
}

beforeEach(() => {
  calls = [];
  routes = [];
  copied.length = 0;
  items = [];
  extrasState = 'open';
  for (const key of Object.keys(local)) delete local[key];
  for (const fn of Object.values(core)) fn.mockClear();
  // A new "vault": what is cached beside it is forgotten.
  vault.closed();
});

// ── Masked addresses ──────────────────────────────────────

describe('masked addresses', () => {
  it('are made for the origin of the tab, with the session and the item', async () => {
    route('POST', `${WEB}/uwu/v1/masked/addresses`, () =>
      json({
        object: 'maskedAddress',
        id: 'x42',
        email: 'quiet.otter17@masked.example.com',
        forDomain: 'https://shop.example.com',
      }),
    );
    const cipherId = '0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0';
    const created = await extras.createMasked(
      account(),
      'https://shop.example.com/cart?item=1#top',
      cipherId,
    );
    expect(created.email).toBe('quiet.otter17@masked.example.com');
    expect(calls).toHaveLength(1);
    expect(calls[0]!.auth).toBe('Bearer token');
    expect(calls[0]!.body).toEqual({
      forDomain: 'https://shop.example.com',
      description: '',
      domain: null,
      emailPrefix: null,
      cipherId,
    });
  });

  it('keep the contract’s code in the error', async () => {
    for (const [status, code] of [
      [409, 'not_connected'],
      [409, 'revoked'],
      [502, 'upstream'],
      [422, 'quota'],
      [429, 'rate_limited'],
    ] as const) {
      routes = [];
      route('POST', `${WEB}/uwu/v1/masked/addresses`, () =>
        json({ message: 'No.', object: 'error', code }, status),
      );
      const error = await extras.createMasked(account(), null, null).catch((e: unknown) => e);
      expect(error).toBeInstanceOf(ApiError);
      expect(failure(error)).toEqual({ kind: `uwu:${code}`, message: 'No.' });
    }
    // Nothing but a web page's origin, and no made-up item ids.
    expect(calls.at(-1)!.body).toMatchObject({ forDomain: '', cipherId: null });
  });

  it('are not asked for when the server has none', async () => {
    const plain = account({ uwu: null });
    await expect(extras.createMasked(plain, 'https://shop.example.com', null)).rejects.toEqual(
      expect.objectContaining({ kind: 'uwu:feature_off' }),
    );
    await expect(extras.maskedConnection(plain)).rejects.toEqual(
      expect.objectContaining({ kind: 'uwu:feature_off' }),
    );
    expect(calls).toHaveLength(0);
  });

  it('say whether the account is connected, and where to connect it', async () => {
    route('GET', `${WEB}/uwu/v1/masked/connection`, () =>
      json({
        object: 'maskedConnection',
        connected: false,
        server: null,
        status: null,
        allowedServers: [{ url: 'https://mail.example.com', name: 'UwUMail' }],
      }),
    );
    expect(await extras.maskedConnection(account())).toEqual({
      connected: false,
      status: null,
      server: null,
      username: null,
      defaultDomain: null,
      settingsUrl: `${WEB}/#/settings/masked`,
    });
  });

  it('take only web pages as their site', () => {
    expect(extras.forDomain('https://shop.example.com:8443/a')).toBe(
      'https://shop.example.com:8443',
    );
    expect(extras.forDomain('chrome://settings')).toBe('');
    expect(extras.forDomain('not a url')).toBe('');
    expect(extras.forDomain(null)).toBe('');
  });
});

// ── Sends ─────────────────────────────────────────────────

describe('sharing an item as a Send', () => {
  const options = {
    fields: [
      ['username', 'Username'],
      ['password', 'Password'],
    ] as [string, string][],
    deletionHours: 24,
    maxAccessCount: 1,
    password: null,
  };

  it('posts the sealed Send and links it on the account’s send domain', async () => {
    route('POST', `${WEB}/api/sends`, () =>
      json({ object: 'send', id: 's1', accessId: 'acc1', key: '2.seed', deletionDate: 'later' }),
    );
    route('GET', `${WEB}/uwu/v1/account`, () => json({ object: 'account', sendDomainId: 'd1' }));
    const before = Date.now();
    const shared = await extras.shareItem(account(), 'item-1', options);
    expect(shared).toEqual({
      id: 's1',
      link: 'https://send.example.com|acc1|2.seed|true',
      deletionDate: 'later',
      onSendDomain: true,
    });
    const [id, sealed] = core.sealShare.mock.calls[0]!;
    expect(id).toBe('item-1');
    const sent = JSON.parse(sealed);
    expect(sent.fields).toEqual(options.fields);
    expect(sent.maxAccessCount).toBe(1);
    const deletion = Date.parse(sent.deletionDate) - before;
    expect(deletion).toBeGreaterThanOrEqual(24 * 3_600_000);
    expect(deletion).toBeLessThan(24 * 3_600_000 + 60_000);
    expect(calls[0]!.body).toMatchObject({ type: 0, key: '2.seed' });
  });

  it('uses the web vault without a default send domain', async () => {
    route('POST', `${WEB}/api/sends`, () => json({ id: 's1', accessId: 'acc1', key: '2.seed' }));
    route('GET', `${WEB}/uwu/v1/account`, () => json({ sendDomainId: null }));
    expect((await extras.shareItem(account(), 'item-1', options)).link).toBe(
      `${WEB}|acc1|2.seed|false`,
    );
  });

  it('works with Bitwarden too, where there are no send domains', async () => {
    const cloud = account({ server: { kind: 'bitwarden-eu' }, uwu: null });
    route('POST', 'https://api.bitwarden.eu/sends', () =>
      json({ Id: 's1', AccessId: 'acc1', Key: '2.seed' }),
    );
    const shared = await extras.shareItem(cloud, 'item-1', options);
    expect(shared.link).toBe('https://vault.bitwarden.eu|acc1|2.seed|false');
    expect(calls.map((c) => c.url)).toEqual(['https://api.bitwarden.eu/sends']);
  });

  it('is an entry Send only when asked for one, in so many words', async () => {
    route('POST', `${WEB}/api/sends`, () => json({ id: 's1', accessId: 'a', key: '2.k' }));
    route('GET', `${WEB}/uwu/v1/account`, () => json({ sendDomainId: null }));
    await extras.shareItem(account(), 'item-1', {
      ...options,
      fields: [...options.fields, ['totp', 'One-time code']],
      entry: true,
    });
    const entry = JSON.parse(core.sealShare.mock.calls.at(-1)![1]);
    expect(entry.entry).toBe(true);
    expect(entry.fields).toContainEqual(['totp', 'One-time code']);
    await extras.shareItem(account(), 'item-1', options);
    expect(JSON.parse(core.sealShare.mock.calls.at(-1)![1]).entry).toBe(false);
    await extras.shareItem(account(), 'item-1', {
      ...options,
      entry: 'yes' as unknown as boolean,
    });
    expect(JSON.parse(core.sealShare.mock.calls.at(-1)![1]).entry).toBe(false);
  });

  it('needs something to share, and keeps the limits sane', async () => {
    await expect(
      extras.shareItem(account(), 'item-1', { ...options, fields: [] }),
    ).rejects.toMatchObject({ kind: 'invalid' });
    route('POST', `${WEB}/api/sends`, () => json({ id: 's1', accessId: 'a', key: '2.k' }));
    await extras.shareItem(account({ uwu: null }), 'item-1', {
      ...options,
      deletionHours: 10_000,
      maxAccessCount: 0,
      password: 'pw',
    });
    const sent = JSON.parse(core.sealShare.mock.calls.at(-1)![1]);
    expect(Date.parse(sent.deletionDate) - Date.now()).toBeLessThanOrEqual(31 * 24 * 3_600_000);
    expect(sent.maxAccessCount).toBe(1);
    expect(sent.password).toBe('pw');
  });
});

// ── File requests ─────────────────────────────────────────

describe('file requests', () => {
  const request = (id: string, patch: Record<string, unknown> = {}) => ({
    object: 'fileRequest',
    id,
    accessId: `access-${id}`,
    name: `2.${id}`,
    linkSecret: '2.secret',
    expirationDate: '2099-01-01T00:00:00.000000Z',
    maxSubmissions: 1,
    submissionCount: 0,
    unseen: 0,
    disabled: false,
    sendDomainId: null,
    ...patch,
  });

  it('are listed with their labels, page by page', async () => {
    route('GET', `${WEB}/uwu/v1/keys`, () =>
      json({ object: 'uwuKeys', extrasKey: {}, lost: false }),
    );
    route('GET', `${WEB}/uwu/v1/file-requests`, () =>
      json({ object: 'list', data: [request('r1')], continuationToken: 'next' }),
    );
    route('GET', `${WEB}/uwu/v1/file-requests?continuationToken=next`, () =>
      json({
        object: 'list',
        data: [
          request('r2', {
            name: null,
            expirationDate: '2020-01-01T00:00:00.000000Z',
            submissionCount: 3,
            unseen: 2,
          }),
        ],
        continuationToken: null,
      }),
    );
    const found = await extras.fileRequests(account());
    expect(found.state).toBe('open');
    expect(found.webUrl).toBe(`${WEB}/#/file-requests`);
    expect(found.requests).toEqual([
      expect.objectContaining({
        id: 'r1',
        label: 'label of 2.r1',
        expired: false,
        manageUrl: `${WEB}/#/file-requests/r1`,
      }),
      expect.objectContaining({
        id: 'r2',
        label: null,
        expired: true,
        submissionCount: 3,
        unseen: 2,
      }),
    ]);
    // The key was opened once, for everything after it too.
    await extras.fileRequests(account());
    expect(core.openExtras).toHaveBeenCalledTimes(1);
  });

  it('copy their link on the request’s send domain, else the main host', async () => {
    route('GET', `${WEB}/uwu/v1/keys`, () =>
      json({ object: 'uwuKeys', extrasKey: {}, lost: false }),
    );
    route('GET', `${WEB}/uwu/v1/file-requests`, () =>
      json({
        object: 'list',
        data: [request('r1', { sendDomainId: 'd1' }), request('r2', { sendDomainId: 'gone' })],
      }),
    );
    await extras.fileRequests(account());
    await extras.copyFileRequestLink(account(), 'r1');
    await extras.copyFileRequestLink(account(), 'r2');
    expect(copied).toEqual(['https://send.example.com|access-r1|true', `${WEB}|access-r2|false`]);
  });

  it('say when the extras key isn’t there', async () => {
    extrasState = 'none';
    route('GET', `${WEB}/uwu/v1/keys`, () => json({ object: 'uwuKeys', extrasKey: null }));
    const found = await extras.fileRequests(account());
    expect(found).toEqual({ state: 'none', requests: [], webUrl: `${WEB}/#/file-requests` });
    expect(calls.map((c) => c.url)).toEqual([`${WEB}/uwu/v1/keys`]);
    await expect(extras.copyFileRequestLink(account(), 'r1')).rejects.toMatchObject({
      kind: 'uwu:no_extras_key',
    });
  });

  it('find an extras key made elsewhere after the next sync', async () => {
    extrasState = 'none';
    route('GET', `${WEB}/uwu/v1/keys`, () => json({ object: 'uwuKeys', extrasKey: null }));
    route('GET', `${WEB}/uwu/v1/file-requests`, () => json({ object: 'list', data: [] }));
    expect((await extras.fileRequests(account())).state).toBe('none');
    expect((await extras.fileRequests(account())).state).toBe('none');
    expect(core.openExtras).toHaveBeenCalledTimes(1);
    // The desktop app made it; a sync opens the vault again.
    extrasState = 'open';
    const synced = vi.spyOn(vault, 'generation').mockReturnValue(vault.generation() + 1);
    try {
      expect((await extras.fileRequests(account())).state).toBe('open');
      expect(core.openExtras).toHaveBeenCalledTimes(2);
      // An open key stays open until the vault closes: no more asking.
      synced.mockReturnValue(vault.generation() + 1);
      await extras.fileRequests(account());
      expect(core.openExtras).toHaveBeenCalledTimes(2);
    } finally {
      synced.mockRestore();
    }
  });

  it('are off where the server has none', async () => {
    await expect(extras.fileRequests(account({ uwu: null }))).rejects.toMatchObject({
      kind: 'uwu:feature_off',
    });
    expect(calls).toHaveLength(0);
  });
});

// ── Icons ─────────────────────────────────────────────────

describe('icons', () => {
  const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47]);

  it('are the own one first, then the server’s automatic one, in batches of 500', async () => {
    items = Array.from({ length: 501 }, (_, i) => ({
      id: `i${i}`,
      host: i === 1 ? 'shop.example.com' : i === 2 ? 'router.lan' : i === 3 ? '192.0.2.1' : null,
      organizationId: null,
    }));
    items[0]!.host = 'mine.example.com';
    route('GET', `${WEB}/uwu/v1/keys`, () =>
      json({ object: 'uwuKeys', extrasKey: {}, lost: false }),
    );
    route('POST', `${WEB}/uwu/v1/icons/own/get`, (call) => {
      const ids = (call.body as { cipherIds: string[] }).cipherIds;
      return json({
        object: 'list',
        data: ids
          .filter((id) => id === 'i0' || id === 'i500')
          .map((id) => ({ object: 'ownIcon', cipherId: id, keyType: 'extras', data: `2.${id}` })),
      });
    });
    route(
      'GET',
      /\/icons\/[^/]+\/icon\.png$/,
      () => new Response(png, { headers: { 'Content-Type': 'image/png' } }),
    );

    const found = await icons(
      account(),
      items.map((i) => i.id),
    );
    const batches = calls
      .filter((c) => c.url.endsWith('/icons/own/get'))
      .map((c) => (c.body as { cipherIds: string[] }).cipherIds.length);
    expect(batches).toEqual([500, 1]);
    expect(found.i0).toBe('data:image/png;base64,png-of-2.i0');
    expect(found.i500).toBe('data:image/png;base64,png-of-2.i500');
    expect(found.i1).toBe(`data:image/png;base64,${Buffer.from(png).toString('base64')}`);
    expect(Object.keys(found).sort()).toEqual(['i0', 'i1', 'i500']);
    // Only the one host the server would look up; the own icon wins over its host.
    expect(calls.filter((c) => c.url.endsWith('/icon.png')).map((c) => c.url)).toEqual([
      `${WEB}/icons/shop.example.com/icon.png`,
    ]);

    // Asked again: from memory.
    calls = [];
    await icons(account(), ['i0', 'i1']);
    expect(calls).toHaveLength(0);
  });

  it('come only from UwULock Server, and only while the setting is on', async () => {
    items = [{ id: 'i1', host: 'shop.example.com', organizationId: null }];
    expect(await icons(account({ uwu: null }), ['i1'])).toEqual({});
    local.settings = { showIcons: false };
    expect(await icons(account(), ['i1'])).toEqual({});
    expect(calls).toHaveLength(0);
  });

  it('are not fetched for addresses and local names', () => {
    expect(iconHost('Shop.Example.com.')).toBe('shop.example.com');
    expect(iconHost('192.0.2.1')).toBeNull();
    expect(iconHost('nas.local')).toBeNull();
    expect(iconHost('printer')).toBeNull();
    expect(iconHost('shop.example')).toBeNull();
    expect(iconHost(null)).toBeNull();
  });
});

// ── Feature switches ──────────────────────────────────────

describe('feature switches', () => {
  function info(switches: Record<string, boolean> | undefined) {
    return json({
      object: 'info',
      name: 'UwULock Server',
      version: '0.6.0',
      features: ['vault', 'masked-addresses', 'own-icons', 'file-requests', 'send-domains'],
      ...(switches ? { switches } : {}),
      sendDomains: [{ id: 'd1', url: 'https://send.example.com' }],
      icons: { automatic: true, url: `${WEB}/icons` },
    });
  }

  it('hide what is off, and an older server without switches offers what it lists', async () => {
    route('GET', `${WEB}/uwu/v1/info`, () =>
      info({ 'masked-addresses': false, 'send-domains': false, 'own-icons': true }),
    );
    const found = await uwuInfo(account());
    expect(found?.features).toEqual(['vault', 'own-icons', 'file-requests']);
    expect(found?.sendDomains).toEqual([]);
    expect(found?.switches).toMatchObject({ 'masked-addresses': false, 'own-icons': true });
    expect(hasFeature(account({ uwu: found }), 'masked-addresses')).toBe(false);

    routes = [];
    route('GET', `${WEB}/uwu/v1/info`, () => info(undefined));
    const older = await uwuInfo(account());
    expect(older?.features).toContain('masked-addresses');
    expect(older?.switches).toBeNull();
  });

  it('a feature_off answer asks the server again, once, and the extra goes away', async () => {
    local.accounts = [account()];
    route('GET', `${WEB}/uwu/v1/keys`, () =>
      json({ object: 'uwuKeys', extrasKey: {}, lost: false }),
    );
    route('GET', `${WEB}/uwu/v1/info`, () => info({ 'file-requests': false }));
    route('GET', /\/uwu\/v1\/file-requests/, () =>
      json(
        { message: 'This is switched off on this server.', object: 'error', code: 'feature_off' },
        404,
      ),
    );
    for (let i = 0; i < 3; i++) {
      const error = await extras.fileRequests(account()).catch((e: unknown) => e);
      expect(failure(error)).toMatchObject({ kind: 'uwu:feature_off' });
    }
    await vi.waitFor(async () => {
      const stored = (local.accounts as Account[])[0]!;
      expect(stored.uwu?.features).not.toContain('file-requests');
    });
    // Three refusals in a row, one question.
    expect(calls.filter((c) => c.url.endsWith('/uwu/v1/info'))).toHaveLength(1);
    // Now it is known: nothing is asked for any more.
    calls = [];
    const stored = (local.accounts as Account[])[0]!;
    await expect(extras.fileRequests(stored)).rejects.toMatchObject({ kind: 'uwu:feature_off' });
    expect(calls).toHaveLength(0);
  });

  it('are asked again on the alarm only every few minutes, and a change is stored', async () => {
    const mine = account({ id: 'u2' });
    local.accounts = [mine];
    route('GET', `${WEB}/uwu/v1/info`, () => info({ 'masked-addresses': false }));
    await vault.refreshInfo(mine, vault.INFO_EVERY_MS);
    await vault.refreshInfo(mine, vault.INFO_EVERY_MS);
    expect(calls.filter((c) => c.url.endsWith('/uwu/v1/info'))).toHaveLength(1);
    expect((local.accounts as Account[])[0]!.uwu?.features).not.toContain('masked-addresses');
    // Bitwarden's clouds have nothing to ask.
    calls = [];
    await vault.refreshInfo(account({ id: 'u3', uwu: null }));
    expect(calls).toHaveLength(0);
  });

  it('own icons fetched before stop showing once they are switched off', async () => {
    items = [{ id: 'i0', host: 'shop.example.com', organizationId: null }];
    route('GET', `${WEB}/uwu/v1/keys`, () =>
      json({ object: 'uwuKeys', extrasKey: {}, lost: false }),
    );
    route('POST', `${WEB}/uwu/v1/icons/own/get`, () =>
      json({
        object: 'list',
        data: [{ object: 'ownIcon', cipherId: 'i0', keyType: 'extras', data: '2.i0' }],
      }),
    );
    route(
      'GET',
      /\/icons\/[^/]+\/icon\.png$/,
      () => new Response(new Uint8Array([1]), { headers: { 'Content-Type': 'image/png' } }),
    );
    expect((await icons(account(), ['i0'])).i0).toBe('data:image/png;base64,png-of-2.i0');
    const off = account();
    off.uwu = { ...off.uwu!, features: off.uwu!.features.filter((f) => f !== 'own-icons') };
    calls = [];
    // The server's automatic icon instead (kept from before, or fetched now).
    const shown = (await icons(off, ['i0'])).i0;
    expect(shown).toMatch(/^data:image\/png;base64,/);
    expect(shown).not.toBe('data:image/png;base64,png-of-2.i0');
    expect(calls.some((c) => c.url.endsWith('/icons/own/get'))).toBe(false);
  });
});

// ── Passkeys ──────────────────────────────────────────────

describe('deleting a passkey', () => {
  it('saves the item the core answers, checked against the credential', async () => {
    await vault.open(account(), JSON.stringify({ profile: { id: 'u1' } }));
    route('PUT', `${WEB}/api/ciphers/item-1`, () => json({ id: 'item-1' }));
    await vault.deletePasskey('item-1', 0, 'cred-1');
    expect(core.deletePasskey).toHaveBeenCalledWith('item-1', 0, 'cred-1');
    const put = calls.find((c) => c.method === 'PUT')!;
    expect(put.url).toBe(`${WEB}/api/ciphers/item-1`);
    expect(put.body).toMatchObject({ type: 1, name: '2.name', encryptedFor: 'u1' });
  });

  it('refuses an index that is none, and needs the vault open', async () => {
    await vault.open(account(), JSON.stringify({ profile: { id: 'u1' } }));
    await expect(vault.deletePasskey('item-1', -1, null)).rejects.toMatchObject({
      kind: 'invalid',
    });
    expect(core.deletePasskey).not.toHaveBeenCalled();
    vault.closed();
    await expect(vault.deletePasskey('item-1', 0, null)).rejects.toMatchObject({
      kind: 'locked',
    });
  });
});
