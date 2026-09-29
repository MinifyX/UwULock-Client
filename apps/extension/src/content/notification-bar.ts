/**
 * The bar at the top right of a page that offers to save or update a login after a form was
 * sent (top frame only), and asks before filling a plain-http page. One bar at a time; a new
 * one replaces the old. Its buttons take only clicks the guard of ui.ts accepts: the bar seen
 * whole and unchanged for a moment, the pointer pressed on the button then.
 */

import { t } from '../shared/i18n';
import { ask, RequestFailed } from '../shared/messages';
import type { SaveAnswer, SavePrompt } from '../shared/protocol';
import { createGuard, createHost, crossGlyph, h, lockGlyph, type Guard, type Host } from './ui';

const CSS = `
.bar {
  position: fixed;
  top: 12px;
  right: 12px;
  width: 360px;
  max-width: calc(100vw - 24px);
  padding: 14px 16px 16px;
  display: grid;
  gap: 10px;
}
.head { display: flex; align-items: flex-start; gap: 10px; }
.title { flex: 1; font-weight: 700; font-size: 14px; overflow-wrap: anywhere; }
.user {
  font-size: 13px;
  color: var(--uwu-muted);
  overflow-wrap: anywhere;
  padding-left: 30px;
}
.close {
  flex: none;
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  margin: -4px -6px 0 0;
  border: 0;
  border-radius: 10px;
  background: transparent;
  color: var(--uwu-muted);
}
.close:hover { background: var(--uwu-elevated); color: var(--uwu-ink); }
.result { font-weight: 700; }
`;

type Bar = Host & { guard: Guard; timer: ReturnType<typeof setTimeout> | null };

let current: Bar | null = null;

export function closeBar() {
  if (!current) return;
  if (current.timer) clearTimeout(current.timer);
  current.guard.dispose();
  current.host.remove();
  current = null;
}

/** Shows `text` in place of the bar's buttons, then removes the bar. */
function finish(bar: Bar, text: string | null, alarm = false) {
  if (current !== bar) return;
  if (!text) {
    closeBar();
    return;
  }
  const panel = bar.root.querySelector('.bar');
  if (panel) {
    const note = h(
      'div',
      { class: alarm ? 'result alarm' : 'result', role: alarm ? 'alert' : 'status' },
      text,
    );
    const title = panel.querySelector('.title');
    panel.replaceChildren(h('div', { class: 'head' }, lockGlyph(20), title), note);
  }
  bar.timer = setTimeout(
    () => {
      if (current === bar) closeBar();
    },
    alarm ? 5000 : 2500,
  );
}

type Action = { label: string; primary?: boolean; run: () => void | Promise<void> };

function open(title: string, detail: string | null, actions: Action[], onClose: () => void): Bar {
  closeBar();
  const host = createHost(CSS);
  const bar: Bar = { ...host, guard: createGuard(host), timer: null };
  current = bar;

  const busy = (on: boolean) => {
    for (const button of Array.from(bar.root.querySelectorAll('button'))) button.disabled = on;
  };
  const guard = (run: () => void | Promise<void>) => async (event: Event) => {
    const button = event.currentTarget;
    if (!(button instanceof HTMLElement) || !bar.guard.accepts(event, button)) return;
    busy(true);
    try {
      await run();
    } finally {
      if (current === bar) busy(false);
    }
  };

  const close = h('button', { class: 'close', type: 'button', 'aria-label': t('Schließen') });
  close.append(crossGlyph());
  close.addEventListener('click', guard(onClose));

  const buttons = actions.map((action) => {
    const button = h(
      'button',
      { class: action.primary ? 'primary' : 'secondary', type: 'button' },
      action.label,
    );
    button.addEventListener('click', guard(action.run));
    return button;
  });

  const panel = h(
    'div',
    { class: 'bar panel', role: 'dialog', 'aria-labelledby': 'uwulock-bar-title' },
    h(
      'div',
      { class: 'head' },
      lockGlyph(20),
      h('div', { class: 'title', id: 'uwulock-bar-title' }, title),
      close,
    ),
    detail !== null && h('div', { class: 'user' }, detail),
    h('div', { class: 'actions' }, ...buttons),
  );
  bar.root.append(panel);
  bar.guard.watch(panel);
  bar.guard.shown();
  return bar;
}

/** The save or update offer; the answer goes to the background. */
export function showSavePrompt(prompt: SavePrompt) {
  const answer = async (choice: SaveAnswer) => {
    try {
      await ask<unknown>({ type: 'content:prompt-answer', id: prompt.id, answer: choice });
      const done =
        choice === 'save' ? t('Gespeichert') : choice === 'update' ? t('Aktualisiert') : null;
      finish(bar, done);
    } catch (error) {
      const message =
        error instanceof RequestFailed && error.message
          ? error.message
          : t('Das hat nicht geklappt.');
      finish(bar, message, true);
    }
  };
  const update = prompt.action === 'update';
  const title = update
    ? t('Passwort für {itemName} bei {host} aktualisieren?', {
        itemName: prompt.itemName ?? prompt.host,
        host: prompt.host,
      })
    : t('Login bei {host} speichern?', { host: prompt.host });
  const bar = open(
    title,
    prompt.username || t('Ohne Benutzernamen'),
    [
      {
        label: update ? t('Aktualisieren') : t('Speichern'),
        primary: true,
        run: () => answer(update ? 'update' : 'save'),
      },
      { label: t('Nie für diese Seite'), run: () => answer('never') },
      { label: t('Nicht jetzt'), run: () => answer('dismiss') },
    ],
    () => answer('dismiss'),
  );
}

/** Asks before filling a plain-http page; `confirm` runs after a real click. */
export function showInsecureConfirm(confirm: () => Promise<void> | void) {
  const bar = open(
    t('Diese Seite ist nicht verschlüsselt (http). Trotzdem ausfüllen?'),
    null,
    [
      {
        label: t('Trotzdem ausfüllen'),
        primary: true,
        run: async () => {
          closeBar();
          await confirm();
        },
      },
      { label: t('Abbrechen'), run: () => finish(bar, null) },
    ],
    () => finish(bar, null),
  );
}
