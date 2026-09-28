/**
 * Requests to an account's server, the way Bitwarden's browser extension makes them: client
 * `browser`, the device type of this browser, this browser's device id. An access token that is
 * about to run out is renewed with the refresh token first; a refresh the server refuses means
 * the session is over (logged out elsewhere, a new master password, the device removed).
 */

import type { Failure } from '../shared/protocol';
import { endpoints } from './server';
import { deviceType } from './server';
import { type Account, updateAccount } from './store';

/** Bitwarden's servers unlock features by client version; this is what the extension speaks. */
export const CLIENT_VERSION = '2025.8.0';

export class ApiError extends Error {
  status: number;
  body: unknown;
  constructor(status: number, message: string, body: unknown) {
    super(message);
    this.status = status;
    this.body = body;
  }
}

/** Errors as `{ kind, message }`, whatever threw them. */
export function failure(error: unknown): Failure {
  if (error instanceof ApiError) {
    const kind =
      error.status === 401 ? 'session-expired' : error.status === 0 ? 'network' : 'server';
    return { kind, message: error.message };
  }
  if (typeof error === 'object' && error !== null && 'kind' in error && 'message' in error)
    return { kind: String(error.kind), message: String(error.message) };
  if (error instanceof Error) return { kind: 'unknown', message: error.message };
  return { kind: 'unknown', message: String(error) };
}

export function headers(extra?: Record<string, string>): Record<string, string> {
  return {
    Accept: 'application/json',
    'Bitwarden-Client-Name': 'browser',
    'Bitwarden-Client-Version': CLIENT_VERSION,
    'Device-Type': String(deviceType().kind),
    ...extra,
  };
}

/** The message a person reads, from whatever the server answered. */
export function messageOf(status: number, body: unknown): string {
  if (body && typeof body === 'object') {
    const value = body as Record<string, unknown>;
    const model = (value.errorModel ?? value.ErrorModel) as Record<string, unknown> | undefined;
    const message = (model?.message ??
      model?.Message ??
      value.message ??
      value.Message ??
      value.error_description) as string | undefined;
    if (message && message !== 'invalid_grant') return message;
  }
  if (status === 0) return 'The server does not answer.';
  return `The server answered with HTTP ${status}.`;
}

export async function parse(response: Response): Promise<unknown> {
  const text = await response.text();
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

type Options = {
  method?: string;
  body?: unknown;
  form?: URLSearchParams;
  extraHeaders?: Record<string, string>;
};

/** A request without a session: prelogin, the token endpoint, two-step mails. */
export async function anonymous<T = unknown>(url: string, options: Options = {}): Promise<T> {
  const extra: Record<string, string> = { ...options.extraHeaders };
  if (options.form) extra['Content-Type'] = 'application/x-www-form-urlencoded; charset=utf-8';
  else if (options.body !== undefined) extra['Content-Type'] = 'application/json';
  let response: Response;
  try {
    response = await fetch(url, {
      method: options.method ?? (options.body !== undefined || options.form ? 'POST' : 'GET'),
      headers: headers(extra),
      body: options.form ?? (options.body !== undefined ? JSON.stringify(options.body) : undefined),
      credentials: 'omit',
      cache: 'no-store',
    });
  } catch {
    throw new ApiError(0, messageOf(0, null), null);
  }
  const body = await parse(response);
  if (!response.ok) throw new ApiError(response.status, messageOf(response.status, body), body);
  return body as T;
}

/** Called when a refresh is refused: the background closes the vault and forgets the tokens. */
let onSessionEnded: (account: Account) => void = () => undefined;

export function whenSessionEnds(handler: (account: Account) => void) {
  onSessionEnded = handler;
}

const refreshing = new Map<string, Promise<Account>>();

/** A new access token from the refresh token. */
export function refresh(account: Account): Promise<Account> {
  let running = refreshing.get(account.id);
  if (!running) {
    running = (async () => {
      const form = new URLSearchParams({
        grant_type: 'refresh_token',
        client_id: 'browser',
        refresh_token: account.refreshToken,
      });
      let body: Record<string, unknown>;
      try {
        body = await anonymous<Record<string, unknown>>(
          `${endpoints(account.server).identity}/connect/token`,
          { form },
        );
      } catch (error) {
        if (error instanceof ApiError && (error.status === 400 || error.status === 401)) {
          onSessionEnded(account);
          throw new ApiError(401, 'The session has ended. Log in again.', error.body);
        }
        throw error;
      }
      const next = await updateAccount(account.id, {
        accessToken: String(body.access_token),
        refreshToken: String(body.refresh_token ?? account.refreshToken),
        expiresAt:
          Date.now() + Math.min(Math.max(Number(body.expires_in ?? 3600), 60), 86_400) * 1000,
      });
      return next ?? account;
    })().finally(() => refreshing.delete(account.id));
    refreshing.set(account.id, running);
  }
  return running;
}

/** A current access token, renewed first when it is about to run out. */
export async function freshAccount(account: Account): Promise<Account> {
  if (account.expiresAt - Date.now() < 120_000) return refresh(account);
  return account;
}

/**
 * A request with the account's session to its API (`/api/…` paths go to the API server, the
 * rest as given). Throws `ApiError` for anything but 2xx.
 */
export async function request<T = unknown>(
  account: Account,
  path: string,
  options: Options = {},
): Promise<T> {
  const base = endpoints(account.server);
  const url = path.startsWith('/api/')
    ? `${base.api}${path.slice(4)}`
    : path.startsWith('/identity/')
      ? `${base.identity}${path.slice(9)}`
      : `${base.web}${path}`;
  let current = await freshAccount(account);
  const send = (token: string) =>
    anonymous<T>(url, {
      ...options,
      extraHeaders: { ...options.extraHeaders, Authorization: `Bearer ${token}` },
    });
  try {
    return await send(current.accessToken);
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) {
      current = await refresh(current);
      return send(current.accessToken);
    }
    throw error;
  }
}

/** The claims of an access token (a JWT), unverified: only to read the user's id and address. */
export function claims(token: string): Record<string, unknown> {
  try {
    const part = token.split('.')[1] ?? '';
    const text = atob(part.replace(/-/g, '+').replace(/_/g, '/'));
    const bytes = Uint8Array.from(text, (c) => c.charCodeAt(0));
    return JSON.parse(new TextDecoder().decode(bytes)) as Record<string, unknown>;
  } catch {
    return {};
  }
}
