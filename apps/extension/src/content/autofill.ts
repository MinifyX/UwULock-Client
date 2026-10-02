/**
 * The content script, injected at `document_idle` into every http(s) frame. It finds login,
 * card and address forms, shows the inline menu in their fields, fills them when somebody
 * picks an item (in the menu, or in the popup: `bg:fill-offer`), notices sent logins, and in
 * the top frame shows the save/update bar.
 *
 * The page can see and fake everything in its DOM, so: our UI lives in closed shadow roots and
 * only reacts to trusted events; values from the background go into fields and nowhere else,
 * and are not kept; which page this is, the background takes from the sender.
 *
 * A frame without any input field doesn't ask the background anything until one appears.
 */

import { ext } from '../shared/browser';
import { setLanguage } from '../shared/i18n';
import { ask } from '../shared/messages';
import type {
  BackgroundMessage,
  FillAnswer,
  FillValues,
  ItemKind,
  PageInfo,
  SavePrompt,
} from '../shared/protocol';
import { answerFrameDocument } from './frame';
import { fillCard, fillField, fillIdentity, fillLogin } from './fill';
import {
  scanFields,
  type CardFields,
  type IdentityFields,
  type LoginFields,
  type Scan,
  type TotpFields,
} from './forms';
import { createInlineMenu, type MenuKind } from './inline-menu';
import { showInsecureConfirm, showSavePrompt } from './notification-bar';
import { installSaveDetection } from './save-detect';

const CONNECTOR = '/webauthn-fallback-connector.html';
/** How often the page's info is asked again because new fields appeared, at most. */
const INFO_INTERVAL_MS = 2000;
/** Added nodes looked at per batch of DOM changes. */
const MUTATION_CAP = 200;

const isTop = (() => {
  try {
    return window.top === window;
  } catch {
    return false;
  }
})();

let info: PageInfo | null = null;
let infoAsked: Promise<PageInfo | null> | null = null;
let infoAt = 0;
let scan: Scan | null = null;
let scannedAt = 0;
let filling = false;

// ── What the page has ─────────────────────────────────────

function fields(fresh = false): Scan {
  if (!scan || fresh) {
    scan = scanFields(document);
    scannedAt = Date.now();
  }
  return scan;
}

function deepActive(): Element | null {
  let el: Element | null = document.activeElement;
  while (el?.shadowRoot?.activeElement) el = el.shadowRoot.activeElement;
  return el;
}

const loginFill = (login: LoginFields) => login.kind !== 'signup';

function contextOf(s: Scan, el: Element): MenuKind | null {
  for (const login of s.logins) {
    if (loginFill(login) && (el === login.username || el === login.password)) return 'login';
  }
  if (s.totps.some((totp) => totp.inputs.includes(el as HTMLInputElement))) return 'login';
  for (const card of s.cards) {
    if (el === card.fields.number || el === card.fields.name) return 'card';
  }
  for (const identity of s.identities) {
    if (Object.values(identity.fields).includes(el as HTMLInputElement)) return 'identity';
  }
  return null;
}

/** The form holding `near`, else the first one; for logins, full logins before single steps. */
function prefer<T>(
  list: T[],
  near: Element | null,
  has: (item: T, el: Element) => boolean,
): T | null {
  if (near) {
    const own = list.find((item) => has(item, near));
    if (own) return own;
  }
  return list[0] ?? null;
}

const inForm = (
  form: HTMLFormElement | null,
  fields: (Element | null | undefined)[],
  el: Element,
) => fields.includes(el) || (!!form && form.contains(el));

function pickLogin(s: Scan, near: Element | null): LoginFields | null {
  const order: LoginFields['kind'][] = ['login', 'username-step', 'change-password'];
  const logins = s.logins
    .filter(loginFill)
    .sort((a, b) => order.indexOf(a.kind) - order.indexOf(b.kind));
  return prefer(logins, near, (login, el) =>
    inForm(login.form, [login.username, login.password], el),
  );
}

function pickTotp(s: Scan, near: Element | null): TotpFields | null {
  return prefer(s.totps, near, (totp, el) => inForm(totp.form, totp.inputs, el));
}

function pickCard(s: Scan, near: Element | null): CardFields | null {
  return prefer(s.cards, near, (card, el) => inForm(card.form, Object.values(card.fields), el));
}

function pickIdentity(s: Scan, near: Element | null): IdentityFields | null {
  return prefer(s.identities, near, (identity, el) =>
    inForm(identity.form, Object.values(identity.fields), el),
  );
}

function canFill(s: Scan, kind: ItemKind): boolean {
  if (kind === 'login') return s.logins.some(loginFill) || s.totps.length > 0;
  if (kind === 'card') return s.cards.length > 0;
  if (kind === 'identity') return s.identities.length > 0;
  return false;
}

// ── Filling ───────────────────────────────────────────────

function apply(values: FillValues, itemId: string, near: Element | null) {
  filling = true;
  try {
    const s = fields(true);
    if (values.kind === 'login') {
      const totp = pickTotp(s, near);
      fillLogin(pickLogin(s, near), totp, values);
      if (values.totp && !totp) {
        void ask<unknown>({ type: 'content:copy-totp', itemId }).catch(() => undefined);
      }
    } else if (values.kind === 'card') {
      const card = pickCard(s, near);
      if (card) fillCard(card, values);
    } else {
      const identity = pickIdentity(s, near);
      if (identity) fillIdentity(identity, values);
    }
  } finally {
    filling = false;
  }
}

async function claim(
  offer: { token: string; itemId: string },
  confirmedInsecure: boolean,
  near: Element | null = null,
) {
  let answer: FillAnswer;
  try {
    answer = await ask<FillAnswer>({
      type: 'content:fill',
      itemId: offer.itemId,
      token: offer.token,
      ...(confirmedInsecure ? { confirmedInsecure: true } : {}),
    });
  } catch {
    return;
  }
  if (answer.filled) {
    apply(answer.values, offer.itemId, near ?? deepActive());
  } else if (answer.reason === 'insecure' && !confirmedInsecure) {
    await loadInfo();
    showInsecureConfirm(() => claim(offer, true, near));
  }
}

function onFillOffer(offer: { token: string; itemId: string; kind: ItemKind; session?: string }) {
  // Picked in this frame's inline menu: into the menu's field, and the menu closes.
  if (offer.session !== undefined) {
    const field = menu.field();
    if (!field || offer.session !== menu.session()) return;
    menu.detach();
    void claim(offer, false, field);
    return;
  }
  if (!canFill(fields(true), offer.kind)) return;
  void claim(offer, false);
}

// ── The page's info ───────────────────────────────────────

function fillText(field: HTMLInputElement, value: string) {
  filling = true;
  try {
    fillField(field, value);
  } finally {
    filling = false;
  }
}

const menu = createInlineMenu({ info: () => info, fillText });

function loadInfo(force = false): Promise<PageInfo | null> {
  if (infoAsked) return infoAsked;
  if (info && !force) return Promise.resolve(info);
  infoAt = Date.now();
  infoAsked = ask<PageInfo>({ type: 'content:page-info' })
    .then((value) => {
      info = value;
      setLanguage(value.language);
      return value;
    })
    .catch(() => info)
    .finally(() => {
      infoAsked = null;
    });
  return infoAsked.then((value) => {
    menu.refresh();
    const active = deepActive();
    if (active instanceof HTMLInputElement && !menu.field()) onFocus(active);
    return value;
  });
}

// ── Focus, and fields that come and go ────────────────────

/** A username or email field of any login form (sign-ups too) or address form. */
function isUsernameField(s: Scan, el: Element): boolean {
  return (
    s.logins.some((login) => login.username === el) ||
    s.identities.some((identity) => identity.fields.email === el)
  );
}

function onFocus(el: HTMLInputElement) {
  if (filling || !info?.inlineMenu) return;
  let s = fields();
  let kind = contextOf(s, el);
  if (!kind && Date.now() - scannedAt > 500) {
    s = fields(true);
    kind = contextOf(s, el);
  }
  // UwULock Server's masked addresses, in the username or email field of any form.
  const maskable =
    info.state === 'unlocked' &&
    info.uwuFeatures.includes('masked-addresses') &&
    isUsernameField(s, el);
  if (!kind && maskable) kind = 'signup';
  if (!kind) return;
  if (
    kind !== 'login' &&
    !maskable &&
    (info.state !== 'unlocked' || !(kind === 'card' ? info.counts.cards : info.counts.identities))
  )
    return;
  menu.attach(el, kind, maskable);
}

function onFocusIn(event: FocusEvent) {
  if (filling) return;
  const el = event.composedPath()[0];
  if (!(el instanceof HTMLInputElement)) return;
  if (!info) {
    void loadInfo();
    return;
  }
  onFocus(el);
}

let changeTimer: ReturnType<typeof setTimeout> | null = null;

function onFieldsChanged() {
  changeTimer = null;
  scan = null;
  menu.check();
  if (!info || Date.now() - infoAt > INFO_INTERVAL_MS) void loadInfo(true);
}

function hasField(node: Node): boolean {
  if (!(node instanceof Element)) return false;
  return (
    node instanceof HTMLInputElement ||
    node instanceof HTMLSelectElement ||
    node.querySelector('input, select') !== null
  );
}

const observer = new MutationObserver((records) => {
  let seen = 0;
  let added = false;
  let changed = false;
  for (const record of records) {
    if (record.type === 'attributes') changed = true;
    for (const node of Array.from(record.addedNodes)) {
      if (added || seen >= MUTATION_CAP) break;
      seen += 1;
      added = hasField(node);
    }
    if (record.removedNodes.length) changed = true;
    if (added || seen >= MUTATION_CAP) break;
  }
  if (changed && !added) {
    scan = null;
    menu.check();
  }
  if (added && !changeTimer) {
    changeTimer = setTimeout(() => requestAnimationFrame(onFieldsChanged), 300);
  }
});

// ── Messages from the background ──────────────────────────

function onMessage(
  message: unknown,
  sender: chrome.runtime.MessageSender,
  respond: (answer: unknown) => void,
): undefined {
  if (answerFrameDocument(message, sender, respond)) return;
  if (sender.id !== ext.runtime.id || !message || typeof message !== 'object') return;
  // From the background only, never from another extension page (the menu's frame has a port).
  if (sender.tab !== undefined) return;
  const msg = message as BackgroundMessage;
  const handled = menu.onMessage(msg);
  if (handled) {
    respond(handled.answer);
    return;
  }
  switch (msg.type) {
    case 'bg:fill-offer':
      onFillOffer(msg);
      break;
    case 'bg:save-prompt':
      if (isTop) void loadInfo().then(() => showSavePrompt(msg.prompt));
      break;
    case 'bg:vault-changed':
      if (info || infoAsked) void loadInfo(true);
      break;
  }
  return undefined;
}

// ── Start ─────────────────────────────────────────────────

function connector() {
  window.addEventListener('message', (event) => {
    if (event.source !== window) return;
    const data = event.data as { command?: unknown; data?: unknown; remember?: unknown } | null;
    if (!data || typeof data !== 'object' || data.command !== 'webAuthnResult') return;
    if (typeof data.data !== 'string' || typeof data.remember !== 'boolean') return;
    void ask<unknown>({
      type: 'content:webauthn-result',
      data: data.data,
      remember: data.remember,
    }).catch(() => undefined);
  });
}

function start() {
  if (location.pathname.endsWith(CONNECTOR)) {
    connector();
    return;
  }
  ext.runtime.onMessage.addListener(onMessage);
  installSaveDetection(document, { enabled: () => !!info?.savePrompt });
  document.addEventListener('focusin', onFocusIn, true);
  observer.observe(document.documentElement, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ['type'],
  });
  if (isTop || document.querySelector('input')) void loadInfo();
  if (isTop) {
    void ask<SavePrompt | null>({ type: 'content:pending-prompt' })
      .then(async (prompt) => {
        if (!prompt) return;
        await loadInfo();
        showSavePrompt(prompt);
      })
      .catch(() => undefined);
  }
}

const flag = '__uwulockContent';
const scope = globalThis as unknown as Record<string, boolean>;
if (!scope[flag]) {
  scope[flag] = true;
  start();
}
