/**
 * The open vault: syncing it, what the popup lists and shows, saving.
 *
 * The whole sync is fetched and opened in the WebAssembly module; after every change the vault
 * is synced again (one request), so what the popup shows is always the server's. Besides the
 * module's copy, the background keeps the autofill index — per item its name, addresses and
 * passkeys' metadata, never a secret — to match pages against without asking the module each
 * time, and the server's equivalent domains.
 */

import type { Draft, ItemDetail, ItemSummary, Overview, TotpCode } from '../shared/protocol';
import { equivalentDomains, type EquivalentDomains } from '../shared/uri';
import { changed } from './events';
import { ApiError, failure, request } from './http';
import * as live from './live';
import { uwuInfo } from './uwu';
import { type Account, cacheSync, updateAccount } from './store';
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
let equivalents: EquivalentDomains = [];
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
    equivalents = [];
  }
  index = await callJson<IndexEntry[]>((core) => core.autofillIndex());
  opened += 1;
  live.start(account);
  changed();
}

/** The vault closed: forget the index too. */
export function closed() {
  index = [];
  equivalents = [];
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
    const body = await request<Record<string, unknown>>(account, '/api/sync?excludeDomains=false');
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
