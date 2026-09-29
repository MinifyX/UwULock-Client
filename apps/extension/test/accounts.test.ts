// @vitest-environment node
/**
 * Accounts are kept by server and user id (background/store.ts, session.ts): a server that
 * names another server's user id gets an entry of its own and inherits nothing of the other —
 * not its protected user key, not its remember-me token. Accounts kept by the user id alone,
 * as up to 0.3.0-beta.1, are moved to their new key with everything kept under it.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

const local = new Map<string, unknown>();
const memory = new Map<string, unknown>();

function area(map: Map<string, unknown>) {
  return {
    get: async (key: string) => (map.has(key) ? { [key]: structuredClone(map.get(key)) } : {}),
    set: async (items: Record<string, unknown>) => {
      for (const [k, v] of Object.entries(items)) map.set(k, structuredClone(v));
    },
    remove: async (keys: string[]) => keys.forEach((k) => map.delete(k)),
  };
}

vi.mock('../src/shared/browser', () => ({
  ext: {
    storage: { local: area(local), session: area(memory) },
    permissions: { contains: async () => true },
    runtime: { getURL: (p: string) => p },
  },
}));
vi.mock('../src/background/events', () => ({ changed: () => undefined }));
vi.mock('../src/background/uwu', () => ({ uwuInfo: async () => null }));
vi.mock('../src/background/vault', () => ({
  open: async () => undefined,
  sync: async () => undefined,
  closed: () => undefined,
  syncState: () => ({ syncing: false, error: null }),
}));
vi.mock('../src/background/wasm', () => ({
  call: async (work: (core: Record<string, unknown>) => unknown) =>
    work({
      lock: () => undefined,
      userKey: () => 'user-key',
      deriveLogin: () => 'hash',
      kdfIsWeakerThan: () => false,
      unlock: () => {
        throw new Error('no master key');
      },
    }),
}));

/** The token each server hands out: the same user id on both. */
const SUB = '11111111-2222-3333-4444-555555555555';
const jwt = (claims: object) =>
  `e30.${Buffer.from(JSON.stringify(claims)).toString('base64url')}.sig`;
let tokenAnswer: Record<string, unknown> = {};

vi.mock('../src/background/http', async (original) => {
  const real = await original<typeof import('../src/background/http')>();
  return {
    ...real,
    anonymous: async (url: string) => {
      if (url.endsWith('/accounts/prelogin')) return { kdf: 0, kdfIterations: 600000 };
      if (url.endsWith('/connect/token')) return tokenAnswer;
      throw new Error(`unexpected ${url}`);
    },
    request: async () => undefined,
  };
});

const store = await import('../src/background/store');
const session = await import('../src/background/session');

const A = { kind: 'self-hosted' as const, url: 'https://lock.example.com' };
const B = { kind: 'self-hosted' as const, url: 'https://evil.example.net' };

beforeEach(() => {
  local.clear();
  memory.clear();
});

describe('accounts by server and user id', () => {
  it('a second server with the same user id inherits nothing', async () => {
    tokenAnswer = {
      access_token: jwt({ sub: SUB, email: 'nyu@example.com' }),
      refresh_token: 'refresh-a',
      key: 'protected-key-a',
      twofactortoken: 'remember-a',
    };
    await session.login(A, 'nyu@example.com', 'password');
    tokenAnswer = {
      access_token: jwt({ sub: SUB, email: 'nyu@example.com' }),
      refresh_token: 'refresh-b',
    };
    await session.login(B, 'nyu@example.com', 'password');

    const list = await store.accounts();
    expect(list).toHaveLength(2);
    const [a, b] = list;
    expect(a).toMatchObject({
      userId: SUB,
      protectedKey: 'protected-key-a',
      rememberToken: 'remember-a',
      refreshToken: 'refresh-a',
    });
    expect(b).toMatchObject({ userId: SUB, protectedKey: null, rememberToken: null });
    expect(a!.id).not.toBe(b!.id);
    expect(b!.id).toBe(store.accountKey('https://evil.example.net/identity', SUB));
  });

  it('a login again on the same server keeps what it had', async () => {
    tokenAnswer = {
      access_token: jwt({ sub: SUB }),
      refresh_token: 'r1',
      key: 'protected-key-a',
      twofactortoken: 'remember-a',
    };
    await session.login(A, 'nyu@example.com', 'password');
    tokenAnswer = { access_token: jwt({ sub: SUB }), refresh_token: 'r2' };
    await session.login(A, 'nyu@example.com', 'password');
    const list = await store.accounts();
    expect(list).toHaveLength(1);
    expect(list[0]).toMatchObject({ protectedKey: 'protected-key-a', rememberToken: 'remember-a' });
  });
});

describe('moving accounts kept by user id alone', () => {
  it('gives each its key and moves what was kept under the old id', async () => {
    local.set('accounts', [
      { id: SUB, email: 'nyu@example.com', server: A, kdf: '{}', protectedKey: 'k' },
      { id: 'other', email: 'mew@example.com', server: { kind: 'bitwarden-eu' }, kdf: '{}' },
    ]);
    local.set('activeAccount', SUB);
    local.set('pinAttempts', { [SUB]: 2 });
    memory.set('unlocked', { accountId: SUB, userKey: 'x' });
    memory.set('pin', { accountId: SUB, protected: 'p' });

    await store.migrateAccounts();

    const idA = store.accountKey('https://lock.example.com/identity', SUB);
    const idOther = store.accountKey('https://identity.bitwarden.eu', 'other');
    expect((await store.accounts()).map((a) => [a.id, a.userId])).toEqual([
      [idA, SUB],
      [idOther, 'other'],
    ]);
    expect(local.get('activeAccount')).toBe(idA);
    expect(local.get('pinAttempts')).toEqual({ [idA]: 2 });
    expect(memory.get('unlocked')).toEqual({ accountId: idA, userKey: 'x' });
    expect(memory.get('pin')).toEqual({ accountId: idA, protected: 'p' });

    // Once is enough: a second run changes nothing.
    await store.migrateAccounts();
    expect((await store.accounts())[0]!.id).toBe(idA);
  });
});
