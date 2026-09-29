/**
 * The inline menu: a small UwULock button at the right edge of a focused login, card or
 * address field, and under it the list of the items that fit.
 *
 * The list is not drawn here. It is an extension page (menu.html) in a frame under the field:
 * the names come to it from the background, and a pick in it goes straight back there, so the
 * page — not even its compromised renderer — can neither read the list nor make up a pick
 * (security review 0.3, CL-L8). The background then offers the item's values to this frame
 * with a one-time token, and they go into the field. This script only asks for the menu's
 * session, places the frame, answers the background whether the frame was seen uncovered (the
 * guard of ui.ts), and closes it.
 *
 * ↓ in the field opens the list and selects its first entry, arrows move, Enter picks, Esc
 * closes. The menu goes away when the field loses focus to anything but the menu, or
 * disappears.
 *
 * A page can focus a field by script and trick somebody into clicking the button (clickjacking):
 * every click on it goes through the guard of ui.ts, which wants it seen whole, unchanged and in
 * place for a moment first. In a frame from another origin, where that can't be checked, the
 * menu only leads to UwULock's own window.
 */

import { ext } from '../shared/browser';
import { t } from '../shared/i18n';
import { ask } from '../shared/messages';
import type { BackgroundMessage, MenuKind, PageInfo } from '../shared/protocol';
import { isVisible } from './forms';
import { createGuard, createHost, h, lockGlyph, type Guard, type Host } from './ui';

export type { MenuKind } from '../shared/protocol';

export type InlineMenuDeps = {
  info: () => PageInfo | null;
  /** Writes one value into `field`, as typing would. */
  fillText: (field: HTMLInputElement, value: string) => void;
};

export type InlineMenu = {
  /**
   * A field of `kind` got focus. `maskable`: it is a username or email field, where UwULock
   * Server's masked addresses may be offered.
   */
  attach: (field: HTMLInputElement, kind: MenuKind, maskable?: boolean) => void;
  detach: () => void;
  /** The page's info changed. */
  refresh: () => void;
  /** The page changed: is the field still there? */
  check: () => void;
  /** The field the menu is attached to. */
  field: () => HTMLInputElement | null;
  /** The open list's session with the background, if any. */
  session: () => string | null;
  /**
   * A message of the background about the list (`bg:menu-*`, `bg:fill-text`): handled, and
   * what to answer; `undefined` when it isn't about this menu.
   */
  onMessage: (message: BackgroundMessage) => { answer: unknown } | undefined;
};

const CSS = `
.button {
  position: fixed;
  display: grid;
  place-items: center;
  padding: 0;
  border: 0;
  border-radius: 7px;
  background: transparent;
  line-height: 0;
}
.button:hover svg { filter: brightness(1.08); }
.frame {
  position: fixed;
  display: block;
  margin: 0;
  padding: 0;
  border: 0;
  border-radius: 16px;
  overflow: hidden;
  background: var(--uwu-surface);
  box-shadow: var(--uwu-shadow);
  color-scheme: normal;
}
.menu {
  position: fixed;
  display: grid;
  gap: 10px;
  padding: 12px;
}
`;

const BUTTON_MAX = 24;
/** The frame's height until it says how tall its list is. */
const FIRST_HEIGHT = 52;
const MAX_HEIGHT = 320;

export function createInlineMenu(deps: InlineMenuDeps): InlineMenu {
  let field: HTMLInputElement | null = null;
  let kind: MenuKind = 'login';
  let maskable = false;
  let ui: Host | null = null;
  let guard: Guard | null = null;
  let button: HTMLElement | null = null;
  /** The list: the extension's frame, or where that can't be checked, a note in the page. */
  let menu: HTMLElement | null = null;
  let frameEl: HTMLIFrameElement | null = null;
  let session: string | null = null;
  let height = FIRST_HEIGHT;
  /** Where the button and the list were drawn last: moving them starts the guard's clock again. */
  let placed = '';
  let frame = 0;

  const openPopup = () => {
    void ask<unknown>({ type: 'content:open-popup' }).catch(() => undefined);
    closeList(false);
  };

  // ── Placing ─────────────────────────────────────────────

  const place = () => {
    frame = 0;
    if (!field || !ui) return;
    if (!field.isConnected || !isVisible(field)) {
      detach();
      return;
    }
    const rect = field.getBoundingClientRect();
    if (button) {
      const size = Math.max(16, Math.min(BUTTON_MAX, rect.height - 8));
      button.style.width = `${size}px`;
      button.style.height = `${size}px`;
      button.style.left = `${rect.right - size - 6}px`;
      button.style.top = `${rect.top + (rect.height - size) / 2}px`;
    }
    if (menu) {
      const viewportWidth = document.documentElement.clientWidth || window.innerWidth;
      const width = Math.min(Math.max(rect.width, 260), 380, Math.max(viewportWidth - 16, 200));
      const left = Math.max(8, Math.min(rect.left, viewportWidth - width - 8));
      menu.style.width = `${width}px`;
      menu.style.left = `${left}px`;
      if (frameEl) frameEl.style.height = `${height}px`;
      const tall = menu.offsetHeight;
      const below = window.innerHeight - rect.bottom;
      menu.style.top =
        tall && below < tall + 8 && rect.top > below
          ? `${rect.top - tall - 4}px`
          : `${rect.bottom + 4}px`;
    }
    const where = `${button?.style.cssText}|${menu?.style.cssText}`;
    if (where !== placed) {
      placed = where;
      guard?.shown();
    }
  };

  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(place);
  };

  // ── The list ────────────────────────────────────────────

  /** Where picks can't be checked (a frame of another origin on Firefox): only UwULock's window. */
  const openNote = () => {
    if (!ui) return;
    const openButton = h('button', { class: 'primary', type: 'button' }, t('UwULock öffnen'));
    openButton.addEventListener('click', (event) => {
      if (guard?.accepts(event, openButton)) openPopup();
    });
    menu = h(
      'div',
      { class: 'menu panel', role: 'dialog', 'aria-label': t('UwULock – passende Einträge') },
      h(
        'div',
        { class: 'muted' },
        t('In diesem eingebetteten Bereich füllst du über das UwULock-Fenster aus.'),
      ),
      h('div', { class: 'actions' }, openButton),
    );
    menu.addEventListener('mousedown', (event) => event.preventDefault());
    ui.root.append(menu);
    guard?.watch(menu);
  };

  const openFrame = async (focusFirst: boolean) => {
    if (!ui) return;
    const own = ui;
    const iframe = document.createElement('iframe');
    iframe.className = 'frame';
    iframe.setAttribute('title', t('UwULock – passende Einträge'));
    iframe.setAttribute('scrolling', 'no');
    // Nothing of the page's may go along: no referrer, no permissions.
    iframe.setAttribute('referrerpolicy', 'no-referrer');
    iframe.setAttribute('allow', '');
    height = FIRST_HEIGHT;
    menu = iframe;
    frameEl = iframe;
    own.root.append(iframe);
    guard?.watch(iframe);
    guard?.watchFrame(iframe);
    place();
    let opened: { session: string };
    try {
      opened = await ask<{ session: string }>({
        type: 'content:menu-open',
        kind,
        maskable,
      });
    } catch {
      if (frameEl === iframe) closeList(false);
      return;
    }
    if (frameEl !== iframe) {
      void ask<unknown>({ type: 'content:menu-close', session: opened.session }).catch(
        () => undefined,
      );
      return;
    }
    session = opened.session;
    iframe.src = `${ext.runtime.getURL('menu.html')}#${opened.session}`;
    if (focusFirst) focusList();
  };

  /** The field's ↓: into the list, its first entry selected. */
  const focusList = () => {
    if (!frameEl || !session) return;
    frameEl.focus();
    void ask<unknown>({ type: 'content:menu-focus', session }).catch(() => undefined);
  };

  const openList = (focusFirst: boolean) => {
    if (!ui || menu) return;
    button?.setAttribute('aria-expanded', 'true');
    if (guard && !guard.verifiable) openNote();
    else void openFrame(focusFirst);
    guard?.shown();
    place();
  };

  function closeList(refocus: boolean) {
    if (session) {
      void ask<unknown>({ type: 'content:menu-close', session }).catch(() => undefined);
    }
    session = null;
    menu?.remove();
    menu = null;
    frameEl = null;
    button?.setAttribute('aria-expanded', 'false');
    if (refocus) field?.focus({ preventScroll: true });
  }

  // ── Following the field ─────────────────────────────────

  const onFieldKey = (event: KeyboardEvent) => {
    if (!event.isTrusted) return;
    if (event.key === 'ArrowDown' && !event.altKey && !event.ctrlKey && !event.metaKey) {
      event.preventDefault();
      if (menu) focusList();
      else openList(true);
    } else if (event.key === 'Escape' && menu) {
      closeList(false);
    }
  };

  const onFieldBlur = (event: FocusEvent) => {
    if (ui && event.relatedTarget === ui.host) return;
    detach();
  };

  const onRootBlur = (event: Event) => {
    const next = (event as FocusEvent).relatedTarget as Node | null;
    if (next && (next === field || ui?.root.contains(next))) return;
    // Into our own frame (its document isn't a node of ours): still the menu.
    if (frameEl && ui?.root.activeElement === frameEl) return;
    detach();
  };

  function detach() {
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    if (field) {
      field.removeEventListener('keydown', onFieldKey, true);
      field.removeEventListener('focusout', onFieldBlur);
    }
    window.removeEventListener('scroll', schedule, true);
    window.removeEventListener('resize', schedule);
    if (menu) closeList(false);
    guard?.dispose();
    guard = null;
    placed = '';
    ui?.host.remove();
    ui = null;
    button = null;
    field = null;
  }

  const attach = (next: HTMLInputElement, nextKind: MenuKind, nextMaskable = false) => {
    if (field === next && kind === nextKind && maskable === nextMaskable && ui) return;
    detach();
    field = next;
    kind = nextKind;
    maskable = nextMaskable;
    ui = createHost(CSS);
    ui.root.addEventListener('focusout', onRootBlur);
    const own = createGuard(ui);
    guard = own;
    const openButton = h('button', {
      class: 'button',
      type: 'button',
      title: 'UwULock',
      'aria-label': t('UwULock-Menü öffnen'),
      'aria-haspopup': 'listbox',
      'aria-expanded': 'false',
    });
    button = openButton;
    openButton.append(lockGlyph(BUTTON_MAX));
    openButton.addEventListener('mousedown', (event) => event.preventDefault());
    openButton.addEventListener('click', (event) => {
      if (!own.accepts(event, openButton)) return;
      if (menu) closeList(false);
      else openList(event.detail === 0);
    });
    ui.root.append(openButton);
    own.watch(openButton);
    next.addEventListener('keydown', onFieldKey, true);
    next.addEventListener('focusout', onFieldBlur);
    window.addEventListener('scroll', schedule, { capture: true, passive: true });
    window.addEventListener('resize', schedule, { passive: true });
    place();
  };

  const onMessage = (message: BackgroundMessage): { answer: unknown } | undefined => {
    switch (message.type) {
      case 'bg:menu-guard':
        return {
          answer:
            !!session &&
            message.session === session &&
            !!frameEl &&
            !!guard &&
            guard.frameSeen(frameEl),
        };
      case 'bg:menu-size':
        if (message.session !== session || !frameEl) return undefined;
        height = Math.max(24, Math.min(MAX_HEIGHT, message.height));
        place();
        return { answer: null };
      case 'bg:menu-close':
        if (message.session !== session) return undefined;
        closeList(message.refocus);
        return { answer: null };
      case 'bg:fill-text': {
        if (message.session !== session || !field) return undefined;
        const target = field;
        detach();
        deps.fillText(target, message.value);
        return { answer: null };
      }
      default:
        return undefined;
    }
  };

  return {
    attach,
    detach,
    refresh: () => {
      // The frame hears from the background itself; a note stays a note.
    },
    check: () => {
      if (field) schedule();
    },
    field: () => field,
    session: () => session,
    onMessage,
  };
}
