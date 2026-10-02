/**
 * How a passkey's site is named (R4-6). The RP id is what the browser checked against the page's
 * origin; the RP name is whatever the site says about itself (`evil.example` can call itself
 * "PayPal"). So the RP id comes first, and the RP name only next to it.
 */

/** The name of a new login for a passkey: "rpName (rpId)", or the rpId alone. */
export function passkeyLoginName(rpId: string, rpName: string | null | undefined): string {
  const name = rpName?.trim();
  return name && name.toLowerCase() !== rpId.toLowerCase() ? `${name} (${rpId})` : rpId;
}
