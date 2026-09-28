/**
 * Noticing that a login was sent: a form's `submit`, a click on a button that sends a form
 * with a password, or Enter in a password field. The values are read at that moment and sent
 * to the background, which decides whether to offer saving or updating them.
 *
 * Only events the browser made count (`isTrusted`): a page can't make UwULock offer to save
 * values it made up. One submit usually causes several of these events; the same values are
 * sent only once within a few seconds.
 */

import { ask } from '../shared/messages';
import type { ContentRequest } from '../shared/protocol';
import { findLoginFields, hintText, isPasswordField, type LoginFields } from './forms';

export type Submission = Extract<ContentRequest, { type: 'content:submitted' }>;

/**
 * What a form holds right now, as the background wants it; null when there is nothing to save
 * (no password, or a new password that doesn't match its confirmation).
 */
export function readSubmission(login: LoginFields): Submission | null {
  const username = login.username?.value.trim() || null;
  const submission = (password: string | null, newPassword: string | null): Submission => ({
    type: 'content:submitted',
    username,
    password,
    newPassword,
  });
  switch (login.kind) {
    case 'username-step':
      return username ? submission(null, null) : null;
    case 'login': {
      const password = login.password?.value ?? '';
      return password ? submission(password, null) : null;
    }
    case 'signup':
    case 'change-password': {
      const [fresh, confirm] = login.newPasswords;
      const value = fresh?.value ?? '';
      if (!value || (confirm && confirm.value !== value)) return null;
      return login.kind === 'signup'
        ? submission(value, null)
        : submission(login.password?.value || null, value);
    }
  }
}

const SUBMIT_HINT =
  /log.?in|sign.?in|anmelden|einloggen|weiter|next|continue|fortfahren|save|speichern|change|ändern|aendern|submit|absenden|update|aktualisieren|register|registrieren|create|erstellen/;

function isSubmitButton(el: Element): boolean {
  if (el instanceof HTMLButtonElement) {
    if (el.type === 'submit') return true;
    if (el.type === 'reset') return false;
  } else if (el instanceof HTMLInputElement) {
    if (el.type === 'submit' || el.type === 'image') return true;
    if (el.type !== 'button') return false;
  }
  const text = `${el.textContent ?? ''} ${(el as HTMLInputElement).value ?? ''} ${hintText(el)}`;
  return SUBMIT_HINT.test(text.slice(0, 300).toLowerCase());
}

function fieldsOf(login: LoginFields): Element[] {
  return [login.username, login.password, ...login.newPasswords].filter(
    (el): el is HTMLInputElement => !!el,
  );
}

function depthOf(node: Node | null): number {
  let depth = 0;
  for (let n = node; n; n = n.parentNode) depth += 1;
  return depth;
}

function commonDepth(a: Node, b: Node): number {
  const ancestors = new Set<Node>();
  for (let n: Node | null = a; n; n = n.parentNode) ancestors.add(n);
  for (let n: Node | null = b; n; n = n.parentNode) if (ancestors.has(n)) return depthOf(n);
  return 0;
}

/** The login form `el` (a form, field or button) belongs to, or the one closest to it. */
export function loginFor(el: Element): LoginFields | null {
  const form = el instanceof HTMLFormElement ? el : ((el as HTMLInputElement).form ?? null);
  const logins = findLoginFields(form ?? (el.getRootNode() as Document | ShadowRoot));
  const own = logins.find((login) => fieldsOf(login).includes(el));
  if (own) return own;
  if (form) return logins[0] ?? null;
  let best: LoginFields | null = null;
  let bestDepth = -1;
  for (const login of logins) {
    const first = fieldsOf(login)[0];
    const depth = first ? commonDepth(first, el) : 0;
    if (depth > bestDepth) [best, bestDepth] = [login, depth];
  }
  return best;
}

export type SaveDetectOptions = {
  /** PageInfo.savePrompt, asked at the moment of the event. */
  enabled: () => boolean;
  send?: (submission: Submission) => void;
  /** For tests: jsdom's dispatched events are never trusted. */
  trusted?: (event: Event) => boolean;
  now?: () => number;
};

/** FNV-1a: tells two submissions apart without keeping the password around. */
function fingerprint(text: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

const REPEAT_MS = 3000;

/** Listens on `doc` (capture phase). Returns a function that stops listening. */
export function installSaveDetection(doc: Document, options: SaveDetectOptions): () => void {
  const send =
    options.send ??
    ((submission: Submission) => {
      void ask<unknown>(submission).catch(() => undefined);
    });
  const trusted = options.trusted ?? ((event: Event) => event.isTrusted);
  const now = options.now ?? (() => Date.now());
  let last: { print: number; at: number } | null = null;

  const report = (el: Element | null) => {
    if (!el || !options.enabled()) return;
    const login = loginFor(el);
    const submission = login && readSubmission(login);
    if (!submission) return;
    const print = fingerprint(
      JSON.stringify([submission.username, submission.password, submission.newPassword]),
    );
    const at = now();
    if (last && last.print === print && at - last.at < REPEAT_MS) return;
    last = { print, at };
    send(submission);
  };

  const target = (event: Event) => (event.composedPath()[0] ?? event.target) as Element | null;

  const onSubmit = (event: Event) => {
    if (!trusted(event)) return;
    const form = target(event);
    if (form instanceof HTMLFormElement) report(form);
  };

  const onClick = (event: Event) => {
    if (!trusted(event)) return;
    const start = target(event);
    const button =
      start instanceof Element
        ? start.closest(
            'button, input[type="submit"], input[type="image"], input[type="button"], [role="button"]',
          )
        : null;
    if (button && isSubmitButton(button)) report(button);
  };

  const onKeyDown = (event: Event) => {
    const key = event as KeyboardEvent;
    if (key.key !== 'Enter' || key.isComposing || !trusted(event)) return;
    const field = target(event);
    if (!(field instanceof HTMLInputElement)) return;
    if (isPasswordField(field)) report(field);
    else if (options.enabled() && field === loginFor(field)?.username) report(field);
  };

  doc.addEventListener('submit', onSubmit, true);
  doc.addEventListener('click', onClick, true);
  doc.addEventListener('keydown', onKeyDown, true);
  return () => {
    doc.removeEventListener('submit', onSubmit, true);
    doc.removeEventListener('click', onClick, true);
    doc.removeEventListener('keydown', onKeyDown, true);
  };
}
