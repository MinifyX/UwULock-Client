/**
 * The WebExtension API, one name for both browsers. Firefox has `browser` (and `chrome` as an
 * alias), Chromium only `chrome`; both return promises in Manifest V3. Where they differ —
 * offscreen documents, session storage access levels, opening the popup — the code asks
 * whether the call exists.
 */

type Api = typeof chrome;

export const ext: Api =
  ((globalThis as unknown as { browser?: Api }).browser as Api | undefined) ?? globalThis.chrome;

export const isFirefox = typeof navigator !== 'undefined' && /Firefox\//.test(navigator.userAgent);

/**
 * Safari (macOS, iPhone, iPad): the extension inside the UwULock app. Its background is a page
 * that can't write the clipboard by itself, and passkeys come from UwULock's passkey provider
 * instead of the extension (scripts/manifest.mjs).
 */
export function isSafari(): boolean {
  try {
    return ext.runtime.getURL('/').startsWith('safari-web-extension:');
  } catch {
    return false;
  }
}

/** The extension's own origin: `chrome-extension://<id>` or `moz-extension://<uuid>`. */
export function ownOrigin(): string {
  return new URL(ext.runtime.getURL('/')).origin;
}
