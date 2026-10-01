/**
 * Which system the app runs on, for the few words and keys that differ.
 *
 * The webview's user agent says it plainly: WebView2 on Windows, WKWebView on
 * macOS and iOS, WebKitGTK on Linux, Chromium's WebView on Android. Android's
 * says Linux too and the iPhone's says Mac OS X, so those come first; an iPad
 * says Macintosh, but has a touch screen.
 */

export type Platform = 'windows' | 'macos' | 'linux' | 'android' | 'ios';

export function platform(): Platform {
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
