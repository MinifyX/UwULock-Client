/**
 * Icons for the vault list (contract §7): an item's own icon, else the server's automatic icon
 * for its site, else the popup's glyph. Only on UwULock Server, and only while the setting is
 * on.
 *
 * - Own icons come encrypted (`POST /uwu/v1/icons/own/get`, at most 500 ids a call) and are
 *   opened in the WebAssembly module: a personal item's under the extras key, an
 *   organisation's item's under the organisation's key.
 * - Automatic icons are fetched here from the account's own server
 *   (`GET <icons.url>/<host>/icon.png`), never from the site itself, and only when the server
 *   says it has them.
 *
 * The popup's content security policy names no server, so it gets data URLs. Both are kept in
 * memory: own icons until the vault changes, automatic ones while the background runs.
 */

import { hasFeature } from './uwu';
import { extrasKey } from './extras';
import { readBody, uwu } from './http';
import { endpoints } from './server';
import { settings } from './settings';
import type { Account } from './store';
import * as vault from './vault';
import { callJson } from './wasm';

/** Ids per `POST /uwu/v1/icons/own/get`. */
export const OWN_BATCH = 500;
/** Automatic icons fetched at the same time. */
const PARALLEL = 6;
/** Bigger answers are not an icon the server made (it makes PNGs of 64 × 64 at most). */
const MAX_ICON_BYTES = 256 * 1024;
const MAX_HOSTS = 2000;

/** Top-level names the server never fetches an icon for (§7.1): local and reserved ones. */
const RESERVED = new Set([
  'local',
  'lan',
  'home',
  'internal',
  'intranet',
  'localhost',
  'localdomain',
  'test',
  'invalid',
  'example',
  'onion',
  'arpa',
  'corp',
  'private',
]);

/** A host the server would look up an icon for, or null: no addresses, no local names. */
export function iconHost(host: string | null | undefined): string | null {
  if (!host) return null;
  const name = host.toLowerCase().replace(/\.$/, '');
  if (name.length > 253 || !/^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(name)) return null;
  if (/^\d+(\.\d+){3}$/.test(name)) return null;
  return RESERVED.has(name.slice(name.lastIndexOf('.') + 1)) ? null : name;
}

/** Where the server's automatic icons are: its `icons.url` if on the server's own host. */
export function iconsBase(account: Account): string | null {
  const icons = account.uwu?.icons;
  if (!icons?.automatic) return null;
  const web = endpoints(account.server).web;
  if (icons.url) {
    try {
      if (new URL(icons.url).host === new URL(web).host) return icons.url.replace(/\/+$/, '');
    } catch {
      // Not an address: the default place below.
    }
  }
  return `${web}/icons`;
}

function base64(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary);
}

const own = new Map<string, string | null>();
let ownGeneration = -1;
const automatic = new Map<string, Promise<string | null>>();

let running = 0;
const waiting: (() => void)[] = [];

async function limited<T>(work: () => Promise<T>): Promise<T> {
  if (running >= PARALLEL) await new Promise<void>((resolve) => waiting.push(resolve));
  running += 1;
  try {
    return await work();
  } finally {
    running -= 1;
    waiting.shift()?.();
  }
}

async function fetchAutomatic(base: string, host: string): Promise<string | null> {
  try {
    // Only from the account's own server: a redirect elsewhere is not followed.
    const response = await fetch(`${base}/${encodeURIComponent(host)}/icon.png`, {
      credentials: 'omit',
      redirect: 'error',
    });
    if (!response.ok) return null;
    if (!(response.headers.get('Content-Type') ?? '').startsWith('image/png')) return null;
    const bytes = await readBody(response, MAX_ICON_BYTES);
    if (!bytes.length) return null;
    return `data:image/png;base64,${base64(bytes)}`;
  } catch {
    return null;
  }
}

function automaticIcon(base: string, host: string): Promise<string | null> {
  const key = `${base} ${host}`;
  let found = automatic.get(key);
  if (!found) {
    if (automatic.size >= MAX_HOSTS) automatic.clear();
    found = limited(() => fetchAutomatic(base, host));
    automatic.set(key, found);
  }
  return found;
}

async function loadOwn(account: Account, ids: string[]): Promise<void> {
  if (!ids.length) return;
  // Without the extras key, organisations' icons still open.
  await extrasKey(account).catch(() => undefined);
  for (let at = 0; at < ids.length; at += OWN_BATCH) {
    const batch = ids.slice(at, at + OWN_BATCH);
    let opened: { cipherId: string; png: string }[] = [];
    try {
      const answer = await uwu<unknown>(account, '/icons/own/get', {
        body: { cipherIds: batch },
      });
      const list = Array.isArray(answer)
        ? answer
        : ((answer as { data?: unknown } | null)?.data ?? []);
      opened = await callJson<{ cipherId: string; png: string }[]>((core) =>
        core.openIcons(JSON.stringify(Array.isArray(list) ? list : [])),
      );
    } catch {
      // Not now; the next time the list is shown asks again.
      return;
    }
    const found = new Map(opened.map((icon) => [icon.cipherId, icon.png]));
    for (const id of batch) {
      const png = found.get(id);
      own.set(id, png ? `data:image/png;base64,${png}` : null);
    }
  }
}

/** Data URLs for those of `ids` that have an icon. */
export async function icons(account: Account, ids: string[]): Promise<Record<string, string>> {
  if (!account.uwu || !(await settings()).showIcons) return {};
  if (vault.generation() !== ownGeneration) {
    own.clear();
    ownGeneration = vault.generation();
  }
  const summaries = new Map((await vault.items()).map((item) => [item.id, item]));
  const wanted = [...new Set(ids)].filter((id) => summaries.has(id)).slice(0, 2 * OWN_BATCH);
  // Switched off by the admin: own icons that were fetched before don't show either.
  const ownOn = hasFeature(account, 'own-icons');
  if (ownOn) {
    await loadOwn(
      account,
      wanted.filter((id) => !own.has(id)),
    );
  }
  const result: Record<string, string> = {};
  const base = iconsBase(account);
  const pending: Promise<void>[] = [];
  for (const id of wanted) {
    const mine = ownOn ? own.get(id) : undefined;
    if (mine) {
      result[id] = mine;
      continue;
    }
    const host = iconHost(summaries.get(id)?.host);
    if (!base || !host) continue;
    pending.push(
      automaticIcon(base, host).then((url) => {
        if (url) result[id] = url;
      }),
    );
  }
  await Promise.all(pending);
  return result;
}
