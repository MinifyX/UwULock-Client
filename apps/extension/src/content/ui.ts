/**
 * What the content script shows in a page (the button and menu in fields, the notification
 * bar) lives in a closed shadow root on its own host element under `<html>`: the page can't
 * look inside, and its styles don't reach in. The host's own styles are set inline with
 * `!important`, which beats anything a page's style sheets say about it, and are put back when
 * the page's script changes them.
 *
 * The page can still draw over our UI, or make it nearly invisible, and trick somebody into
 * clicking it (clickjacking). `createGuard()` accepts a click only when it can tell that the
 * person saw what they clicked; see there.
 */

/**
 * The design's tokens and controls: in the page's shadow roots and in the menu's frame. The
 * values are @uwusuite/design's tokens (tokens.css, light and dark), written out: nothing of the
 * package's CSS reaches a web page. UwU Sans is first in the font list; in a page's shadow root
 * it isn't loaded (nothing there may fetch), so the system's font stands in. The menu's frame is
 * an extension page and has it (menu/main.ts).
 */
export const BASE_CSS = `
/* Custom properties are the one thing 'all: initial' doesn't reset. Without '!important' a
   page's rule for our host would win over ':host' and could make the menu see-through. */
:host {
  --uwu-surface: #ffffff !important;
  --uwu-elevated: #fcf8fa !important;
  --uwu-ink: #1c1420 !important;
  --uwu-muted: #716672 !important;
  --uwu-hairline: #f2e8ee !important;
  --uwu-border: #e9dde4 !important;
  --uwu-pink: #ff4d8d !important;
  --uwu-pink-solid: #e11d74 !important;
  --uwu-on-pink: #ffffff !important;
  --uwu-pink-ink: #a3154f !important;
  --uwu-pink-tint: #ffe4ef !important;
  --uwu-warning-ink: #8e5510 !important;
  --uwu-shadow: 0 12px 32px rgb(28 20 32 / 0.12), 0 2px 8px rgb(28 20 32 / 0.06) !important;
  --uwu-font: 'UwU Sans', system-ui, -apple-system, 'Segoe UI', Roboto, 'Helvetica Neue',
    'Noto Sans', Arial, sans-serif !important;
  color-scheme: light !important;
}
@media (prefers-color-scheme: dark) {
  :host {
    --uwu-surface: #1c171f !important;
    --uwu-elevated: #241e28 !important;
    --uwu-ink: #f8f2f6 !important;
    --uwu-muted: #b3a8b3 !important;
    --uwu-hairline: #2c2430 !important;
    --uwu-border: #3a3040 !important;
    --uwu-pink: #ff7fac !important;
    --uwu-pink-solid: #ff7fac !important;
    --uwu-on-pink: #1c1420 !important;
    --uwu-pink-ink: #ffa3c4 !important;
    --uwu-pink-tint: #3a1a2a !important;
    --uwu-warning-ink: #d8a25c !important;
    --uwu-shadow: 0 12px 32px rgb(0 0 0 / 0.45), 0 2px 8px rgb(0 0 0 / 0.3) !important;
    color-scheme: dark !important;
  }
}
* { box-sizing: border-box; }
.panel {
  font: 500 14px/1.4 var(--uwu-font);
  color: var(--uwu-ink);
  background: var(--uwu-surface);
  border: 1px solid var(--uwu-border);
  border-radius: 16px;
  box-shadow: var(--uwu-shadow);
  text-align: left;
  letter-spacing: normal;
}
button {
  font: inherit;
  margin: 0;
  cursor: pointer;
}
.primary, .secondary {
  border-radius: 999px;
  padding: 7px 14px;
  font-weight: 600;
  font-size: 13px;
  white-space: nowrap;
}
.primary {
  background: var(--uwu-pink-solid);
  color: var(--uwu-on-pink);
  border: 1px solid var(--uwu-pink-solid);
}
.secondary {
  background: transparent;
  color: var(--uwu-ink);
  border: 1px solid var(--uwu-border);
}
.secondary:hover { background: var(--uwu-elevated); }
button:disabled { opacity: 0.6; cursor: default; }
:focus { outline: none; }
:focus-visible { outline: 2px solid var(--uwu-pink); outline-offset: 2px; }
.muted { color: var(--uwu-muted); font-size: 13px; }
.alarm { color: var(--uwu-warning-ink); }
.actions { display: flex; flex-wrap: wrap; gap: 8px; }
`;

let sheet: CSSStyleSheet | null | undefined;

/** Our styles, as a constructed sheet (no page CSP applies) or else a `<style>` element. */
function addStyles(root: ShadowRoot, extra: string) {
  if (sheet === undefined) {
    try {
      sheet = new CSSStyleSheet();
      sheet.replaceSync(BASE_CSS);
    } catch {
      sheet = null;
    }
  }
  if (sheet) {
    try {
      const own = new CSSStyleSheet();
      own.replaceSync(extra);
      root.adoptedStyleSheets = [sheet, own];
      return;
    } catch {
      // fall back to a style element
    }
  }
  const style = document.createElement('style');
  style.textContent = BASE_CSS + extra;
  root.append(style);
}

export type Host = { host: HTMLElement; root: ShadowRoot };

/** Our hosts in this document, which may sit on top of each other. */
const hosts = new WeakSet<Element>();

function pinHost(host: HTMLElement) {
  host.removeAttribute('style');
  const set = (name: string, value: string) => host.style.setProperty(name, value, 'important');
  set('all', 'initial');
  set('position', 'fixed');
  set('top', '0');
  set('left', '0');
  set('width', '0');
  set('height', '0');
  set('overflow', 'visible');
  set('display', 'block');
  set('visibility', 'visible');
  set('opacity', '1');
  set('pointer-events', 'auto');
  set('z-index', '2147483647');
}

/** A host under `<html>`, on top of everything, with a closed shadow root and our styles. */
export function createHost(extraCss: string): Host {
  const host = document.createElement('div');
  pinHost(host);
  const root = host.attachShadow({ mode: 'closed' });
  addStyles(root, extraCss);
  hosts.add(host);
  document.documentElement.append(host);
  return { host, root };
}

// ── Seeing before clicking ────────────────────────────────

/** Clicks sooner than this after our UI appeared, moved or the page changed around it were not aimed at it. */
export const MIN_SHOW_MS = 500;
/** How long the browser's own visibility check must have seen it whole before a click. */
const VISIBLE_MS = 300;

/** Where the browser can tell whether an element is really seen (Chromium's IntersectionObserver v2). */
const tracksVisibility =
  typeof IntersectionObserverEntry !== 'undefined' &&
  'isVisible' in IntersectionObserverEntry.prototype;

/** In a frame from another origin than the page around it, which could hide or cover it. */
export function inForeignFrame(): boolean {
  try {
    if (window.top === window) return false;
    return window.top!.location.origin !== location.origin;
  } catch {
    return true;
  }
}

/**
 * A filter that makes things faint, blurry, washed out or grey, or does whatever an SVG filter
 * does. A colour filter (a dark mode's invert and hue-rotate) is fine.
 */
export function hidingFilter(filter: string): boolean {
  if (!filter || filter === 'none') return false;
  if (/url\(/.test(filter)) return true;
  for (const [, name, raw] of filter.matchAll(/([a-z-]+)\(([^)]*)\)/g)) {
    const text = (raw ?? '').trim();
    const value = text.endsWith('%') ? parseFloat(text) / 100 : parseFloat(text);
    if (name === 'blur') {
      if (parseFloat(text) > 0.5) return true;
      continue;
    }
    if (Number.isNaN(value)) continue;
    if (name === 'opacity' && value < 0.9) return true;
    if (name === 'brightness' && (value < 0.5 || value > 3)) return true;
    if (name === 'contrast' && value < 0.5) return true;
    if (name === 'invert' && value > 0.3 && value < 0.7) return true;
  }
  return false;
}

/** The host, or the whole page, is made (nearly) invisible, clipped, masked or blended away. */
export function obscured(host: HTMLElement): boolean {
  const doc = host.ownerDocument;
  const view = doc.defaultView;
  if (!view || host.parentNode !== doc.documentElement) return true;
  for (const el of [host, doc.documentElement]) {
    const style = view.getComputedStyle(el);
    if (style.visibility !== 'visible' || style.display === 'none') return true;
    if (style.opacity && Number(style.opacity) < 0.9) return true;
    if (hidingFilter(style.filter)) return true;
    if (style.clipPath && style.clipPath !== 'none') return true;
    const mask =
      style.getPropertyValue('mask-image') || style.getPropertyValue('-webkit-mask-image');
    if (mask && mask !== 'none') return true;
    if (style.mixBlendMode && style.mixBlendMode !== 'normal') return true;
    // In a 3D context, depth decides what is on top, not the z-index.
    if (style.transformStyle === 'preserve-3d') return true;
  }
  const own = view.getComputedStyle(host);
  if (own.position !== 'fixed' || own.zIndex !== '2147483647') return true;
  // `html::after` comes after our host and paints over it at the same z-index.
  const after = view.getComputedStyle(doc.documentElement, '::after');
  if (after.content && after.content !== 'none' && after.display !== 'none') return true;
  return false;
}

function rendered(el: Element): boolean {
  if (el instanceof HTMLStyleElement || el instanceof HTMLScriptElement) return false;
  if (el instanceof HTMLHeadElement || el instanceof HTMLBodyElement) return false;
  return getComputedStyle(el).display !== 'none';
}

/** Whether `root` has something open in the top layer (popovers, modal dialogs, full screen). */
function topLayerIn(root: Document | ShadowRoot): boolean {
  for (const selector of [':popover-open', ':modal', ':fullscreen']) {
    let found: Element[];
    try {
      found = Array.from(root.querySelectorAll(selector));
    } catch {
      continue;
    }
    if (found.some((el) => !hosts.has(el))) return true;
  }
  return false;
}

type ClosedRoots = { openOrClosedShadowRoot?: (el: HTMLElement) => ShadowRoot | null };

/**
 * `el`'s shadow root, a closed one too: extensions may see those (Firefox's
 * `Element.openOrClosedShadowRoot`, Chromium's `chrome.dom.openOrClosedShadowRoot`).
 */
function anyShadowRoot(el: Element): ShadowRoot | null {
  const firefox = (el as Element & { openOrClosedShadowRoot?: ShadowRoot | null })
    .openOrClosedShadowRoot;
  if (firefox !== undefined) return firefox;
  const dom = (globalThis as { chrome?: { dom?: ClosedRoots } }).chrome?.dom;
  if (dom?.openOrClosedShadowRoot && el instanceof HTMLElement) {
    try {
      return dom.openOrClosedShadowRoot(el);
    } catch {
      // Not an element that can have one.
    }
  }
  return el.shadowRoot;
}

/** More than this many elements to look through: refused rather than half checked. */
const MAX_SHADOW_WALK = 100_000;

/**
 * Whether a shadow root of the page, a closed one too, has something open in the top layer
 * (CL-L13): a popover in there lies over any z-index, and `document`'s selectors don't reach it.
 */
function topLayerInShadows(): boolean {
  const roots: (Document | ShadowRoot)[] = [document];
  let walked = 0;
  while (roots.length) {
    const root = roots.pop()!;
    if (root !== document && topLayerIn(root)) return true;
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (++walked > MAX_SHADOW_WALK) return true;
      const el = node as Element;
      if (hosts.has(el)) continue;
      const shadow = anyShadowRoot(el);
      if (shadow) roots.push(shadow);
    }
  }
  return false;
}

/** Something that is drawn above any z-index: the top layer, or what comes after our host. */
function coveredFromAbove(host: HTMLElement): boolean {
  // The top layer (popovers, modal dialogs, full screen) is above every z-index, and a layer
  // with `pointer-events: none` over us isn't found by hit testing.
  if (topLayerIn(document)) return true;
  // Full screen shows the page's element alone (retargeted to its shadow host, if any).
  const full = document.fullscreenElement;
  if (full && !hosts.has(full)) return true;
  // Without the browser's visibility tracking, the page's shadow roots are searched too; with
  // it, the browser sees what lies over us wherever it comes from. (A modal dialog anywhere
  // makes the rest of the page, us too, inert, and the hit tests fail.)
  if (!tracksVisibility && topLayerInShadows()) return true;
  for (let el = host.nextElementSibling; el; el = el.nextElementSibling) {
    if (!hosts.has(el) && rendered(el)) return true;
  }
  return false;
}

/** Whether a click at (x, y) lands on `el` of our host. */
function hits(host: Host, el: Element, x: number, y: number): boolean {
  if (document.elementFromPoint(x, y) !== host.host) return false;
  const inner = host.root.elementFromPoint?.(x, y);
  return inner === undefined || (inner !== null && el.contains(inner));
}

/**
 * `el` is where it is drawn, at full size, and nothing of the page is on top of it: the same
 * element answers at its middle and near its corners, and nothing is above our host.
 */
export function uncovered(host: Host, el: HTMLElement): boolean {
  if (obscured(host.host) || coveredFromAbove(host.host)) return false;
  const rect = el.getBoundingClientRect();
  if (rect.width < 4 || rect.height < 4) return false;
  // A transform on the page's root shrinks what we drew; the layout size doesn't know.
  if (rect.width < el.offsetWidth * 0.9 || rect.height < el.offsetHeight * 0.9) return false;
  let tested = 0;
  for (const [fx, fy] of [
    [0.5, 0.5],
    [0.15, 0.2],
    [0.85, 0.2],
    [0.15, 0.8],
    [0.85, 0.8],
  ] as const) {
    const x = rect.left + rect.width * fx;
    const y = rect.top + rect.height * fy;
    if (x < 0 || y < 0 || x >= window.innerWidth || y >= window.innerHeight) continue;
    if (!hits(host, el, x, y)) return false;
    tested += 1;
  }
  return tested > 0;
}

export type Guard = {
  /** The UI was just shown or moved: the clock starts again. */
  shown: () => void;
  /** Checks `el` (and what is inside it) with the browser's visibility tracking, where it has one. */
  watch: (el: Element) => void;
  /**
   * Whether `event` (a click, or Enter/Space) is somebody knowingly using `el`: a real event;
   * a pointer that went down on `el` while it had been shown, unchanged and uncovered for a
   * moment and is still over it; or the keyboard with the focus in `el`.
   */
  accepts: (event: Event, el: HTMLElement) => boolean;
  /** Whether picks can be checked at all here (not in a frame from another origin that could hide it, unless the browser tracks visibility). */
  verifiable: boolean;
  /**
   * The inline menu's frame (an extension page) took a click or a key: was the frame, as this
   * page shows it, in place, unchanged and uncovered long enough — and is it now? The frame
   * sees its own pointer; this is the page's side of it.
   */
  frameSeen: (frame: HTMLElement) => boolean;
  /**
   * Checks `frame` every 100 ms while it is shown: whatever covers it for a moment starts the
   * clock again, so a decoy taken away just before the check doesn't count as not there.
   */
  watchFrame: (frame: HTMLElement) => void;
  dispose: () => void;
};

export function createGuard(ui: Host, now: () => number = () => performance.now()): Guard {
  let shownAt = now();
  let press: { pressed: HTMLElement | null; ok: boolean } | null = null;
  const watched = new Map<Element, number | null>();
  let repinned = 0;

  const visibleEnough = (el: Element, at: number): boolean => {
    if (!tracksVisibility) return true;
    for (const [target, since] of watched) {
      if (target.contains(el)) return since !== null && at - since >= VISIBLE_MS;
    }
    return false;
  };

  const seen = (el: HTMLElement, at: number): boolean =>
    at - shownAt >= MIN_SHOW_MS && visibleEnough(el, at) && uncovered(ui, el);

  let frameTimer: ReturnType<typeof setInterval> | null = null;

  const intersections = tracksVisibility
    ? new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            const visible = (entry as IntersectionObserverEntry & { isVisible?: boolean })
              .isVisible;
            const before = watched.get(entry.target) ?? null;
            watched.set(entry.target, visible ? (before ?? now()) : null);
          }
        },
        { trackVisibility: true, delay: 100, threshold: [1] } as IntersectionObserverInit,
      )
    : null;

  // The page changing the styles around us (or ours) starts the clock again; our host's own
  // styles are put back.
  const pinned = ui.host.style.cssText;
  const mutations = new MutationObserver((records) => {
    const restyled = records.some(
      (record) => record.target === ui.host && record.attributeName === 'style',
    );
    if (restyled && ui.host.style.cssText !== pinned && repinned < 50) {
      repinned += 1;
      pinHost(ui.host);
    }
    lastAgain();
    shownAt = now();
  });
  // Something added after our host would be drawn over it: go last again, unless that takes
  // the focus from our UI (or a page keeps fighting over it; then clicks are refused).
  const lastAgain = () => {
    const html = document.documentElement;
    if (repinned >= 50 || ui.host.parentNode !== html || ui.root.activeElement) return;
    for (let el = ui.host.nextElementSibling; el; el = el.nextElementSibling) {
      if (!hosts.has(el) && rendered(el)) {
        repinned += 1;
        html.append(ui.host);
        return;
      }
    }
  };
  mutations.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['style', 'class', 'hidden', 'popover'],
    childList: true,
  });
  if (document.body) {
    mutations.observe(document.body, {
      attributes: true,
      attributeFilter: ['style', 'class', 'hidden'],
    });
  }
  mutations.observe(ui.host, { attributes: true });
  lastAgain();

  // What the pointer went down on, and whether it had been seen whole long enough then: what
  // the person saw when they pressed is what counts.
  const onPointerDown = (event: Event) => {
    const at = now();
    const pressed =
      event
        .composedPath()
        .find(
          (node): node is HTMLElement =>
            node instanceof HTMLElement &&
            (node.tagName === 'BUTTON' || node.getAttribute('role') === 'option'),
        ) ?? null;
    press = { pressed, ok: event.isTrusted && !!pressed && seen(pressed, at) };
  };
  ui.root.addEventListener('pointerdown', onPointerDown, true);

  return {
    shown: () => {
      lastAgain();
      shownAt = now();
    },
    watch: (el) => {
      if (!intersections || watched.has(el)) return;
      for (const old of watched.keys()) {
        if (!old.isConnected) {
          intersections.unobserve(old);
          watched.delete(old);
        }
      }
      watched.set(el, null);
      intersections.observe(el);
    },
    verifiable: tracksVisibility || !inForeignFrame(),
    frameSeen: (frame) => {
      if (!tracksVisibility && inForeignFrame()) return false;
      return frame.isConnected && seen(frame, now());
    },
    watchFrame: (frame) => {
      if (frameTimer) clearInterval(frameTimer);
      frameTimer = setInterval(() => {
        if (!frame.isConnected) {
          if (frameTimer) clearInterval(frameTimer);
          frameTimer = null;
          return;
        }
        if (!uncovered(ui, frame)) shownAt = now();
      }, 100);
    },
    accepts: (event, el) => {
      if (!event.isTrusted) return false;
      if (!tracksVisibility && inForeignFrame()) return false;
      const at = now();
      if (event instanceof MouseEvent && event.detail > 0) {
        // A pointer: it went down on this very element after it had been seen long enough,
        // and it is still over it.
        const down = press;
        press = null;
        return !!down?.ok && down.pressed === el && hits(ui, el, event.clientX, event.clientY);
      }
      // The keyboard: the focus is in our UI, which the page can't put there.
      const focused = ui.root.activeElement;
      return !!focused && el.contains(focused) && seen(el, at);
    },
    dispose: () => {
      if (frameTimer) clearInterval(frameTimer);
      intersections?.disconnect();
      mutations.disconnect();
      ui.root.removeEventListener('pointerdown', onPointerDown, true);
    },
  };
}

type Children = (Node | string | null | false | undefined)[];

/** A small `createElement`: attributes set with `setAttribute`, text only as text nodes. */
export function h(
  tag: string,
  attributes: Record<string, string | undefined> = {},
  ...children: Children
): HTMLElement {
  const el = document.createElement(tag);
  for (const [name, value] of Object.entries(attributes)) {
    if (value !== undefined) el.setAttribute(name, value);
  }
  for (const child of children) if (child) el.append(child);
  return el;
}

const SVG = 'http://www.w3.org/2000/svg';

function svg(tag: string, attributes: Record<string, string>): SVGElement {
  const el = document.createElementNS(SVG, tag);
  for (const [name, value] of Object.entries(attributes)) el.setAttribute(name, value);
  return el;
}

/** UwULock's mark: a pink rounded square with a padlock. */
export function lockGlyph(size: number): SVGElement {
  const icon = svg('svg', {
    viewBox: '0 0 24 24',
    width: String(size),
    height: String(size),
    'aria-hidden': 'true',
    focusable: 'false',
  });
  icon.append(
    svg('rect', { width: '24', height: '24', rx: '6', fill: 'var(--uwu-pink-solid)' }),
    svg('path', {
      d: 'M8.5 11V8.6a3.5 3.5 0 0 1 7 0V11',
      fill: 'none',
      stroke: 'var(--uwu-on-pink)',
      'stroke-width': '2',
      'stroke-linecap': 'round',
    }),
    svg('rect', {
      x: '6.5',
      y: '10.5',
      width: '11',
      height: '8.5',
      rx: '2',
      fill: 'var(--uwu-on-pink)',
    }),
    svg('circle', { cx: '12', cy: '14.1', r: '1.4', fill: 'var(--uwu-pink-solid)' }),
    svg('rect', {
      x: '11.3',
      y: '14.4',
      width: '1.4',
      height: '2.6',
      rx: '0.7',
      fill: 'var(--uwu-pink-solid)',
    }),
  );
  return icon;
}

/** The close cross of the bar. */
export function crossGlyph(): SVGElement {
  const icon = svg('svg', {
    viewBox: '0 0 16 16',
    width: '14',
    height: '14',
    'aria-hidden': 'true',
  });
  icon.append(
    svg('path', {
      d: 'M4 4l8 8M12 4l-8 8',
      stroke: 'currentColor',
      'stroke-width': '1.8',
      'stroke-linecap': 'round',
    }),
  );
  return icon;
}
