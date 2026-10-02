// The manifest, for Chromium and for Firefox, from one description.
//
// The differences: Chromium runs the background as a service worker and needs an offscreen
// document for the clipboard; Firefox runs it as an event page (a background script), has no
// offscreen documents, and wants an add-on id. Everything else is the same.
//
// Browsers take only numbers as the version: 0.3.0-beta.1 becomes 0.3.0.1 (Chromium shows the
// full name through `version_name`). Nothing updates these files by itself — they are
// installed from the release by hand — so the order of betas and releases doesn't matter.

/** Numbers only: `0.3.0-beta.1` → `0.3.0.1`, `0.3.0` → `0.3.0`. */
export function numericVersion(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:-[a-z]+\.(\d+))?$/i.exec(version);
  if (!match) throw new Error(`Unexpected version ${version}`);
  return match
    .slice(1)
    .filter((part) => part !== undefined)
    .join('.');
}

const CSP = [
  "default-src 'self'",
  // The WebAssembly module with the vault's crypto; no other code than the extension's own.
  "script-src 'self' 'wasm-unsafe-eval'",
  "object-src 'none'",
  "base-uri 'none'",
  // The inline menu's list (menu.html) is shown in a frame in web pages. Which pages may load
  // an extension page at all is the manifest's web_accessible_resources: menu.html only — the
  // popup and the other pages stay unreachable from the web, whatever this line allows.
  "frame-ancestors 'self' https: http:",
  "img-src 'self' data:",
  "style-src 'self'",
  "font-src 'self'",
  // The server: any https address, or plain http to this computer; its live connection likewise.
  'connect-src https: wss: http://localhost:* http://127.0.0.1:* ws://localhost:* ws://127.0.0.1:*',
].join('; ');

/** WebAuthn only runs in secure contexts: https, and http to this computer. */
const SECURE_PAGES = ['https://*/*', 'http://localhost/*'];

export function manifest(browser, version) {
  const firefox = browser === 'firefox';
  const icons = {
    16: 'icons/icon-16.png',
    32: 'icons/icon-32.png',
    48: 'icons/icon-48.png',
    128: 'icons/icon-128.png',
  };
  return {
    manifest_version: 3,
    name: '__MSG_extName__',
    short_name: 'UwULock',
    description: '__MSG_extDescription__',
    default_locale: 'en',
    version: numericVersion(version),
    ...(firefox ? {} : { version_name: version, minimum_chrome_version: '116' }),
    homepage_url: 'https://github.com/MinifyX/UwULock-Client',
    icons,
    action: {
      default_popup: 'popup.html',
      default_title: 'UwULock',
      default_icon: { 16: icons[16], 32: icons[32], 48: icons[48] },
    },
    background: firefox
      ? { scripts: ['background.js'], type: 'module' }
      : { service_worker: 'background.js', type: 'module' },
    permissions: [
      'storage',
      'activeTab',
      'contextMenus',
      'alarms',
      'clipboardWrite',
      // Locking with the computer's screen (idle state `locked`).
      'idle',
      ...(firefox ? [] : ['offscreen']),
    ],
    // Asked for per server when somebody logs in: bitwarden.com, bitwarden.eu, or their own.
    optional_host_permissions: ['https://*/*', 'http://*/*'],
    content_scripts: [
      {
        matches: SECURE_PAGES,
        js: ['page.js'],
        run_at: 'document_start',
        all_frames: true,
        world: 'MAIN',
      },
      { matches: SECURE_PAGES, js: ['bridge.js'], run_at: 'document_start', all_frames: true },
      {
        matches: ['https://*/*', 'http://*/*'],
        js: ['content.js'],
        run_at: 'document_idle',
        all_frames: true,
      },
    ],
    commands: {
      _execute_action: {
        suggested_key: { default: 'Ctrl+Shift+Y', mac: 'Command+Shift+Y' },
        description: '__MSG_commandOpen__',
      },
      'autofill-login': {
        suggested_key: { default: 'Ctrl+Shift+L', mac: 'Command+Shift+L' },
        description: '__MSG_commandAutofill__',
      },
    },
    // The inline menu's list, in a frame under a field: the one page web pages may show.
    // Chromium gives it a new address each session, so a page can't use it to see UwULock.
    web_accessible_resources: [
      {
        resources: ['menu.html'],
        matches: ['https://*/*', 'http://*/*'],
        ...(firefox ? {} : { use_dynamic_url: true }),
      },
    ],
    content_security_policy: { extension_pages: CSP },
    ...(firefox
      ? {
          browser_specific_settings: {
            gecko: {
              id: 'uwulock@minifyx.de',
              // 140 (an ESR) is the first Firefox that asks for the data below at install.
              strict_min_version: '140.0',
              // Mozilla counts every byte that leaves the browser, end-to-end encrypted or not:
              // the login (email, master password hash, device) and the vault's items — logins,
              // identities and cards — go to the server the user signs in to, nowhere else
              // (PRIVACY.md).
              data_collection_permissions: {
                required: [
                  'authenticationInfo',
                  'personallyIdentifyingInfo',
                  'financialAndPaymentInfo',
                ],
              },
            },
          },
        }
      : {}),
  };
}
