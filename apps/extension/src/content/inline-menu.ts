/**
 * The inline menu: a small UwULock button at the right edge of a focused login, card or
 * address field, and under it a list of the items that fit. Picking one (a real click, or
 * Enter in the list) asks the background for its values, which go straight into the fields.
 *
 * ↓ in the field opens the list, arrows move, Enter picks, Esc closes. The menu goes away when
 * the field loses focus to anything but the menu, or disappears.
 */

import { t } from '../shared/i18n';
import { ask, RequestFailed } from '../shared/messages';
import type { FillAnswer, FillValues, PageInfo, PageItem } from '../shared/protocol';
import { uwuErrorText } from '../shared/uwu-errors';
import { isVisible } from './forms';
import { createHost, genuine, h, lockGlyph, type Host } from './ui';

/** `signup`: a sign-up form's username or email field, where only a masked address is offered. */
export type MenuKind = 'login' | 'card' | 'identity' | 'signup';

export type InlineMenuDeps = {
  info: () => PageInfo | null;
  /** Writes an item's values into the form of `field`. */
  fill: (values: FillValues, itemId: string, field: HTMLElement) => void;
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
  /** The page's info changed: redraw an open list. */
  refresh: () => void;
  /** The page changed: is the field still there? */
  check: () => void;
  /** The field the menu is attached to. */
  field: () => HTMLInputElement | null;
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
.menu {
  position: fixed;
  max-height: 300px;
  overflow: auto;
  padding: 6px;
}
[role='listbox'] { display: grid; gap: 2px; }
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
  font-weight: 800;
  font-size: 13px;
}
.text { display: grid; min-width: 0; }
.name, .sub { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.name { font-weight: 700; }
.sub { font-size: 12px; color: var(--uwu-muted); }
.note { padding: 8px 10px 4px; }
.message { display: grid; gap: 10px; padding: 8px 10px; }
`;

/** Clicks sooner than this after the list appeared were not aimed at it. */
const MIN_SHOW_MS = 300;
const BUTTON_MAX = 24;

type Entry = { label: string; sub?: string | null; letter?: string; run: () => void };

export function createInlineMenu(deps: InlineMenuDeps): InlineMenu {
  let field: HTMLInputElement | null = null;
  let kind: MenuKind = 'login';
  let maskable = false;
  let ui: Host | null = null;
  let button: HTMLElement | null = null;
  let menu: HTMLElement | null = null;
  let shownAt = 0;
  let frame = 0;
  let busy = false;

  const items = (): PageItem[] => {
    const info = deps.info();
    if (!info) return [];
    if (kind === 'signup') return [];
    return kind === 'login' ? info.logins : kind === 'card' ? info.cards : info.identities;
  };

  /** "New masked address", where the field and the server allow it. */
  const maskedEntries = (): Entry[] => {
    const info = deps.info();
    if (!maskable || !info?.uwuFeatures.includes('masked-addresses')) return [];
    return [
      {
        label: t('Neue maskierte Adresse'),
        sub: t('Von UwUMail, nur für diese Seite'),
        letter: '@',
        run: () => void createMasked(),
      },
    ];
  };

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
      const height = menu.offsetHeight;
      const below = window.innerHeight - rect.bottom;
      menu.style.top =
        height && below < height + 8 && rect.top > below
          ? `${rect.top - height - 4}px`
          : `${rect.bottom + 4}px`;
    }
  };

  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(place);
  };

  // ── The list ────────────────────────────────────────────

  const options = (): HTMLElement[] =>
    menu ? Array.from(menu.querySelectorAll<HTMLElement>('[role="option"]')) : [];

  const select = (option: HTMLElement | undefined) => {
    if (!option) return;
    for (const other of options()) other.setAttribute('aria-selected', String(other === option));
    option.focus();
  };

  const entry = (item: Entry, index: number): HTMLElement => {
    const option = h(
      'div',
      {
        class: 'option',
        role: 'option',
        tabindex: '-1',
        id: `uwulock-option-${index}`,
        'aria-selected': 'false',
      },
      item.letter
        ? h('span', { class: 'tile', 'aria-hidden': 'true' }, item.letter)
        : lockGlyph(28),
      h(
        'span',
        { class: 'text' },
        h('span', { class: 'name' }, item.label),
        item.sub ? h('span', { class: 'sub' }, item.sub) : null,
      ),
    );
    option.addEventListener('click', (event) => {
      if (!ui || !genuine(event, ui.host)) return;
      if (performance.now() - shownAt < MIN_SHOW_MS) return;
      item.run();
    });
    option.addEventListener('keydown', (event) => {
      if (!event.isTrusted || (event.key !== 'Enter' && event.key !== ' ')) return;
      event.preventDefault();
      item.run();
    });
    return option;
  };

  const entries = (): { note: string | null; list: Entry[] } => {
    const state = deps.info()?.state ?? 'locked';
    if (state === 'logged-out') {
      return { note: null, list: [{ label: t('Bei UwULock anmelden'), run: openPopup }] };
    }
    if (state === 'locked') {
      return { note: null, list: [{ label: t('UwULock entsperren'), run: openPopup }] };
    }
    const found = items();
    const masked = maskedEntries();
    if (kind === 'signup') return { note: null, list: masked };
    if (!found.length) {
      return {
        note: t('Keine passenden Einträge'),
        list: [{ label: t('UwULock öffnen'), run: openPopup }, ...masked],
      };
    }
    return {
      note: null,
      list: [
        ...found.map((item) => ({
          label: item.name,
          sub: item.subtitle,
          letter: (item.name.trim()[0] ?? '?').toUpperCase(),
          run: () => void pick(item, false),
        })),
        ...masked,
      ],
    };
  };

  const render = (focusFirst: boolean) => {
    if (!menu) return;
    const { note, list } = entries();
    const listbox = h(
      'div',
      { role: 'listbox', 'aria-label': t('UwULock – passende Einträge') },
      ...list.map(entry),
    );
    listbox.addEventListener('keydown', onListKey);
    menu.replaceChildren(note ? h('div', { class: 'note muted' }, note) : '', listbox);
    if (focusFirst) select(options()[0]);
  };

  const message = (
    text: string,
    actions: { label: string; primary?: boolean; run: () => void }[],
  ) => {
    if (!menu) return;
    const buttons = actions.map((action) => {
      const el = h(
        'button',
        { class: action.primary ? 'primary' : 'secondary', type: 'button' },
        action.label,
      );
      el.addEventListener('click', (event) => {
        if (ui && genuine(event, ui.host)) action.run();
      });
      return el;
    });
    const hadFocus = !!ui && ui.root.activeElement !== null;
    menu.replaceChildren(
      h(
        'div',
        { class: 'message', role: 'alert' },
        h('div', {}, text),
        h('div', { class: 'actions' }, ...buttons),
      ),
    );
    shownAt = performance.now();
    if (hadFocus) buttons[0]?.focus();
    schedule();
  };

  const pick = async (item: PageItem, confirmedInsecure: boolean) => {
    if (busy || !field) return;
    busy = true;
    const target = field;
    let answer: FillAnswer;
    try {
      answer = await ask<FillAnswer>({
        type: 'content:fill',
        itemId: item.id,
        ...(confirmedInsecure ? { confirmedInsecure: true } : {}),
      });
    } catch (error) {
      busy = false;
      message(
        error instanceof RequestFailed && error.message
          ? error.message
          : t('Ausfüllen hat nicht geklappt.'),
        [],
      );
      return;
    }
    busy = false;
    if (answer.filled) {
      detach();
      deps.fill(answer.values, item.id, target);
      return;
    }
    if (field !== target) return;
    switch (answer.reason) {
      case 'insecure':
        message(t('Diese Seite ist nicht verschlüsselt (http). Trotzdem ausfüllen?'), [
          { label: t('Trotzdem ausfüllen'), primary: true, run: () => void pick(item, true) },
          { label: t('Abbrechen'), run: () => render(true) },
        ]);
        break;
      case 'reprompt':
        message(t('Dieser Eintrag verlangt dein Master-Passwort – öffne UwULock.'), [
          { label: t('UwULock öffnen'), primary: true, run: openPopup },
        ]);
        break;
      case 'locked':
        message(t('UwULock ist gesperrt.'), [
          { label: t('UwULock entsperren'), primary: true, run: openPopup },
        ]);
        break;
      default:
        message(t('Ausfüllen hat nicht geklappt.'), []);
    }
  };

  /** A new masked address from the account's UwUMail, for this tab's site, into the field. */
  const createMasked = async () => {
    if (busy || !field) return;
    busy = true;
    const target = field;
    message(t('Maskierte Adresse wird angelegt …'), []);
    let email: string;
    try {
      ({ email } = await ask<{ email: string }>({ type: 'content:masked-create' }));
    } catch (error) {
      busy = false;
      if (field !== target) return;
      const failed = error instanceof RequestFailed ? error : null;
      const text =
        (failed && uwuErrorText(failed.kind, failed.message)) ??
        (failed?.kind === 'locked'
          ? t('UwULock ist gesperrt.')
          : t('Die maskierte Adresse ließ sich nicht anlegen.'));
      const connect = failed?.kind === 'uwu:not_connected' || failed?.kind === 'uwu:revoked';
      message(
        text,
        connect
          ? [
              {
                label: t('Web-Tresor öffnen'),
                primary: true,
                run: () => {
                  void ask<unknown>({ type: 'content:open-masked-settings' }).catch(
                    () => undefined,
                  );
                  closeList(false);
                },
              },
            ]
          : [],
      );
      return;
    }
    busy = false;
    detach();
    deps.fillText(target, email);
  };

  function onListKey(event: KeyboardEvent) {
    if (!event.isTrusted) return;
    const all = options();
    const index = all.indexOf(ui?.root.activeElement as HTMLElement);
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
        closeList(true);
        break;
      case 'Tab':
        closeList(false);
        return;
      default:
        return;
    }
    event.preventDefault();
    event.stopPropagation();
  }

  const openList = (focusFirst: boolean) => {
    if (!ui) return;
    if (!menu) {
      menu = h('div', { class: 'menu panel' });
      menu.addEventListener('mousedown', (event) => event.preventDefault());
      ui.root.append(menu);
    }
    button?.setAttribute('aria-expanded', 'true');
    render(focusFirst);
    shownAt = performance.now();
    place();
  };

  function closeList(refocus: boolean) {
    menu?.remove();
    menu = null;
    button?.setAttribute('aria-expanded', 'false');
    if (refocus) field?.focus({ preventScroll: true });
  }

  // ── Following the field ─────────────────────────────────

  const onFieldKey = (event: KeyboardEvent) => {
    if (!event.isTrusted) return;
    if (event.key === 'ArrowDown' && !event.altKey && !event.ctrlKey && !event.metaKey) {
      event.preventDefault();
      if (menu) select(options()[0]);
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
    ui?.host.remove();
    ui = null;
    button = null;
    menu = null;
    field = null;
    busy = false;
  }

  const attach = (next: HTMLInputElement, nextKind: MenuKind, nextMaskable = false) => {
    if (field === next && kind === nextKind && maskable === nextMaskable && ui) return;
    detach();
    field = next;
    kind = nextKind;
    maskable = nextMaskable;
    ui = createHost(CSS);
    ui.root.addEventListener('focusout', onRootBlur);
    const host = ui.host;
    button = h('button', {
      class: 'button',
      type: 'button',
      title: 'UwULock',
      'aria-label': t('UwULock-Menü öffnen'),
      'aria-haspopup': 'listbox',
      'aria-expanded': 'false',
    });
    button.append(lockGlyph(BUTTON_MAX));
    button.addEventListener('mousedown', (event) => event.preventDefault());
    button.addEventListener('click', (event) => {
      if (!genuine(event, host)) return;
      if (menu) closeList(false);
      else openList(event.detail === 0);
    });
    ui.root.append(button);
    next.addEventListener('keydown', onFieldKey, true);
    next.addEventListener('focusout', onFieldBlur);
    window.addEventListener('scroll', schedule, { capture: true, passive: true });
    window.addEventListener('resize', schedule, { passive: true });
    place();
  };

  return {
    attach,
    detach,
    refresh: () => {
      if (menu) render(false);
    },
    check: () => {
      if (field) schedule();
    },
    field: () => field,
  };
}
