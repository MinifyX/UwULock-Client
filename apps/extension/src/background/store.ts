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

export type Account = {
  /** The user id from the access token. */
  id: string;
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
  /** A "remember this device" token for two-step login. */
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
};

type Session = {
  /** The unlocked account and its user key (base64 of the 64 bytes). */
  unlocked: { accountId: string; userKey: string } | null;
  lastActive: number;
  /** The PIN-wrapped user key until the browser restarts. */
  pin: { accountId: string; protected: string } | null;
  pinAttempts: number;
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
  await forgetSync(id);
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
