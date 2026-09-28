/**
 * Where an account's vault lives: bitwarden.com, bitwarden.eu, or a self-hosted UwULock
 * Server, Vaultwarden or Bitwarden — and the addresses of its parts.
 *
 * The extension talks to the server from its background, across origins. That needs host
 * permission for the server, which it asks for when somebody logs in (optional host
 * permissions: nothing is granted for servers nobody uses).
 */

import type { ServerChoice, ServerKind } from '../shared/protocol';

export type Endpoints = {
  api: string;
  identity: string;
  notifications: string;
  web: string;
  /** Match patterns for the host permission. */
  origins: string[];
  label: string;
};

const CLOUD: Record<Exclude<ServerKind, 'self-hosted'>, string> = {
  'bitwarden-us': 'bitwarden.com',
  'bitwarden-eu': 'bitwarden.eu',
};

function isLoopback(host: string): boolean {
  const bare = host.replace(/^\[|\]$/g, '');
  return (
    bare === 'localhost' || bare.endsWith('.localhost') || bare === '::1' || /^127\./.test(bare)
  );
}

/**
 * A self-hosted server from what somebody typed: with or without `https://`, with or without
 * a path into the web vault. Plain `http://` only to this computer itself — anything else
 * would send the password hash and the session in the clear. Throws a message to show.
 */
export function normalizeServerUrl(input: string): string {
  let text = input.trim();
  if (!text) throw new Error('empty');
  if (!/^[a-z]+:\/\//i.test(text)) text = `https://${text}`;
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    throw new Error('not-a-url');
  }
  if (url.protocol === 'http:' && !isLoopback(url.hostname)) throw new Error('insecure');
  if (url.protocol !== 'https:' && url.protocol !== 'http:') throw new Error('not-a-url');
  if (!url.hostname) throw new Error('not-a-url');
  let path = url.pathname.replace(/\/+$/, '');
  for (const tail of ['/#', '/api', '/identity', '/vault', '/login']) {
    if (path.endsWith(tail)) path = path.slice(0, -tail.length);
  }
  return `${url.protocol}//${url.host}${path}`;
}

export function endpoints(server: ServerChoice): Endpoints {
  if (server.kind !== 'self-hosted') {
    const domain = CLOUD[server.kind];
    return {
      api: `https://api.${domain}`,
      identity: `https://identity.${domain}`,
      notifications: `https://notifications.${domain}`,
      web: `https://vault.${domain}`,
      origins: [`https://*.${domain}/*`],
      label: domain,
    };
  }
  const base = normalizeServerUrl(server.url ?? '');
  const url = new URL(base);
  return {
    api: `${base}/api`,
    identity: `${base}/identity`,
    notifications: `${base}/notifications`,
    web: base,
    origins: [`${url.protocol}//${url.hostname}/*`],
    label: `${url.host}${url.pathname === '/' ? '' : url.pathname}`,
  };
}

/** Bitwarden's device types for browser extensions, by browser. */
export function deviceType(): { kind: number; name: string } {
  const agent = typeof navigator === 'undefined' ? '' : navigator.userAgent;
  if (/Firefox\//.test(agent)) return { kind: 3, name: 'firefox' };
  if (/Edg\//.test(agent)) return { kind: 5, name: 'edge' };
  if (/OPR\//.test(agent)) return { kind: 4, name: 'opera' };
  if (/Vivaldi/.test(agent)) return { kind: 19, name: 'vivaldi' };
  return { kind: 2, name: 'chrome' };
}
