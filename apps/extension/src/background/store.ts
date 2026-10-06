/**
 * What the background keeps, and where.
 *
 * - `storage.local` (on disk): the accounts — server, address, KDF, the user key *wrapped under
 *   the master key* as the server hands it out, the session tokens — this browser's device id
 *   and the settings. Never a key that opens anything by itself.
 * - IndexedDB (on disk): the last sync of each account, as the server sent it — every value in
 *   it still encrypted — so unlocking works offline and the popup opens at once.
 * - `storage.session` (in memory only, gone when the browser closes, and closed to content
 *   scripts): the user key while the vault is unlocked. A Manifest V3 service worker is ended
 *   after half a minute without work; when the next event wakes it, it opens the vault again
 *   from here instead of asking for the master password. Also the PIN-wrapped user key (unless
 *   the PIN is to survive a restart), logins waiting to be saved, and when the vault was last
 *   used.
 */

import { ext } from '../shared/browser';
import type { ServerChoice, Settings, UwuInfo } from '../shared/protocol';
import { endpoints } from './server';

export type Account = {
  /**
   * This browser's key for the account: its server's identity endpoint and its user id
   * (`accountKey`). Two servers may name the same user id — a hostile one can copy another's —
   * and never share an entry, with its protected user key and its remember-me token.
   */
  id: string;
  /** The user id from the access token (`sub`), as its server names it. */
  userId: string;
  email: string;
  name: string | null;
  server: ServerChoice;
  /** `{"kdf","kdfIterations","kdfMemory","kdfParallelism"}`, as the WebAssembly takes it. */
  kdf: string;
  /** The user key, wrapped under the stretched master key (the profile's `key`). */
  protectedKey: string | null;
  accessToken: string;
  refreshToken: string;
  /** Milliseconds since 1970. */
  expiresAt: number;
  lastSync: number | null;
  /**
   * Up to 0.5.0-beta.1: the "remember this device" token for two-step login. It went with the
   * account when the server ended the session; now it is kept in `rememberTokens` and this stays
   * null (an old one is still read, and moved at the next login).
   */
  rememberToken: string | null;
  /** The PIN-wrapped user key, when the PIN is to work after a browser restart too. */
  pinProtected: string | null;
  /** The server logged this browser out; shown once on the login screen. */
  sessionExpired?: boolean;
  /** UwULock Server's `/uwu/v1/info`, or null for other servers. */
  uwu: UwuInfo | null;
};

type Local = {
  accounts: Account[];
  activeAccount: string | null;
  deviceId: string;
  settings: Settings;
  /**
   * The KDF each account's last login accepted, by identity endpoint and address: a prelogin
   * asking for less is refused (session.ts). Kept when the server ends a session, which a
   * hostile server can do at will; only logging out here, or forgetting it in the popup, drops it.
   */
  kdfFloors: Record<string, string>;
  /**
   * The "remember this device" token of two-step login, by identity endpoint and address. Like
   * the KDF floors it outlives the account entry: kept when the server ends the session or the
   * vault locks, so the next login skips the code; only logging out here forgets it.
   */
  rememberTokens: Record<string, string>;
  /**
   * Wrong PINs in a row, per account. On disk, so a browser restart doesn't give a guesser five
   * new tries; the fifth removes the PIN.
   */
  pinAttempts: Record<string, number>;
};

type Session = {
  /** The unlocked account and its user key (base64 of the 64 bytes). */
  unlocked: { accountId: string; userKey: string } | null;
  lastActive: number;
  /** The PIN-wrapped user key until the browser restarts. */
  pin: { accountId: string; protected: string } | null;
  /** Logins sent while locked; see autofill.ts. */
  pendingSaves: unknown[];
  /** A login in progress: what the next step needs (the master password hash, not the password). */
  pendingLogin: unknown;
  /** The generator's last passwords, until the browser closes. */
  generated: { password: string; date: number }[];
};

export async function local<K extends keyof Local>(key: K): Promise<Local[K] | undefined> {
  const found = await ext.storage.local.get(key);
  return found[key] as Local[K] | undefined;
}

export async function setLocal<K extends keyof Local>(key: K, value: Local[K]): Promise<void> {
  await ext.storage.local.set({ [key]: value });
}

export async function session<K extends keyof Session>(key: K): Promise<Session[K] | undefined> {
  const found = await ext.storage.session.get(key);
  return found[key] as Session[K] | undefined;
}

export async function setSession<K extends keyof Session>(
  key: K,
  value: Session[K],
): Promise<void> {
  await ext.storage.session.set({ [key]: value });
}

export async function removeSession(...keys: (keyof Session)[]): Promise<void> {
  await ext.storage.session.remove(keys);
}

/**
 * Session storage for the extension's own pages and workers only. Chromium's default already
 * is this, and Firefox never shows session storage to content scripts; said anyway, in case a
 * default changes.
 */
export async function closeSessionToContentScripts(): Promise<void> {
  const area = ext.storage.session as chrome.storage.StorageArea & {
    setAccessLevel?: (options: { accessLevel: string }) => Promise<void>;
  };
  await area.setAccessLevel?.({ accessLevel: 'TRUSTED_CONTEXTS' }).catch(() => undefined);
}

// ── Accounts ──────────────────────────────────────────────

export async function accounts(): Promise<Account[]> {
  return (await local('accounts')) ?? [];
}

export async function account(id: string | null | undefined): Promise<Account | null> {
  if (!id) return null;
  return (await accounts()).find((a) => a.id === id) ?? null;
}

export async function activeAccount(): Promise<Account | null> {
  return account(await local('activeAccount'));
}

/** The key of the account `userId` on the server whose identity endpoint is `identity`. */
export function accountKey(identity: string, userId: string): string {
  return `${identity} ${userId}`;
}

/**
 * Up to 0.3.0-beta.1 accounts were kept by the token's user id alone. Each is given its key by
 * its own server, and everything kept under the old id moves with it: the open account, the
 * unlocked vault and the PIN of this browser session, wrong PIN tries and the cached sync. Two
 * old entries can't have shared a key (the id was unique), so nothing is merged.
 */
export async function migrateAccounts(): Promise<void> {
  const list = await accounts();
  if (list.every((a) => typeof a.userId === 'string')) return;
  const moved = new Map<string, string>();
  const next = list.map((a) => {
    if (typeof a.userId === 'string') return a;
    const id = accountKey(endpoints(a.server).identity, a.id);
    moved.set(a.id, id);
    return { ...a, id, userId: a.id };
  });
  await setLocal('accounts', next);
  const rename = (id: string | null | undefined) => (id ? (moved.get(id) ?? id) : id);
  const active = await local('activeAccount');
  if (active) await setLocal('activeAccount', rename(active) ?? null);
  const attempts = await local('pinAttempts');
  if (attempts) {
    await setLocal(
      'pinAttempts',
      Object.fromEntries(Object.entries(attempts).map(([id, n]) => [rename(id)!, n])),
    );
  }
  const unlocked = await session('unlocked');
  if (unlocked)
    await setSession('unlocked', { ...unlocked, accountId: rename(unlocked.accountId)! });
  const pin = await session('pin');
  if (pin) await setSession('pin', { ...pin, accountId: rename(pin.accountId)! });
  for (const [old, id] of moved) {
    const text = await cachedSync(old);
    if (text) await cacheSync(id, text);
    await forgetSync(old);
  }
}

export async function saveAccount(next: Account): Promise<void> {
  const list = await accounts();
  const at = list.findIndex((a) => a.id === next.id);
  if (at >= 0) list[at] = next;
  else list.push(next);
  await setLocal('accounts', list);
}

export async function updateAccount(id: string, patch: Partial<Account>): Promise<Account | null> {
  const found = await account(id);
  if (!found) return null;
  const next = { ...found, ...patch };
  await saveAccount(next);
  return next;
}

export async function removeAccount(id: string): Promise<void> {
  await setLocal(
    'accounts',
    (await accounts()).filter((a) => a.id !== id),
  );
  await setPinAttempts(id, 0);
  await forgetSync(id);
}

// ── The KDF each account's login accepted ─────────────────

function floorKey(identity: string, email: string): string {
  return `${identity} ${email}`;
}

export async function kdfFloor(identity: string, email: string): Promise<string | null> {
  return (await local('kdfFloors'))?.[floorKey(identity, email)] ?? null;
}

export async function setKdfFloor(identity: string, email: string, kdf: string): Promise<void> {
  await setLocal('kdfFloors', { ...(await local('kdfFloors')), [floorKey(identity, email)]: kdf });
}

export async function forgetKdfFloor(identity: string, email: string): Promise<void> {
  const floors = { ...(await local('kdfFloors')) };
  delete floors[floorKey(identity, email)];
  await setLocal('kdfFloors', floors);
}

// ── "Remember this device" ────────────────────────────────

export async function rememberToken(identity: string, email: string): Promise<string | null> {
  return (await local('rememberTokens'))?.[floorKey(identity, email)] ?? null;
}

export async function setRememberToken(
  identity: string,
  email: string,
  token: string,
): Promise<void> {
  await setLocal('rememberTokens', {
    ...(await local('rememberTokens')),
    [floorKey(identity, email)]: token,
  });
}

export async function forgetRememberToken(identity: string, email: string): Promise<void> {
  const tokens = { ...(await local('rememberTokens')) };
  delete tokens[floorKey(identity, email)];
  await setLocal('rememberTokens', tokens);
}

// ── Wrong PINs ────────────────────────────────────────────

export async function pinAttempts(accountId: string): Promise<number> {
  return (await local('pinAttempts'))?.[accountId] ?? 0;
}

export async function setPinAttempts(accountId: string, attempts: number): Promise<void> {
  const all = { ...(await local('pinAttempts')) };
  if (attempts > 0) all[accountId] = attempts;
  else delete all[accountId];
  await setLocal('pinAttempts', all);
}

/** This browser, as a device of the account: made once. */
export async function deviceId(): Promise<string> {
  let id = await local('deviceId');
  if (!id) {
    id = crypto.randomUUID();
    await setLocal('deviceId', id);
  }
  return id;
}

// ── The last sync, in IndexedDB ───────────────────────────

const DB = 'uwulock';
const SYNCS = 'syncs';

function database(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(DB, 1);
    open.onupgradeneeded = () => open.result.createObjectStore(SYNCS);
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
}

async function transaction<T>(
  mode: IDBTransactionMode,
  work: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  const db = await database();
  try {
    return await new Promise<T>((resolve, reject) => {
      const request = work(db.transaction(SYNCS, mode).objectStore(SYNCS));
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  } finally {
    db.close();
  }
}

/** The sync's JSON text, still encrypted. */
export async function cachedSync(accountId: string): Promise<string | null> {
  const value = await transaction<unknown>('readonly', (store) => store.get(accountId)).catch(
    () => null,
  );
  return typeof value === 'string' ? value : null;
}

export async function cacheSync(accountId: string, text: string): Promise<void> {
  await transaction('readwrite', (store) => store.put(text, accountId)).catch(() => undefined);
}

export async function forgetSync(accountId: string): Promise<void> {
  await transaction('readwrite', (store) => store.delete(accountId)).catch(() => undefined);
}
