// @vitest-environment node
/**
 * Unlocking with a PIN (background/session.ts): wrong tries are counted on disk, so a browser
 * restart gives no new ones; tries sent at once go one after the other and can't slip past the
 * count; a PIN kept across restarts needs six characters.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

type Account = {
  id: string;
  email: string;
  kdf: string;
  protectedKey: string;
  pinProtected: string | null;
};

/** storage.local and storage.session, shared across "restarts" of the background. */
const disk = new Map<string, unknown>();
const memory = new Map<string, unknown>();
const state = { running: 0, most: 0, tries: 0 };
const RIGHT = '246810';

vi.mock('../src/shared/browser', () => ({ ext: { runtime: { getURL: (p: string) => p } } }));
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
      pinProtect: (pin: string) => `wrapped:${pin}`,
      unlockWithPin: async (_e: string, _k: string, _p: string, pin: string) => {
        state.running += 1;
        state.most = Math.max(state.most, state.running);
        state.tries += 1;
        await new Promise((resolve) => setTimeout(resolve, 1));
        state.running -= 1;
        if (pin !== RIGHT) throw { kind: 'wrong-password', message: 'Wrong PIN.' };
      },
    }),
}));
vi.mock('../src/background/store', () => {
  const accounts = () => (disk.get('accounts') as Account[] | undefined) ?? [];
  const account = async (id: string | null | undefined) =>
    accounts().find((a) => a.id === id) ?? null;
  return {
    local: async (key: string) => disk.get(key),
    setLocal: async (key: string, value: unknown) => void disk.set(key, value),
    session: async (key: string) => memory.get(key),
    setSession: async (key: string, value: unknown) => void memory.set(key, value),
    removeSession: async (...keys: string[]) => keys.forEach((k) => memory.delete(k)),
    accounts: async () => accounts(),
    account,
    activeAccount: async () => account(disk.get('activeAccount') as string),
    updateAccount: async (id: string, patch: Partial<Account>) => {
      disk.set(
        'accounts',
        accounts().map((a) => (a.id === id ? { ...a, ...patch } : a)),
      );
    },
    pinAttempts: async (id: string) =>
      ((disk.get('pinAttempts') as Record<string, number> | undefined) ?? {})[id] ?? 0,
    setPinAttempts: async (id: string, n: number) => {
      const all = { ...((disk.get('pinAttempts') as Record<string, number> | undefined) ?? {}) };
      if (n > 0) all[id] = n;
      else delete all[id];
      disk.set('pinAttempts', all);
    },
    cachedSync: async () => null,
    saveAccount: async () => undefined,
    removeAccount: async () => undefined,
    deviceId: async () => 'device',
    kdfFloor: async () => null,
    setKdfFloor: async () => undefined,
    forgetKdfFloor: async () => undefined,
  };
});

/** The background, started (again). */
async function background() {
  vi.resetModules();
  return import('../src/background/session');
}

const pinOf = () => (disk.get('accounts') as Account[])[0]!.pinProtected;

beforeEach(() => {
  disk.clear();
  memory.clear();
  Object.assign(state, { running: 0, most: 0, tries: 0 });
  disk.set('accounts', [
    {
      id: 'user-1',
      email: 'nyu@example.com',
      server: { kind: 'bitwarden-us' },
      kdf: '{}',
      protectedKey: 'key',
      pinProtected: 'wrapped',
    },
  ]);
  disk.set('activeAccount', 'user-1');
});

describe('PIN tries', () => {
  it('survive a browser restart', async () => {
    let session = await background();
    for (let i = 0; i < 3; i++) await expect(session.unlockWithPin('0000')).rejects.toBeTruthy();
    memory.clear();
    session = await background();
    await expect(session.unlockWithPin('0000')).rejects.toMatchObject({ kind: 'wrong-password' });
    await expect(session.unlockWithPin('0000')).rejects.toMatchObject({ kind: 'pin-cleared' });
    expect(pinOf()).toBeNull();
    expect(state.tries).toBe(5);
  });

  it('go one at a time and stop at five, even when sent at once', async () => {
    const session = await background();
    const results = await Promise.allSettled(
      Array.from({ length: 12 }, () => session.unlockWithPin('0000')),
    );
    expect(state.most).toBe(1);
    expect(state.tries).toBe(5);
    expect(results.every((r) => r.status === 'rejected')).toBe(true);
    expect(pinOf()).toBeNull();
  });

  it('start again after the right PIN', async () => {
    const session = await background();
    for (let i = 0; i < 4; i++) await expect(session.unlockWithPin('0000')).rejects.toBeTruthy();
    await session.unlockWithPin(RIGHT);
    expect(disk.get('pinAttempts')).toEqual({});
  });
});

describe('setting a PIN', () => {
  async function unlocked() {
    const session = await background();
    await session.unlockWithPin(RIGHT);
    return session;
  }

  it('needs six characters when it is kept across restarts', async () => {
    const session = await unlocked();
    await expect(session.setPin('12345', true)).rejects.toMatchObject({ kind: 'pin-too-short' });
    await session.setPin('123456', true);
    expect(pinOf()).toBe('wrapped:123456');
  });

  it('needs four until the browser closes', async () => {
    const session = await unlocked();
    await expect(session.setPin('123', false)).rejects.toMatchObject({ kind: 'pin-too-short' });
    await session.setPin('1234', false);
    expect(memory.get('pin')).toEqual({ accountId: 'user-1', protected: 'wrapped:1234' });
  });
});
