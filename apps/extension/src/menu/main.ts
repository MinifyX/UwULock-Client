/**
 * The inline menu's list: an extension page (menu.html#<session>) that the content script shows
 * in a frame under a field (security review 0.3, CL-L8). It gets the names to list from the
 * background over its port and sends the pick back the same way; the page around it can't read
 * the one or forge the other — only the extension's own page speaks on this port.
 *
 * The page can still lay things over the frame or make it see-through. So a click or a key
 * counts only when the list had been shown, unchanged, for half a second; on Chromium the
 * browser must also have seen it whole (IntersectionObserver v2, which covers what the page
 * around it does); a pointer must have gone down on the very entry; and Enter only picks an
 * entry that a key in here selected. The background then asks the content script whether the
 * frame, as the page shows it, was uncovered too (ui.ts), before it offers anything.
 */

import './fonts.css';
import { BASE_CSS, h, lockGlyph, MIN_SHOW_MS } from '../content/ui';
import { ext } from '../shared/browser';
import { setLanguage, t } from '../shared/i18n';
import type {
  MenuMessage,
  MenuPickAnswer,
  MenuRequest,
  MenuView,
  PageItem,
} from '../shared/protocol';
import { uwuErrorText } from '../shared/uwu-errors';

const CSS = `
html, body {
  margin: 0;
  padding: 0;
  overflow: hidden;
  background: var(--uwu-surface);
}
.menu {
  box-sizing: border-box;
  max-height: 320px;
  overflow: auto;
  padding: 6px;
  border-radius: 16px;
  box-shadow: none;
}
[role='listbox'] { display: grid; gap: 2px; outline: none; }
.option {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 10px;
  cursor: pointer;
  min-width: 0;
}
.option:hover, .option[aria-selected='true'] { background: var(--uwu-pink-tint); }
.option:focus-visible { outline: 2px solid var(--uwu-pink); outline-offset: -2px; }
.tile {
  flex: none;
  width: 28px;
  height: 28px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  background: var(--uwu-pink-tint);
  color: var(--uwu-pink-ink);
  font-weight: 700;
  font-size: 13px;
}
.text { display: grid; min-width: 0; }
.name, .sub { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.name { font-weight: 600; }
.sub { font-size: 12px; color: var(--uwu-muted); }
.note { padding: 8px 10px 4px; }
.message { display: grid; gap: 10px; padding: 8px 10px; }
`;

/** How long the browser's own visibility check must have seen the list whole. */
const VISIBLE_MS = 300;

type Entry = { label: string; sub?: string | null; letter?: string; run: () => void };

// ── Styles, and the list's panel ──────────────────────────

function addStyles() {
  try {
    const sheet = new CSSStyleSheet();
    // The design's tokens are written for a shadow host; here they belong to the document.
    sheet.replaceSync(BASE_CSS.replaceAll(':host', ':root') + CSS);
    document.adoptedStyleSheets = [sheet];
  } catch {
    // Without constructed style sheets (tests): unstyled, still working.
  }
}

addStyles();
const panel = h('div', { class: 'menu panel' });
panel.addEventListener('mousedown', (event) => event.preventDefault());
document.body.append(panel);

// ── Seeing before using ───────────────────────────────────

const now = () => performance.now();
let shownAt = now();
let visibleSince: number | null = null;
const tracksVisibility =
  typeof IntersectionObserverEntry !== 'undefined' &&
  'isVisible' in IntersectionObserverEntry.prototype;

if (tracksVisibility) {
  new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        const visible = (entry as IntersectionObserverEntry & { isVisible?: boolean }).isVisible;
        visibleSince = visible ? (visibleSince ?? now()) : null;
      }
    },
    { trackVisibility: true, delay: 100, threshold: [1] } as IntersectionObserverInit,
  ).observe(panel);
}

/** The list had been shown, unchanged, long enough — and on Chromium, seen whole. */
export function seen(at: number): boolean {
  if (document.visibilityState !== 'visible') return false;
  if (at - shownAt < MIN_SHOW_MS) return false;
  return !tracksVisibility || (visibleSince !== null && at - visibleSince >= VISIBLE_MS);
}

const changedNow = () => {
  shownAt = now();
};
// Other entries under the pointer than a moment ago: they have to be seen first, too. (Which
// entry is selected is no change of what is shown.)
new MutationObserver(changedNow).observe(document.documentElement, {
  childList: true,
  subtree: true,
  characterData: true,
});
window.addEventListener('resize', changedNow);

let press: { el: HTMLElement | null; ok: boolean } | null = null;
document.addEventListener(
  'pointerdown',
  (event) => {
    const el =
      (event.target as Element | null)?.closest<HTMLElement>('[role="option"], button') ?? null;
    press = { el, ok: event.isTrusted && !!el && seen(now()) };
  },
  true,
);

/** A pointer that went down on `el` after it had been seen, and is still over it. */
function pointerAccepts(event: MouseEvent, el: HTMLElement): boolean {
  const down = press;
  press = null;
  if (!event.isTrusted || !down?.ok || down.el !== el) return false;
  const under = document.elementFromPoint(event.clientX, event.clientY);
  return !!under && el.contains(under);
}

// ── Talking to the background ─────────────────────────────

const session = location.hash.slice(1);
let port: chrome.runtime.Port | null = null;
let view: MenuView | null = null;
let busy = false;

function send(message: MenuRequest) {
  try {
    port?.postMessage(message);
  } catch {
    // Disconnected: the background was restarted; `gone` follows.
  }
}

function connect(hello: boolean) {
  const own = ext.runtime.connect({ name: 'menu' });
  port = own;
  own.onMessage.addListener((message: MenuMessage) => onMessage(message));
  own.onDisconnect.addListener(() => {
    if (port === own) port = null;
    renderGone();
  });
  if (hello) send({ type: 'hello', session });
}

/** UwULock's window; a port that lost its session (the background restarted) still opens it. */
const openPopup = () => {
  if (!port) connect(false);
  send({ type: 'open-popup' });
};

// ── The list ──────────────────────────────────────────────

/** An entry was selected by a key in here (or the field's ↓): Enter may pick it. */
let armed = false;
/** What each entry does, for Enter. */
const runs = new WeakMap<HTMLElement, () => void>();

const options = (): HTMLElement[] => Array.from(panel.querySelectorAll('[role="option"]'));

function select(option: HTMLElement | undefined) {
  if (!option) return;
  for (const other of options()) other.setAttribute('aria-selected', String(other === option));
  option.focus();
  armed = true;
}

function entry(item: Entry, index: number): HTMLElement {
  const option = h(
    'div',
    {
      class: 'option',
      role: 'option',
      tabindex: '-1',
      id: `uwulock-option-${index}`,
      'aria-selected': 'false',
    },
    item.letter ? h('span', { class: 'tile', 'aria-hidden': 'true' }, item.letter) : lockGlyph(28),
    h(
      'span',
      { class: 'text' },
      h('span', { class: 'name' }, item.label),
      item.sub ? h('span', { class: 'sub' }, item.sub) : null,
    ),
  );
  option.addEventListener('click', (event) => {
    if (event.detail > 0 && pointerAccepts(event, option)) item.run();
  });
  runs.set(option, item.run);
  return option;
}

function entries(): { note: string | null; list: Entry[] } {
  const state = view?.state ?? 'locked';
  if (state === 'logged-out') {
    return { note: null, list: [{ label: t('Bei UwULock anmelden'), run: openPopup }] };
  }
  if (state === 'locked') {
    return { note: null, list: [{ label: t('UwULock entsperren'), run: openPopup }] };
  }
  const masked: Entry[] = view?.masked
    ? [
        {
          label: t('Neue maskierte Adresse'),
          sub: t('Von UwUMail, nur für diese Seite'),
          letter: '@',
          run: createMasked,
        },
      ]
    : [];
  if (view?.kind === 'signup') return { note: null, list: masked };
  const found = view?.items ?? [];
  if (!found.length) {
    return {
      note: t('Keine passenden Einträge'),
      list: [{ label: t('UwULock öffnen'), run: openPopup }, ...masked],
    };
  }
  return {
    note: null,
    list: [
      ...found.map((item: PageItem) => ({
        label: item.name,
        sub: item.subtitle,
        letter: (item.name.trim()[0] ?? '?').toUpperCase(),
        run: () => pick(item, false),
      })),
      ...masked,
    ],
  };
}

function render() {
  const { note, list } = entries();
  const listbox = h(
    'div',
    { role: 'listbox', tabindex: '-1', 'aria-label': t('UwULock – passende Einträge') },
    ...list.map(entry),
  );
  panel.replaceChildren(note ? h('div', { class: 'note muted' }, note) : '', listbox);
  armed = false;
}

function renderGone() {
  view = null;
  panel.replaceChildren(
    h(
      'div',
      { role: 'listbox', tabindex: '-1', 'aria-label': t('UwULock – passende Einträge') },
      entry({ label: t('UwULock öffnen'), run: openPopup }, 0),
    ),
  );
}

function message(text: string, actions: { label: string; primary?: boolean; run: () => void }[]) {
  const buttons = actions.map((action) => {
    const el = h(
      'button',
      { class: action.primary ? 'primary' : 'secondary', type: 'button' },
      action.label,
    );
    el.addEventListener('click', (event) => {
      if (event.detail > 0 ? pointerAccepts(event, el) : keyAccepts(el)) action.run();
    });
    return el;
  });
  panel.replaceChildren(
    h(
      'div',
      { class: 'message', role: 'alert' },
      h('div', {}, text),
      h('div', { class: 'actions' }, ...buttons),
    ),
  );
  if (document.hasFocus()) buttons[0]?.focus();
}

let picked: PageItem | null = null;

function pick(item: PageItem, confirmedInsecure: boolean) {
  if (busy) return;
  busy = true;
  picked = item;
  send({ type: 'pick', itemId: item.id, ...(confirmedInsecure ? { confirmedInsecure } : {}) });
}

function onPicked(answer: MenuPickAnswer) {
  busy = false;
  const item = picked;
  if (answer.filled) return; // The content script fills and closes the menu.
  switch (answer.reason) {
    case 'unseen':
      // The page's side didn't see the frame uncovered: the click doesn't count, the list stays.
      return;
    case 'insecure':
      message(t('Diese Seite ist nicht verschlüsselt (http). Trotzdem ausfüllen?'), [
        {
          label: t('Trotzdem ausfüllen'),
          primary: true,
          run: () => {
            if (item) pick(item, true);
          },
        },
        { label: t('Abbrechen'), run: render },
      ]);
      return;
    case 'reprompt':
      message(t('Dieser Eintrag verlangt dein Master-Passwort – öffne UwULock.'), [
        { label: t('UwULock öffnen'), primary: true, run: openPopup },
      ]);
      return;
    case 'locked':
      message(t('UwULock ist gesperrt.'), [
        { label: t('UwULock entsperren'), primary: true, run: openPopup },
      ]);
      return;
    default:
      message(t('Ausfüllen hat nicht geklappt.'), []);
  }
}

function createMasked() {
  if (busy) return;
  busy = true;
  message(t('Maskierte Adresse wird angelegt …'), []);
  send({ type: 'masked' });
}

function onMasked(failure: { kind: string; message: string } | null) {
  busy = false;
  if (!failure) return; // The content script types it in and closes the menu.
  const text =
    uwuErrorText(failure.kind, failure.message) ??
    (failure.kind === 'locked'
      ? t('UwULock ist gesperrt.')
      : t('Die maskierte Adresse ließ sich nicht anlegen.'));
  const connectAgain = failure.kind === 'uwu:not_connected' || failure.kind === 'uwu:revoked';
  message(
    text,
    connectAgain
      ? [
          {
            label: t('Web-Tresor öffnen'),
            primary: true,
            run: () => send({ type: 'open-masked-settings' }),
          },
        ]
      : [],
  );
}

function onMessage(msg: MenuMessage) {
  switch (msg.type) {
    case 'view':
      view = msg.view;
      setLanguage(msg.view.language);
      // A pick or a message on screen stays until it is answered.
      if (!busy && !panel.querySelector('.message')) render();
      return;
    case 'gone':
      renderGone();
      return;
    case 'picked':
      onPicked(msg.answer);
      return;
    case 'masked':
      onMasked(msg.failure);
      return;
    case 'select-first':
      select(options()[0]);
      return;
  }
}

// ── Keys ──────────────────────────────────────────────────

/** Enter or Space on `el`: selected by a key in here, and seen long enough. */
function keyAccepts(el: HTMLElement): boolean {
  return document.activeElement === el && seen(now());
}

document.addEventListener('keydown', (event) => {
  if (!event.isTrusted) return;
  const all = options();
  const index = all.indexOf(document.activeElement as HTMLElement);
  switch (event.key) {
    case 'ArrowDown':
      select(all[(index + 1) % all.length]);
      break;
    case 'ArrowUp':
      select(all[(index - 1 + all.length) % all.length]);
      break;
    case 'Home':
      select(all[0]);
      break;
    case 'End':
      select(all[all.length - 1]);
      break;
    case 'Escape':
      send({ type: 'close', refocus: true });
      break;
    case 'Tab':
      send({ type: 'close', refocus: true });
      break;
    case 'Enter':
    case ' ': {
      const option = document.activeElement as HTMLElement | null;
      if (!option || option.getAttribute('role') !== 'option') return;
      if (armed && keyAccepts(option)) runs.get(option)?.();
      break;
    }
    default:
      return;
  }
  event.preventDefault();
  event.stopPropagation();
});

// The frame got the focus from somewhere (the page can focus it too): the list takes the keys,
// but nothing is selected until a key in here or the field's ↓ selects it.
window.addEventListener('focus', () => {
  if (!panel.contains(document.activeElement)) {
    panel.querySelector<HTMLElement>('[role="listbox"]')?.focus();
  }
});

// ── Size ──────────────────────────────────────────────────

let lastHeight = 0;
const reportSize = () => {
  const height = Math.ceil(panel.getBoundingClientRect().height);
  if (height === lastHeight) return;
  lastHeight = height;
  send({ type: 'size', height });
};
if (typeof ResizeObserver !== 'undefined') new ResizeObserver(reportSize).observe(panel);

if (session) connect(true);
else renderGone();
