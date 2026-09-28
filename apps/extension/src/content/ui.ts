/**
 * What the content script shows in a page (the button and menu in fields, the notification
 * bar) lives in a closed shadow root on its own host element under `<html>`: the page can't
 * look inside, and its styles don't reach in. The host's own styles are set inline with
 * `!important`, which beats anything a page's style sheets say about it.
 *
 * The page can still see the host and cover or hide the whole document; `obscured()` refuses
 * clicks while the host or the root element is made see-through.
 */

const CSS = `
:host {
  --uwu-surface: #ffffff;
  --uwu-elevated: #fcf8fa;
  --uwu-ink: #1c1420;
  --uwu-muted: #716672;
  --uwu-hairline: #f2e8ee;
  --uwu-border: #e9dde4;
  --uwu-pink: #ff4d8d;
  --uwu-pink-solid: #e11d74;
  --uwu-on-pink: #ffffff;
  --uwu-pink-ink: #a3154f;
  --uwu-pink-tint: #ffe4ef;
  --uwu-alarm: #8e5510;
  --uwu-shadow: 0 12px 32px rgba(28, 20, 32, 0.18), 0 2px 6px rgba(28, 20, 32, 0.08);
  --uwu-font: 'Manrope Variable', 'Manrope', 'Segoe UI', system-ui, -apple-system, sans-serif;
  color-scheme: light;
}
@media (prefers-color-scheme: dark) {
  :host {
    --uwu-surface: #1c171f;
    --uwu-elevated: #241e28;
    --uwu-ink: #f8f2f6;
    --uwu-muted: #b3a8b3;
    --uwu-hairline: #2c2430;
    --uwu-border: #3a3040;
    --uwu-pink: #ff7fac;
    --uwu-pink-solid: #ff7fac;
    --uwu-on-pink: #1c1420;
    --uwu-pink-ink: #ffa3c4;
    --uwu-pink-tint: #3a1a2a;
    --uwu-alarm: #d8a25c;
    --uwu-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
    color-scheme: dark;
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
  border-radius: 10px;
  padding: 7px 14px;
  font-weight: 700;
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
.alarm { color: var(--uwu-alarm); }
.actions { display: flex; flex-wrap: wrap; gap: 8px; }
`;

let sheet: CSSStyleSheet | null | undefined;

/** Our styles, as a constructed sheet (no page CSP applies) or else a `<style>` element. */
function addStyles(root: ShadowRoot, extra: string) {
  if (sheet === undefined) {
    try {
      sheet = new CSSStyleSheet();
      sheet.replaceSync(CSS);
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
  style.textContent = CSS + extra;
  root.append(style);
}

export type Host = { host: HTMLElement; root: ShadowRoot };

/** A host under `<html>`, on top of everything, with a closed shadow root and our styles. */
export function createHost(extraCss: string): Host {
  const host = document.createElement('div');
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
  const root = host.attachShadow({ mode: 'closed' });
  addStyles(root, extraCss);
  document.documentElement.append(host);
  return { host, root };
}

/** The host, or the whole page, is made (nearly) invisible: a click on it wasn't meant. */
export function obscured(host: HTMLElement): boolean {
  const view = host.ownerDocument.defaultView;
  if (!view) return true;
  for (const el of [host, host.ownerDocument.documentElement]) {
    const style = view.getComputedStyle(el);
    if (style.visibility === 'hidden' || style.display === 'none') return true;
    if (style.opacity && Number(style.opacity) < 0.9) return true;
    if (style.filter && style.filter !== 'none' && /opacity|blur/.test(style.filter)) return true;
  }
  return false;
}

/** A trusted event, and the page isn't hiding our UI. */
export function genuine(event: Event, host: HTMLElement): boolean {
  return event.isTrusted && !obscured(host);
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
