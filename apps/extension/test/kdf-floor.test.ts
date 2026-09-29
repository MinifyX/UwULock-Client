// @vitest-environment node
/**
 * Logging in again (background/session.ts): the server's prelogin may not ask for a weaker key
 * derivation than the account's last login accepted, not even after the server ended the
 * session and the account is gone; logging out, or forgetting it by hand, accepts it again.
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

/** What prelogin answers, and every request the server got. */
let prelogin = { kdf: 0, kdfIterations: 600_000 };
const requests: string[] = [];
let sessionEnds: (account: Account) => void = () => undefined;

vi.mock('../src/background/http', async (original) => ({
  ...(await original<typeof import('../src/background/http')>()),
  anonymous: async (url: string) => {
    requests.push(url);
    if (url.endsWith('/accounts/prelogin')) return prelogin;
    return { access_token: 'token', refresh_token: 'refresh', expires_in: 3600 };
  },
  claims: () => ({ sub: 'user-1' }),
  request: async () => undefined,
  whenSessionEnds: (handler: (account: Account) => void) => {
    sessionEnds = handler;
  },
}));

const iterations = (kdf: string) => Number(JSON.parse(kdf).kdfIterations);
const core = {
  deriveLogin: vi.fn(() => 'hash'),
  // PBKDF2 only, which is all these tests use; the real rule has its own tests in Rust.
  kdfIsWeakerThan: vi.fn((kdf: string, stored: string) => iterations(kdf) < iterations(stored)),
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
const EMAIL = 'nyu@example.com';
/** The account's key here: its server's identity endpoint and its user id. */
const ID = store.accountKey('https://vault.example.com/identity', 'user-1');

async function waitFor(check: () => Promise<boolean>) {
  for (let i = 0; i < 100 && !(await check()); i++) await new Promise((r) => setTimeout(r, 0));
}

beforeEach(() => {
  for (const key of Object.keys(local)) delete local[key];
  for (const key of Object.keys(sessionStore)) delete sessionStore[key];
  requests.length = 0;
  core.deriveLogin.mockClear();
  prelogin = { kdf: 0, kdfIterations: 600_000 };
});

describe('a weaker KDF than the last login', () => {
  it('is refused before anything is derived or sent', async () => {
    expect(await session.login(SERVER, EMAIL, 'pw')).toMatchObject({ step: 'done' });
    prelogin = { kdf: 0, kdfIterations: 5_000 };
    requests.length = 0;
    core.deriveLogin.mockClear();
    await expect(session.login(SERVER, EMAIL, 'pw')).rejects.toMatchObject({ kind: 'weaker-kdf' });
    expect(core.deriveLogin).not.toHaveBeenCalled();
    expect(requests.every((url) => url.endsWith('/accounts/prelogin'))).toBe(true);
    // The same or more is fine.
    prelogin = { kdf: 0, kdfIterations: 700_000 };
    expect(await session.login(SERVER, EMAIL, 'pw')).toMatchObject({ step: 'done' });
  });

  it('stays refused after the server ended the session', async () => {
    await session.login(SERVER, EMAIL, 'pw');
    sessionEnds((await store.account(ID))!);
    await waitFor(async () => (await store.accounts()).length === 0);
    expect(await store.accounts()).toEqual([]);
    prelogin = { kdf: 0, kdfIterations: 5_000 };
    await expect(session.login(SERVER, EMAIL, 'pw')).rejects.toMatchObject({ kind: 'weaker-kdf' });
    // Another address, or another server, starts from nothing.
    await expect(session.login(SERVER, 'other@example.com', 'pw')).resolves.toBeTruthy();
  });

  it('is accepted after forgetting it by hand, which needs the account logged out', async () => {
    await session.login(SERVER, EMAIL, 'pw');
    await expect(session.forgetKdf(SERVER, EMAIL)).rejects.toMatchObject({ kind: 'invalid' });
    sessionEnds((await store.account(ID))!);
    await waitFor(async () => (await store.accounts()).length === 0);
    await session.forgetKdf(SERVER, ' NYU@example.com ');
    prelogin = { kdf: 0, kdfIterations: 5_000 };
    expect(await session.login(SERVER, EMAIL, 'pw')).toMatchObject({ step: 'done' });
  });

  it('is accepted after logging out on purpose', async () => {
    await session.login(SERVER, EMAIL, 'pw');
    await session.logout(ID);
    prelogin = { kdf: 0, kdfIterations: 5_000 };
    expect(await session.login(SERVER, EMAIL, 'pw')).toMatchObject({ step: 'done' });
  });
});
