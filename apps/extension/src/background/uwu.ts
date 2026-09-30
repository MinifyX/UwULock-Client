/**
 * UwULock Server says what it is at `/uwu/v1/info`; Bitwarden and Vaultwarden don't. Its own
 * features (masked addresses, the server's icons, file requests, sharing an item as a Send …)
 * are offered only when it lists them. Asked at login and again with every sync, so a server
 * that was updated shows its new features without logging in again. An admin may switch extras
 * off (`switches`); the background asks again every few minutes and whenever the server answers
 * `feature_off`, so what was switched off goes away without an error (vault.ts).
 */

import type { SendDomain, UwuInfo } from '../shared/protocol';
import { anonymous, ApiError } from './http';
import { endpoints } from './server';
import type { Account } from './store';

function lowerKeys(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object') return {};
  return Object.fromEntries(Object.entries(value).map(([k, v]) => [k.toLowerCase(), v]));
}

/** An absolute https address (or http to this computer), else null. */
function address(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  try {
    const url = new URL(value);
    const local = url.hostname === 'localhost' || url.hostname.endsWith('.localhost');
    if (url.protocol !== 'https:' && !(url.protocol === 'http:' && local)) return null;
    return `${url.origin}${url.pathname.replace(/\/+$/, '')}`;
  } catch {
    return null;
  }
}

function icons(value: unknown): UwuInfo['icons'] {
  const found = lowerKeys(value);
  if (found.automatic !== true) return null;
  return { automatic: true, url: address(found.url) };
}

function sendDomains(value: unknown): SendDomain[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((entry) => {
    const found = lowerKeys(entry);
    const url = address(found.url);
    return typeof found.id === 'string' && url ? [{ id: found.id, url }] : [];
  });
}

/**
 * The feature switches (`switches`, UwULock Server 0.6.0-beta.2): each extra an admin can switch
 * off, `true` when it works. Null from an older server, which has none.
 */
function switches(value: unknown): Record<string, boolean> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  return Object.fromEntries(
    Object.entries(value).filter(
      (entry): entry is [string, boolean] => typeof entry[1] === 'boolean',
    ),
  );
}

/** `undefined`: the server didn't answer, so what was known stays. */
export async function uwuInfo(found: Account): Promise<Account['uwu'] | undefined> {
  if (found.server.kind !== 'self-hosted') return null;
  let answer: unknown;
  try {
    answer = await anonymous(`${endpoints(found.server).web}/uwu/v1/info`);
  } catch (error) {
    return error instanceof ApiError && error.status !== 0 ? null : undefined;
  }
  const info = lowerKeys(answer);
  if (typeof info.name !== 'string' || !info.name.startsWith('UwULock')) return null;
  const switched = switches(info.switches);
  return {
    version: typeof info.version === 'string' ? info.version : null,
    // A server leaves out what is switched off; should one list it anyway, the switch wins.
    features: Array.isArray(info.features)
      ? info.features.filter((f): f is string => typeof f === 'string' && switched?.[f] !== false)
      : [],
    icons: icons(info.icons),
    sendDomains: switched?.['send-domains'] === false ? [] : sendDomains(info.senddomains),
    switches: switched,
  };
}

/** Whether the account's server is a UwULock Server that offers `feature`. */
export function hasFeature(found: Account | null, feature: string): boolean {
  return Boolean(found?.uwu?.features.includes(feature));
}
