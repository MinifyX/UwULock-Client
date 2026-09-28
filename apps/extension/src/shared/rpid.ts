/**
 * The WebAuthn rules for which relying party a page may speak for.
 *
 * A page at `https://login.example.com` may create and use passkeys for `login.example.com`
 * or `example.com` — its own host or a registrable domain above it — but not for `com`, not
 * for `other.example`, and a page at `https://a.github.io` not for `github.io` (a public
 * suffix: every GitHub Pages site would share its passkeys). Only secure origins: https, or
 * http to this computer itself (`localhost`), as browsers allow it.
 */

import { getPublicSuffix, parse } from 'tldts';

export type RpCheck = { ok: true; rpId: string } | { ok: false; reason: string };

function isLocalhost(host: string): boolean {
  return host === 'localhost' || host.endsWith('.localhost');
}

/** The rpId a page at `origin` asks for (none: its own host), checked. */
export function checkRpId(origin: string, requested: string | undefined | null): RpCheck {
  let url: URL;
  try {
    url = new URL(origin);
  } catch {
    return { ok: false, reason: 'not an origin' };
  }
  const host = url.hostname.toLowerCase();
  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && isLocalhost(host))) {
    return { ok: false, reason: 'not a secure origin' };
  }
  if (!host || parse(host).isIp) return { ok: false, reason: 'an IP address has no passkeys' };
  const rpId = (requested ?? host).toLowerCase().replace(/\.$/, '');
  if (!rpId) return { ok: false, reason: 'empty rpId' };
  if (rpId !== host && !host.endsWith(`.${rpId}`)) {
    return { ok: false, reason: 'the rpId is not this site' };
  }
  if (isLocalhost(rpId)) return { ok: true, rpId };
  // A public suffix (`com`, `co.uk`, `github.io`) is nobody's to claim.
  const suffix = getPublicSuffix(rpId, { allowPrivateDomains: true });
  if (!suffix || suffix === rpId || !rpId.includes('.')) {
    return { ok: false, reason: 'the rpId is a public suffix' };
  }
  return { ok: true, rpId };
}
