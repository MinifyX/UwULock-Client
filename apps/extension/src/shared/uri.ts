/**
 * Which items belong to a page: Bitwarden's URI match detection, the same rules its apps use,
 * so an item saved in either one shows up on the same sites in the other.
 *
 * Every address of a login has a match detection, or none and then the account's default:
 *
 * | 0 | Domain            | same registrable domain (`accounts.example.com` ~ `example.com`),  |
 * |   |                   | or an equivalent one (`google.com` ~ `youtube.com`, see below)      |
 * | 1 | Host              | same host name and port                                            |
 * | 2 | Starts with       | same origin, and the page's address starts with it                 |
 * | 3 | Exact             | the page's address is exactly it                                   |
 * | 4 | Regular expression| it matches the page's address (case-insensitive), top frame only   |
 * | 5 | Never             | never offered                                                      |
 *
 * The registrable domain comes from the Public Suffix List (tldts), so `a.github.io` and
 * `b.github.io` are different sites while `a.example.co.uk` and `b.example.co.uk` are one.
 *
 * Where UwULock is stricter than Bitwarden (security review 0.3, CL-L11):
 *
 * - "Starts with" wants the page's origin to be the saved address's: `https://bank.example`
 *   doesn't match `https://bank.example.evil.test/`.
 * - A regular expression is tried only in the top frame, only when it is short and can't
 *   backtrack for long (`safeRegex`), and only on an address of reasonable length: an org
 *   member's regex can't stall every page and every frame.
 * - Equivalent domains are sent by the server unencrypted. Bitwarden's global list comes with
 *   the extension (global-domains.json, from Bitwarden's server; scripts/global-domains.mjs);
 *   of the server's global list only which groups the account switched off counts. The
 *   account's own groups, which it sets in the web vault, come only from the server: they are
 *   kept apart (`custom`), and a login that matches only through one of them is listed but never
 *   filled without being picked.
 */

import { getDomain, getHostname, parse } from 'tldts';
import GLOBAL from './global-domains.json';

export const MATCH_DOMAIN = 0;
export const MATCH_HOST = 1;
export const MATCH_STARTS_WITH = 2;
export const MATCH_EXACT = 3;
export const MATCH_REGEX = 4;
export const MATCH_NEVER = 5;

export type MatchUri = { uri: string; match: number | null };

/**
 * Equivalent domains: Bitwarden's global groups as the extension ships them, less those the
 * account switched off, and the account's own groups (from the server, see above).
 */
export type EquivalentDomains = { global: string[][]; custom: string[][] };

export const NO_EQUIVALENTS: EquivalentDomains = { global: [], custom: [] };

/** Bitwarden's global groups, shipped with the extension, by their type number. */
export const GLOBAL_DOMAINS: { type: number; domains: string[] }[] = GLOBAL.groups;

/** Regular expressions longer than this are not tried. */
export const MAX_REGEX_LENGTH = 500;
/** Nor on page addresses longer than this. */
export const MAX_REGEX_TARGET = 2048;
/** Repeats without an upper bound (`*`, `+`, `{n,}`) a pattern may have. */
const MAX_UNBOUNDED = 2;

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
export function matchingDomains(pageUrl: string, equivalents: string[][]): Set<string> {
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

/**
 * The domains a page matches by: `strict` its own and Bitwarden's global equivalents, `wide`
 * also the account's own groups from the server.
 */
export function pageDomains(
  pageUrl: string,
  equivalents: EquivalentDomains,
): { strict: Set<string>; wide: Set<string> } {
  return {
    strict: matchingDomains(pageUrl, equivalents.global),
    wide: matchingDomains(pageUrl, [...equivalents.global, ...equivalents.custom]),
  };
}

/**
 * Whether a pattern is cheap to try on any address: short, at most `MAX_UNBOUNDED` repeats
 * without an upper bound, no repeat inside a repeated group (`(a+)+`), and no alternatives
 * inside one (`(a|aa)*`) — the shapes that backtrack for exponential or high polynomial time.
 * A heuristic that errs on the side of not matching: such an item can still be picked in the
 * popup.
 */
export function safeRegex(pattern: string): boolean {
  if (pattern.length > MAX_REGEX_LENGTH) return false;
  /** Per open group: whether it holds a repeat, and whether it holds an alternative. */
  const groups: { repeat: boolean; alternative: boolean }[] = [];
  let closed: { repeat: boolean; alternative: boolean } | null = null;
  let unbounded = 0;
  let inClass = false;
  for (let i = 0; i < pattern.length; i++) {
    const c = pattern[i]!;
    const after = closed;
    closed = null;
    if (c === '\\') {
      i += 1;
      continue;
    }
    if (inClass) {
      if (c === ']') inClass = false;
      continue;
    }
    if (c === '[') {
      inClass = true;
      continue;
    }
    if (c === '(') {
      groups.push({ repeat: false, alternative: false });
      continue;
    }
    if (c === '|') {
      const top = groups[groups.length - 1];
      if (top) top.alternative = true;
      continue;
    }
    if (c === ')') {
      const group = groups.pop() ?? { repeat: false, alternative: false };
      const top = groups[groups.length - 1];
      if (top && group.repeat) top.repeat = true;
      closed = group;
      continue;
    }
    let repeats = c === '*' || c === '+';
    let endless = repeats;
    if (c === '{') {
      const bound = /^\{(\d+)(,(\d*))?\}/.exec(pattern.slice(i));
      if (bound) {
        const [whole, lower, comma, upper] = bound;
        endless = Boolean(comma) && upper === '';
        repeats = endless || Number(comma ? upper : lower) > 1;
        i += whole.length - 1;
      }
    }
    if (!repeats) continue;
    if (endless && ++unbounded > MAX_UNBOUNDED) return false;
    if (after && (after.repeat || after.alternative)) return false;
    const top = groups[groups.length - 1];
    if (top) top.repeat = true;
  }
  return true;
}

/** A regular expression from the vault, without letting a broken or slow one run. */
function regexMatches(pattern: string, target: string): boolean {
  if (target.length > MAX_REGEX_TARGET || !safeRegex(pattern)) return false;
  try {
    return new RegExp(pattern, 'i').test(target);
  } catch {
    return false;
  }
}

/** The saved address has an origin, and it is the page's. */
function sameOrigin(saved: string, pageUrl: string): boolean {
  const a = url(saved);
  const b = url(pageUrl);
  return !!a && !!b && a.origin !== 'null' && a.origin === b.origin;
}

/** What a match may use beyond the address: regular expressions only in the top frame. */
export type MatchOptions = { regex?: boolean };

/**
 * Whether one saved address matches the page. `domains` is `matchingDomains(pageUrl, …)`,
 * computed once per page.
 */
export function uriMatches(
  saved: MatchUri,
  pageUrl: string,
  domains: Set<string>,
  defaultMatch: number = MATCH_DOMAIN,
  options: MatchOptions = {},
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
      return sameOrigin(uri, pageUrl) && pageUrl.startsWith(uri);
    case MATCH_EXACT:
      return pageUrl === uri;
    case MATCH_REGEX:
      return options.regex !== false && regexMatches(uri, pageUrl);
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
  options: MatchOptions = {},
): boolean {
  return uris.some((saved) => uriMatches(saved, pageUrl, domains, defaultMatch, options));
}

/**
 * The server's `domains` object (from the sync or `/api/settings/domains`): which of Bitwarden's
 * global groups the account switched off, and its own groups. The server's copy of the global
 * groups is not used — the extension has its own.
 */
export function equivalentDomains(domains: unknown): EquivalentDomains {
  const value = (domains && typeof domains === 'object' ? domains : {}) as Record<string, unknown>;
  const own = (value.equivalentDomains ?? value.EquivalentDomains) as unknown;
  const global = (value.globalEquivalentDomains ?? value.GlobalEquivalentDomains) as unknown;
  const listed = (value.excludedGlobalEquivalentDomains ??
    value.ExcludedGlobalEquivalentDomains) as unknown;
  const off = new Set<number>();
  if (Array.isArray(listed)) for (const n of listed) if (typeof n === 'number') off.add(n);
  if (Array.isArray(global)) {
    for (const entry of global) {
      if (!entry || typeof entry !== 'object') continue;
      const group = entry as Record<string, unknown>;
      const type = group.type ?? group.Type;
      if ((group.excluded ?? group.Excluded) === true && typeof type === 'number') off.add(type);
    }
  }
  const custom: string[][] = [];
  if (Array.isArray(own)) {
    for (const group of own) {
      if (!Array.isArray(group)) continue;
      const list = group
        .filter((d): d is string => typeof d === 'string')
        .map((d) => d.trim().toLowerCase())
        .filter(Boolean);
      if (list.length > 1) custom.push(list);
    }
  }
  return {
    global: GLOBAL_DOMAINS.filter((group) => !off.has(group.type)).map((group) => group.domains),
    custom,
  };
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
