/**
 * Which system the app runs on, for the few words and keys that differ.
 *
 * The webview's user agent says it plainly: WebView2 on Windows, WKWebView on
 * macOS and iOS, WebKitGTK on Linux, Chromium's WebView on Android. Android's
 * says Linux too and the iPhone's says Mac OS X, so those come first; an iPad
 * says Macintosh, but has a touch screen.
 */

export type Platform = 'windows' | 'macos' | 'linux' | 'android' | 'ios';

/**
 * The platform this build is for (Tauri's TAURI_ENV_PLATFORM: ios, android,
 * darwin, windows, linux); empty in `pnpm dev` and the tests. It wins over the
 * user agent for the phone builds: the iPhone/iPad app on an Apple silicon Mac
 * ("Designed for iPad") says "Macintosh" without touch, yet it is the iOS app,
 * shown by macOS at 77 % — with the desktop layout and sizes its text came out
 * at 10 pt, and it has no menu bar of its own to set.
 */
export const BUILD_PLATFORM: string = import.meta.env?.TAURI_ENV_PLATFORM ?? '';

/** The iPhone/iPad build running on a Mac. */
export function isIosAppOnMac(): boolean {
  return (
    BUILD_PLATFORM === 'ios' &&
    /Macintosh/.test(navigator.userAgent) &&
    navigator.maxTouchPoints <= 1
  );
}

export function platform(): Platform {
  if (BUILD_PLATFORM === 'ios') return 'ios';
  if (BUILD_PLATFORM === 'android') return 'android';
  const agent = `${navigator.userAgent} ${navigator.platform ?? ''}`.toLowerCase();
  if (agent.includes('android')) return 'android';
  if (/iphone|ipad|ipod/.test(agent)) return 'ios';
  if (agent.includes('mac') && navigator.maxTouchPoints > 1) return 'ios';
  if (agent.includes('win')) return 'windows';
  if (agent.includes('mac')) return 'macos';
  return 'linux';
}

/** Android or iOS: no window to move, no updater, the phone's back button. */
export function isMobile(): boolean {
  const p = platform();
  return p === 'android' || p === 'ios';
}

/** "Windows", "macOS", "Linux", "Android" or "iOS", as the system calls itself. */
export function systemName(): string {
  switch (platform()) {
    case 'windows':
      return 'Windows';
    case 'macos':
      return 'macOS';
    case 'android':
      return 'Android';
    case 'ios':
      return 'iOS';
    default:
      return 'Linux';
  }
}
