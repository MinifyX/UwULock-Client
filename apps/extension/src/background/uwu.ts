/**
 * UwULock Server says what it is at `/uwu/v1/info`; Bitwarden and Vaultwarden don't. Its own
 * features (masked addresses, the server's icons, file requests, sharing an item as a Send …)
 * are offered only when it lists them. Asked at login and again with every sync, so a server
 * that was updated shows its new features without logging in again.
 */

import { anonymous, ApiError } from './http';
import { endpoints } from './server';
import type { Account } from './store';

function lowerKeys(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object') return {};
  return Object.fromEntries(Object.entries(value).map(([k, v]) => [k.toLowerCase(), v]));
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
  return {
    version: typeof info.version === 'string' ? info.version : null,
    features: Array.isArray(info.features)
      ? info.features.filter((f): f is string => typeof f === 'string')
      : [],
  };
}
