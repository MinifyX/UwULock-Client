// @vitest-environment node
/**
 * "Remember this device" (background/session.ts): two-step login once, then the remember token
 * skips the code at every later login of that address on that server — after the vault locked,
 * after the server ended the session (which removes the account entry), and for a token kept the
 * old way in the account. Logging out forgets it; a token the server refuses is dropped.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Account } from '../src/background/store';

const local: Record<string, unknown> = {};
const sessionStore: Record<string, unknown> = {};

vi.mock('../src/shared/browser', () => {
  const area = (data: Record<string, unknown>) => ({
    get: async (key: string) => ({ [key]: data[key] }),
    set: async (values: Record<string, unknown>) => Object.assign(data, values),
    remove: async (keys: string[]) => keys.forEach((key) => delete data[key]),
  });
  return {
    ext: {
      storage: { local: area(local), session: area(sessionStore) },
      permissions: { contains: async () => true },
      runtime: { sendMessage: async () => undefined },
    },
    isFirefox: false,
  };
});

/** The toy server: which remember tokens it takes, and every token request's form. */
const server = { taken: new Set<string>(), issued: 0 };
const forms: URLSearchParams[] = [];
let sessionEnds: (account: Account) => void = () => undefined;

vi.mock('../src/background/http', async (original) => {
  const real = await original<typeof import('../src/background/http')>();
  return {
    ...real,
    anonymous: async (url: string, options: { form?: URLSearchParams }) => {
      if (url.endsWith('/accounts/prelogin')) return { kdf: 0, kdfIterations: 600_000 };
      const form = options.form!;
      forms.push(new URLSearchParams(form));
      const provider = form.get('twoFactorProvider');
      const code = form.get('twoFactorToken');
      const remembered = provider === '5' && code !== null && server.taken.has(code);
      if (!remembered && !(provider === '0' && code === '123456')) {
        throw new real.ApiError(400, 'Two-step login required.', {
          TwoFactorProviders2: { '0': null },
        });
      }
      const body: Record<string, unknown> = {
        access_token: 'token',
        refresh_token: 'refresh',
        expires_in: 3600,
      };
      if (provider === '0' && form.get('twoFactorRemember') === '1') {
        const token = `remember-${++server.issued}`;
        server.taken.add(token);
        body.TwoFactorToken = token;
      }
      return body;
    },
    claims: () => ({ sub: 'user-1' }),
    request: async () => undefined,
    whenSessionEnds: (handler: (account: Account) => void) => {
      sessionEnds = handler;
    },
  };
});

const core = {
  deriveLogin: () => 'hash',
  kdfIsWeakerThan: () => false,
  lock: () => undefined,
};
vi.mock('../src/background/wasm', () => ({
  call: async <T>(work: (c: typeof core) => T) => work(core),
}));
vi.mock('../src/background/uwu', () => ({ uwuInfo: async () => null }));
vi.mock('../src/background/vault', () => ({
  closed: () => undefined,
  open: async () => undefined,
  sync: async () => undefined,
  syncState: () => ({ syncing: false, error: null }),
}));
vi.mock('../src/background/events', () => ({ changed: () => undefined }));

const session = await import('../src/background/session');
const store = await import('../src/background/store');

const SERVER = { kind: 'self-hosted', url: 'https://vault.example.com' } as const;
const IDENTITY = 'https://vault.example.com/identity';
const EMAIL = 'nyu@example.com';
const ID = store.accountKey(IDENTITY, 'user-1');

async function waitFor(check: () => Promise<boolean>) {
  for (let i = 0; i < 100 && !(await check()); i++) await new Promise((r) => setTimeout(r, 0));
}

/** A login that asks for the code, answered with "remember this device" as given. */
async function loginWithCode(remember: boolean) {
  expect(await session.login(SERVER, EMAIL, 'pw')).toMatchObject({ step: 'two-factor' });
  expect(await session.loginTwoFactor(0, '123 456', remember)).toMatchObject({ step: 'done' });
}

beforeEach(() => {
  for (const key of Object.keys(local)) delete local[key];
  for (const key of Object.keys(sessionStore)) delete sessionStore[key];
  forms.length = 0;
  server.taken.clear();
  server.issued = 0;
});

describe('remember this device', () => {
  it('skips the code at the next login, also after a lock', async () => {
    await loginWithCode(true);
    expect(await store.rememberToken(IDENTITY, EMAIL)).toBe('remember-1');
    expect((await store.account(ID))?.rememberToken).toBeNull();
    await session.lock();
    forms.length = 0;
    expect(await session.login(SERVER, EMAIL, 'pw')).toMatchObject({ step: 'done' });
    expect(forms[0]!.get('twoFactorProvider')).toBe('5');
    expect(forms[0]!.get('twoFactorToken')).toBe('remember-1');
    // The same device every time.
    const devices = new Set(forms.map((f) => f.get('deviceIdentifier')));
    expect(devices.size).toBe(1);
  });

  it('survives the server ending the session', async () => {
    await loginWithCode(true);
    const device = await store.deviceId();
    sessionEnds((await store.account(ID))!);
    await waitFor(async () => (await store.accounts()).length === 0);
    expect(await session.login(SERVER, EMAIL, 'pw')).toMatchObject({ step: 'done' });
    expect(forms.at(-1)?.get('deviceIdentifier')).toBe(device);
  });

  it('is forgotten by logging out', async () => {
    await loginWithCode(true);
    const device = await store.deviceId();
    await session.logout(ID);
    expect(await store.rememberToken(IDENTITY, EMAIL)).toBeNull();
    expect(await session.login(SERVER, EMAIL, 'pw')).toMatchObject({ step: 'two-factor' });
    expect(forms.at(-1)?.get('twoFactorToken')).toBeNull();
    // Logging out forgets the token, not the device.
    expect(await store.deviceId()).toBe(device);
  });

  it('logging out of one account keeps the token of another', async () => {
    const OTHER = { kind: 'self-hosted', url: 'https://other.example.org' } as const;
    const OTHER_IDENTITY = 'https://other.example.org/identity';
    await loginWithCode(true);
    expect(await session.login(OTHER, EMAIL, 'pw')).toMatchObject({ step: 'two-factor' });
    expect(await session.loginTwoFactor(0, '123456', true)).toMatchObject({ step: 'done' });
    expect(await store.rememberToken(OTHER_IDENTITY, EMAIL)).toBe('remember-2');
    await session.logout(ID);
    expect(await store.rememberToken(IDENTITY, EMAIL)).toBeNull();
    expect(await store.rememberToken(OTHER_IDENTITY, EMAIL)).toBe('remember-2');
  });

  it('a session ending after a logout brings no old token back', async () => {
    await loginWithCode(true);
    await store.forgetRememberToken(IDENTITY, EMAIL);
    await store.updateAccount(ID, { rememberToken: 'remember-1' });
    const ended = (await store.account(ID))!;
    await session.logout(ID);
    sessionEnds(ended);
    await new Promise((r) => setTimeout(r, 10));
    expect(await store.rememberToken(IDENTITY, EMAIL)).toBeNull();
  });

  it('asks again when the server no longer takes the token, and keeps the new one', async () => {
    await loginWithCode(true);
    server.taken.clear();
    expect(await session.login(SERVER, EMAIL, 'pw')).toMatchObject({ step: 'two-factor' });
    expect(await store.rememberToken(IDENTITY, EMAIL)).toBeNull();
    expect(await session.loginTwoFactor(0, '123456', true)).toMatchObject({ step: 'done' });
    expect(await store.rememberToken(IDENTITY, EMAIL)).toBe('remember-2');
    expect(await session.login(SERVER, EMAIL, 'pw')).toMatchObject({ step: 'done' });
  });

  it('keeps nothing when it is not asked for', async () => {
    await loginWithCode(false);
    expect(await store.rememberToken(IDENTITY, EMAIL)).toBeNull();
  });

  it('takes over a token kept in the account by an older version', async () => {
    await loginWithCode(true);
    // As 0.5.0-beta.1 kept it: in the account entry only.
    await store.forgetRememberToken(IDENTITY, EMAIL);
    await store.updateAccount(ID, { rememberToken: 'remember-1' });
    sessionEnds((await store.account(ID))!);
    await waitFor(async () => (await store.accounts()).length === 0);
    expect(await store.rememberToken(IDENTITY, EMAIL)).toBe('remember-1');
    expect(await session.login(SERVER, EMAIL, 'pw')).toMatchObject({ step: 'done' });

    // Read from the account directly too, and moved at that login.
    await store.forgetRememberToken(IDENTITY, EMAIL);
    await store.updateAccount(ID, { rememberToken: 'remember-1' });
    expect(await session.login(SERVER, EMAIL, 'pw')).toMatchObject({ step: 'done' });
    expect(await store.rememberToken(IDENTITY, EMAIL)).toBe('remember-1');
    expect((await store.account(ID))?.rememberToken).toBeNull();
  });
});
