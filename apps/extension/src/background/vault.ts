/**
 * The open vault: syncing it, what the popup lists and shows, saving.
 *
 * The whole sync is fetched and opened in the WebAssembly module; after every change the vault
 * is synced again (one request), so what the popup shows is always the server's. Besides the
 * module's copy, the background keeps the autofill index — per item its name, addresses and
 * passkeys' metadata, never a secret — to match pages against without asking the module each
 * time, and the server's equivalent domains.
 */

import type {
  Draft,
  ItemDetail,
  ItemSummary,
  Overview,
  PasskeyInfo,
  TotpCode,
} from '../shared/protocol';
import { equivalentDomains, type EquivalentDomains, NO_EQUIVALENTS } from '../shared/uri';
import { changed } from './events';
import { ApiError, failure, MAX_SYNC_BYTES, request, whenFeatureOff } from './http';
import * as live from './live';
import { uwuInfo } from './uwu';
import { type Account, account as storedAccount, cacheSync, updateAccount } from './store';
import { call, callJson } from './wasm';

export type IndexPasskey = {
  credentialId: string;
  rpId: string;
  userName: string | null;
  userDisplayName: string | null;
  userHandle: string | null;
  discoverable: boolean;
  counter: number;
};

export type IndexEntry = {
  id: string;
  kind: ItemSummary['kind'];
  name: string;
  subtitle: string | null;
  favorite: boolean;
  reprompt: boolean;
  hasTotp: boolean;
  hasPassword: boolean;
  hasUsername: boolean;
  /** Archived items stay out of suggestions, as in Bitwarden. */
  archived?: boolean;
  uris?: { uri: string; match: number | null }[];
  passkeys?: IndexPasskey[];
};

let index: IndexEntry[] = [];
let equivalents: EquivalentDomains = NO_EQUIVALENTS;
let syncing = false;
let syncError: string | null = null;
/** The account whose vault is in the module. */
let current: Account | null = null;
let userId: string | null = null;
/** The server's revision date at the last sync: a newer one means something changed. */
let knownRevision: number | null = null;

/** Counts every vault opened (each sync) and closed: what is cached beside it knows when to go. */
let opened = 0;
let closings = 0;

/** Changes with every sync and every lock. */
export function generation(): number {
  return opened + closings;
}

/** Changes only when the vault closes (locked, logged out, another account). */
export function closedCount(): number {
  return closings;
}

export function syncState() {
  return { syncing, error: syncError };
}

export function autofillIndex(): IndexEntry[] {
  return index;
}

export function domains(): EquivalentDomains {
  return equivalents;
}

/** Open a sync (JSON text as the server sent it) in the module. */
export async function open(account: Account, text: string): Promise<void> {
  await call((core) => core.open(text));
  current = account;
  try {
    const parsed = JSON.parse(text) as Record<string, unknown>;
    equivalents = equivalentDomains(parsed.domains ?? parsed.Domains);
    const profile = (parsed.profile ?? parsed.Profile) as Record<string, unknown> | undefined;
    userId = (profile?.id ?? profile?.Id ?? null) as string | null;
  } catch {
    equivalents = NO_EQUIVALENTS;
  }
  index = await callJson<IndexEntry[]>((core) => core.autofillIndex());
  opened += 1;
  live.start(account);
  changed();
}

/** The vault closed: forget the index too. */
export function closed() {
  index = [];
  equivalents = NO_EQUIVALENTS;
  current = null;
  userId = null;
  knownRevision = null;
  syncError = null;
  closings += 1;
  live.stop();
}

/** Fetch the vault again and open it. */
export async function sync(account: Account): Promise<void> {
  syncing = true;
  changed();
  try {
    // Asked first: a change made while the sync runs shows up as newer the next time.
    const revision = await revisionDate(account).catch(() => null);
    const body = await request<Record<string, unknown>>(account, '/api/sync?excludeDomains=false', {
      maxBytes: MAX_SYNC_BYTES,
    });
    const text = JSON.stringify(body);
    await open(account, text);
    await cacheSync(account.id, text);
    const profile = (body.profile ?? body.Profile) as Record<string, unknown> | undefined;
    const key = (profile?.key ?? profile?.Key) as string | undefined;
    const name = (profile?.name ?? profile?.Name) as string | undefined;
    const uwu = await uwuInfo(account);
    const next = await updateAccount(account.id, {
      lastSync: Date.now(),
      ...(uwu !== undefined ? { uwu } : {}),
      ...(key ? { protectedKey: key } : {}),
      ...(name !== undefined ? { name: name ?? null } : {}),
    });
    if (next) current = next;
    knownRevision = revision;
    syncError = null;
  } catch (error) {
    syncError = failure(error).message;
    throw failure(error);
  } finally {
    syncing = false;
    changed();
  }
}

/** How often the minute alarm asks `/uwu/v1/info` again, for an admin's feature switches. */
export const INFO_EVERY_MS = 5 * 60_000;
/** After a `feature_off`, at most this often: a list of failing calls is one question. */
const INFO_AFTER_OFF_MS = 10_000;
const infoAskedAt = new Map<string, number>();

/**
 * Asks the server again what it offers (`/uwu/v1/info`), unless that was asked less than
 * `minAgeMs` ago, and tells the popup and the pages when it changed: an extra an admin switched
 * off goes away, one switched on appears. Bitwarden's hub, which this extension listens to, says
 * nothing about it; the minute alarm and `feature_off` answers bring it here.
 */
export async function refreshInfo(account: Account, minAgeMs = 0): Promise<void> {
  if (account.server.kind !== 'self-hosted' || !account.uwu) return;
  const now = Date.now();
  if (now - (infoAskedAt.get(account.id) ?? -Infinity) < minAgeMs) return;
  infoAskedAt.set(account.id, now);
  const uwu = await uwuInfo(account);
  if (uwu === undefined) return;
  const stored = await storedAccount(account.id);
  if (!stored || JSON.stringify(uwu) === JSON.stringify(stored.uwu ?? null)) return;
  const next = await updateAccount(account.id, { uwu });
  if (next && current?.id === next.id) current = next;
  changed();
}

// The server switched off something this extension still offered: ask what it offers now.
whenFeatureOff((account) => {
  void refreshInfo(account, INFO_AFTER_OFF_MS).catch(() => undefined);
});

async function revisionDate(account: Account): Promise<number | null> {
  const revision = await request<unknown>(account, '/api/accounts/revision-date');
  const date = typeof revision === 'number' ? revision : Date.parse(String(revision));
  return Number.isFinite(date) ? date : null;
}

/**
 * Whether the server has something newer than the last sync; if so, sync. Every minute, as a
 * fallback for the live connection (and the only way while it can't connect).
 */
export async function syncIfChanged(account: Account): Promise<void> {
  if (syncing) return;
  const date = await revisionDate(account);
  if (knownRevision === null || date === null || date > knownRevision) await sync(account);
}

function requireOpen(): Account {
  if (!current) throw { kind: 'locked', message: 'The vault is locked.' };
  return current;
}

export const overview = () => callJson<Overview>((core) => core.overview());
export const items = () => callJson<ItemSummary[]>((core) => core.items());
export const item = (id: string) => callJson<ItemDetail>((core) => core.item(id));
export const reveal = (id: string, field: string) =>
  call((core) => core.reveal(id, field, Date.now() / 1000));
export const totp = (id: string) => callJson<TotpCode>((core) => core.totp(id, Date.now() / 1000));
export const verifyReprompt = (id: string, password: string) =>
  call((core) => core.verifyReprompt(id, password));
/** Checks the master password of the open vault. */
export const checkPassword = (password: string) => call((core) => core.passwordHash(password));

// ── Saving ────────────────────────────────────────────────

/** A write, then a sync so the vault shows what the server has now. */
async function changedOnServer<T>(write: (account: Account) => Promise<T>): Promise<T> {
  const account = requireOpen();
  let result: T;
  try {
    result = await write(account);
  } catch (error) {
    if (error instanceof ApiError && error.status === 409)
      throw { kind: 'conflict', message: 'The item changed somewhere else. It was synced again.' };
    throw failure(error);
  } finally {
    await sync(account).catch(() => undefined);
  }
  return result;
}

/** A sealed item (`CipherRequest`) to the server: new without an id. Returns the item's id. */
export async function putCipher(
  id: string | null,
  sealed: Record<string, unknown>,
): Promise<string> {
  return changedOnServer(async (account) => {
    if (!id) {
      const created = await request<{ id?: string; Id?: string }>(account, '/api/ciphers', {
        body: { ...sealed, encryptedFor: userId ?? undefined },
      });
      return String(created.id ?? created.Id ?? '');
    }
    await request(account, `/api/ciphers/${encodeURIComponent(id)}`, {
      method: 'PUT',
      body: { ...sealed, encryptedFor: userId ?? undefined },
    });
    return id;
  });
}

export async function saveItem(id: string | null, draft: Draft): Promise<string> {
  requireOpen();
  const sealed = await callJson<Record<string, unknown>>((core) =>
    core.sealDraft(id ?? '', JSON.stringify(draft), new Date().toISOString()),
  );
  return putCipher(id, sealed);
}

/** A login's password changed, everything else kept, the old one into the history. */
export async function savePassword(id: string, password: string): Promise<void> {
  requireOpen();
  const sealed = await callJson<Record<string, unknown>>((core) =>
    core.sealPassword(id, password, new Date().toISOString()),
  );
  await putCipher(id, sealed);
}

/** An item's passkeys, nothing secret. */
export const itemPasskeys = (id: string) =>
  callJson<PasskeyInfo[]>((core) => core.itemPasskeys(id));

/**
 * Deletes a login's passkey at `index` — with `credentialId` only if it still is that one — and
 * saves the item, everything else as it was.
 */
export async function deletePasskey(
  id: string,
  index: number,
  credentialId: string | null,
): Promise<void> {
  requireOpen();
  if (!Number.isInteger(index) || index < 0) throw { kind: 'invalid', message: 'No such passkey.' };
  const { cipher } = await callJson<{ cipher: Record<string, unknown> }>((core) =>
    core.deletePasskey(id, index, typeof credentialId === 'string' ? credentialId : undefined),
  );
  await putCipher(id, cipher);
}

export async function setFavorite(id: string, favorite: boolean) {
  const found = (await items()).find((i) => i.id === id);
  if (!found) throw { kind: 'not-found', message: "This item isn't in the vault any more." };
  await changedOnServer((account) =>
    request(account, `/api/ciphers/${encodeURIComponent(id)}/partial`, {
      method: 'PUT',
      body: { folderId: found.folderId, favorite },
    }),
  );
}

export const deleteItem = (id: string, permanent: boolean) =>
  changedOnServer((account) =>
    permanent
      ? request(account, `/api/ciphers/${encodeURIComponent(id)}`, { method: 'DELETE' })
      : request(account, `/api/ciphers/${encodeURIComponent(id)}/delete`, { method: 'PUT' }),
  ).then(() => undefined);

export const restoreItem = (id: string) =>
  changedOnServer((account) =>
    request(account, `/api/ciphers/${encodeURIComponent(id)}/restore`, { method: 'PUT' }),
  ).then(() => undefined);

export async function saveFolder(id: string | null, name: string): Promise<string> {
  requireOpen();
  const encrypted = await call((core) => core.encryptText(name.trim()));
  const folder = await changedOnServer((account) =>
    id
      ? request<{ id?: string; Id?: string }>(account, `/api/folders/${encodeURIComponent(id)}`, {
          method: 'PUT',
          body: { name: encrypted },
        })
      : request<{ id?: string; Id?: string }>(account, '/api/folders', {
          body: { name: encrypted },
        }),
  );
  return String(folder.id ?? folder.Id ?? id ?? '');
}

export const deleteFolder = (id: string) =>
  changedOnServer((account) =>
    request(account, `/api/folders/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  ).then(() => undefined);
