/**
 * The window never zooms: UwULock is an app, not a page, and a zoomed web view stays zoomed —
 * cut-off bars, a tab bar half off screen, sheets that no longer fit. The text size is the
 * app's own setting (Darstellung → Textgröße, lib/appearance.ts), which follows the system's.
 *
 * What keeps it from zooming, on every system:
 *
 * - index.html's viewport (`maximum-scale=1, user-scalable=no`): pinching and iOS's zoom into a
 *   focused field. WKWebView honours it (unlike Safari, it doesn't ignore the viewport's limits),
 *   and the fields are 16 px or more on iOS anyway (app.css).
 * - `touch-action: manipulation` on the page (index.css): no double-tap zoom.
 * - Here: Safari/WebKit's pinch gestures (`gesturestart`, iPad and Mac trackpads), a pinch on a
 *   trackpad that Chromium and WebView2 send as Ctrl + wheel, and Ctrl/⌘ with + − 0 where a
 *   web view would zoom by itself.
 * - tauri.conf.json's `zoomHotkeysEnabled: false` (WebView2's own zoom keys and Ctrl + wheel),
 *   and on Android the plugin switches the web view's zoom off (UwuLockMobilePlugin.kt).
 *
 * Kept pure where it can be (`isZoomKey`, `isZoomWheel`), tested in test/zoom.test.ts.
 */

type Keyish = Pick<KeyboardEvent, 'ctrlKey' | 'metaKey' | 'altKey' | 'key'>;
type Wheelish = Pick<WheelEvent, 'ctrlKey'>;

/** Ctrl/⌘ with +, −, = or 0 (and the number pad's): a browser's zoom keys. */
export function isZoomKey(event: Keyish): boolean {
  if (!(event.ctrlKey || event.metaKey) || event.altKey) return false;
  return ['+', '-', '=', '_', '0', 'Add', 'Subtract'].includes(event.key);
}

/** A trackpad pinch arrives as a wheel event with Ctrl held (also Ctrl + the mouse wheel). */
export function isZoomWheel(event: Wheelish): boolean {
  return event.ctrlKey;
}

let installed = false;

/** Once, before the first render. */
export function blockZoom(target: Window = window) {
  if (installed) return;
  installed = true;
  const stop = (event: Event) => event.preventDefault();
  // WebKit's own gesture events (iOS, iPadOS, macOS): not in the DOM's types.
  for (const type of ['gesturestart', 'gesturechange', 'gestureend'])
    target.addEventListener(type, stop, { passive: false });
  target.addEventListener(
    'wheel',
    (event) => {
      if (isZoomWheel(event)) event.preventDefault();
    },
    { passive: false },
  );
  target.addEventListener(
    'keydown',
    (event) => {
      if (isZoomKey(event)) event.preventDefault();
    },
    { capture: true },
  );
}
