/**
 * Which items belong to a page: Bitwarden's URI match detection, the same rules its apps use,
 * so an item saved in either one shows up on the same sites in the other.
 *
 * Every address of a login has a match detection, or none and then the account's default:
 *
 * | 0 | Domain            | same registrable domain (`accounts.example.com` ~ `example.com`),  |
 * |   |                   | or one the server lists as equivalent (`google.com` ~ `youtube.com`) |
 * | 1 | Host              | same host name and port                                            |
 * | 2 | Starts with       | the page's address starts with it                                  |
 * | 3 | Exact             | the page's address is exactly it                                   |
 * | 4 | Regular expression| it matches the page's address (case-insensitive)                   |
 * | 5 | Never             | never offered                                                      |
 *
 * The registrable domain comes from the Public Suffix List (tldts), so `a.github.io` and
 * `b.github.io` are different sites while `a.example.co.uk` and `b.example.co.uk` are one.
 */

import { getDomain, getHostname, parse } from 'tldts';

export const MATCH_DOMAIN = 0;
export const MATCH_HOST = 1;
export const MATCH_STARTS_WITH = 2;
export const MATCH_EXACT = 3;
export const MATCH_REGEX = 4;
export const MATCH_NEVER = 5;

export type MatchUri = { uri: string; match: number | null };

/** The server's equivalent domains: the account's own and Bitwarden's global ones. */
export type EquivalentDomains = string[][];

/** An address someone typed without a scheme still has a host: Bitwarden reads it as http. */
function withScheme(uri: string): string {
  const trimmed = uri.trim();
  return /^[a-z][a-z0-9+.-]*:/i.test(trimmed) ? trimmed : `http://${trimmed}`;
}

function url(uri: string): URL | null {
  try {
    return new URL(withScheme(uri));
  } catch {
    return null;
  }
}

/**
 * The registrable domain of an address — `example.co.uk` for `https://a.b.example.co.uk/x`.
 * An IP address, `localhost` or a name without a public suffix is its own domain.
 */
export function domainOf(uri: string): string | null {
  const parsed = url(uri);
  if (!parsed || !/^https?:$|^ftp:$/.test(parsed.protocol)) return null;
  const host = parsed.hostname.replace(/^\[|\]$/g, '').toLowerCase();
  if (!host) return null;
  const info = parse(host, { allowPrivateDomains: true });
  if (info.isIp) return host;
  return getDomain(host, { allowPrivateDomains: true }) ?? host;
}

/** Host name and port, as `Host` compares them: `example.com:8443`. */
export function hostOf(uri: string): string | null {
  const parsed = url(uri);
  if (!parsed || !parsed.host) return null;
  return parsed.host.toLowerCase();
}

/** The host name without port, for showing: `github.com`. */
export function hostnameOf(uri: string): string | null {
  const parsed = url(uri);
  if (!parsed) return null;
  return getHostname(parsed.href) ?? (parsed.hostname || null);
}

/** Every domain that counts as the page's own: its registrable domain and its equivalents. */
export function matchingDomains(pageUrl: string, equivalents: EquivalentDomains): Set<string> {
  const domain = domainOf(pageUrl);
  const out = new Set<string>();
  if (!domain) return out;
  out.add(domain);
  for (const group of equivalents) {
    if (group.some((entry) => entry.toLowerCase() === domain)) {
      for (const entry of group) out.add(entry.toLowerCase());
    }
  }
  return out;
}

/** A regular expression from the vault, without letting a broken one throw. */
function regexMatches(pattern: string, target: string): boolean {
  try {
    return new RegExp(pattern, 'i').test(target);
  } catch {
    return false;
  }
}

/**
 * Whether one saved address matches the page. `domains` is `matchingDomains(pageUrl, …)`,
 * computed once per page.
 */
export function uriMatches(
  saved: MatchUri,
  pageUrl: string,
  domains: Set<string>,
  defaultMatch: number = MATCH_DOMAIN,
): boolean {
  const uri = saved.uri.trim();
  if (!uri) return false;
  switch (saved.match ?? defaultMatch) {
    case MATCH_DOMAIN: {
      const domain = domainOf(uri);
      return domain !== null && domains.has(domain);
    }
    case MATCH_HOST: {
      const host = hostOf(uri);
      return host !== null && host === hostOf(pageUrl);
    }
    case MATCH_STARTS_WITH:
      return pageUrl.startsWith(uri);
    case MATCH_EXACT:
      return pageUrl === uri;
    case MATCH_REGEX:
      return regexMatches(uri, pageUrl);
    default:
      return false;
  }
}

/** Whether any of an item's addresses match the page. */
export function itemMatches(
  uris: MatchUri[],
  pageUrl: string,
  domains: Set<string>,
  defaultMatch: number = MATCH_DOMAIN,
): boolean {
  return uris.some((saved) => uriMatches(saved, pageUrl, domains, defaultMatch));
}

/** The server's `domains` object (from the sync or `/api/settings/domains`), as groups. */
export function equivalentDomains(domains: unknown): EquivalentDomains {
  if (!domains || typeof domains !== 'object') return [];
  const value = domains as Record<string, unknown>;
  const own = (value.equivalentDomains ?? value.EquivalentDomains) as unknown;
  const global = (value.globalEquivalentDomains ?? value.GlobalEquivalentDomains) as unknown;
  const out: EquivalentDomains = [];
  if (Array.isArray(own)) {
    for (const group of own) {
      if (Array.isArray(group)) out.push(group.filter((d): d is string => typeof d === 'string'));
    }
  }
  if (Array.isArray(global)) {
    for (const entry of global) {
      if (!entry || typeof entry !== 'object') continue;
      const group = entry as Record<string, unknown>;
      if (group.excluded ?? group.Excluded) continue;
      const list = (group.domains ?? group.Domains) as unknown;
      if (Array.isArray(list)) out.push(list.filter((d): d is string => typeof d === 'string'));
    }
  }
  return out.filter((group) => group.length > 1);
}

/** Pages an extension may never fill: the browser's own, stores, other extensions. */
export function isFillableUrl(pageUrl: string | undefined | null): boolean {
  if (!pageUrl) return false;
  const parsed = url(pageUrl);
  if (!parsed || !/^https?:$/.test(parsed.protocol)) return false;
  const host = parsed.hostname.toLowerCase();
  return !(
    host === 'chrome.google.com' ||
    host === 'chromewebstore.google.com' ||
    host === 'addons.mozilla.org' ||
    host === 'microsoftedge.microsoft.com'
  );
}

/** Plain http, except to this computer itself, which is never on the wire. */
export function isInsecureUrl(pageUrl: string): boolean {
  const parsed = url(pageUrl);
  if (!parsed || parsed.protocol !== 'http:') return false;
  const host = parsed.hostname.replace(/^\[|\]$/g, '');
  return !(
    host === 'localhost' ||
    host.endsWith('.localhost') ||
    host === '127.0.0.1' ||
    host === '::1'
  );
}
