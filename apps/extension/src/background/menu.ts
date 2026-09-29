/**
 * The inline menu's list, drawn in an extension page (menu.html) in a frame inside the web page
 * (security review 0.3, CL-L8).
 *
 * The content script only keeps the small button in the field. When the list opens, it asks
 * for a session here (`content:menu-open`) and shows `menu.html#<session>` in a frame under the
 * field. That page connects a port named `menu`, says hello with the session, and gets the names
 * to list — the content script never sees them. A pick in it arrives here over that port, from
 * the extension's own page: a page's renderer, even a compromised one, can't send it. The pick
 * becomes a one-time offer (autofill.ts) for exactly the frame the menu belongs to, which that
 * frame's content script claims to fill the field.
 *
 * What still lives in the page — the frame element, where it is and what lies over it — is
 * checked by the content script's guard (content/ui.ts) before each pick is taken
 * (`bg:menu-guard`); inside the frame, the page checks itself (menu/main.ts).
 */

import { ext } from '../shared/browser';
import { resolveLanguage } from '../shared/i18n';
import type {
  BackgroundMessage,
  MenuKind,
  MenuMessage,
  MenuPickAnswer,
  MenuRequest,
  MenuView,
} from '../shared/protocol';
import { isFillableUrl, isInsecureUrl } from '../shared/uri';
import * as autofill from './autofill';
import * as extras from './extras';
import { failure } from './http';
import * as session from './session';
import { settings } from './settings';
import { activeAccount } from './store';
import { hasFeature } from './uwu';

type Sender = chrome.runtime.MessageSender;
type Port = chrome.runtime.Port;

type Session = {
  id: string;
  tabId: number;
  /** The frame with the field, and its address when the menu opened (from the browser). */
  frameId: number;
  url: string;
  kind: MenuKind;
  maskable: boolean;
  created: number;
  /** The menu's frame, once it said hello; no other may take the session. */
  port: Port | null;
  /** The field's ↓ asked to select the first entry before the frame said hello. */
  selectFirst: boolean;
  busy: boolean;
};

const sessions = new Map<string, Session>();
/** Sessions nobody said hello to are forgotten after this. */
const UNCLAIMED_MS = 30_000;
const MAX_SESSIONS = 64;

function randomId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

function forgetOld() {
  const now = Date.now();
  for (const [id, found] of sessions) {
    if (!found.port && now - found.created > UNCLAIMED_MS) sessions.delete(id);
  }
  while (sessions.size >= MAX_SESSIONS) {
    const oldest = sessions.keys().next().value;
    if (oldest === undefined) break;
    close(oldest);
  }
}

function close(id: string) {
  const found = sessions.get(id);
  if (!found) return;
  sessions.delete(id);
  try {
    found.port?.disconnect();
  } catch {
    // Gone already.
  }
}

/** The field's frame, as the menu's lists and fills judge it. */
function fieldSender(found: Session): Sender {
  return {
    id: ext.runtime.id,
    url: found.url,
    frameId: found.frameId,
    tab: { id: found.tabId } as chrome.tabs.Tab,
  };
}

function toContent(found: Session, message: BackgroundMessage): Promise<unknown> {
  return ext.tabs.sendMessage(found.tabId, message, { frameId: found.frameId });
}

function post(found: Session, message: MenuMessage) {
  try {
    found.port?.postMessage(message);
  } catch {
    // The frame is gone; its disconnect closes the session.
  }
}

// ── From the content script ───────────────────────────────

const KINDS = new Set<MenuKind>(['login', 'card', 'identity', 'signup']);

/** A field got the menu's list: a session for its frame. */
export function open(sender: Sender, kind: unknown, maskable: unknown): { session: string } {
  const tabId = sender.tab?.id;
  const url = sender.url ?? '';
  if (tabId === undefined || sender.frameId === undefined || !isFillableUrl(url)) {
    throw { kind: 'refused', message: 'Not a web page.' };
  }
  if (typeof kind !== 'string' || !KINDS.has(kind as MenuKind)) {
    throw { kind: 'invalid', message: 'Unknown field.' };
  }
  forgetOld();
  // One list per frame: a new one replaces the old.
  for (const [id, found] of sessions) {
    if (found.tabId === tabId && found.frameId === sender.frameId) close(id);
  }
  const id = randomId();
  sessions.set(id, {
    id,
    tabId,
    frameId: sender.frameId,
    url,
    kind: kind as MenuKind,
    maskable: maskable === true,
    created: Date.now(),
    port: null,
    selectFirst: false,
    busy: false,
  });
  return { session: id };
}

/** The session, if it belongs to the frame that asks. */
function own(sender: Sender, id: unknown): Session | null {
  const found = typeof id === 'string' ? sessions.get(id) : undefined;
  if (!found || found.tabId !== sender.tab?.id || found.frameId !== sender.frameId) return null;
  return found;
}

export function closeFromContent(sender: Sender, id: unknown) {
  const found = own(sender, id);
  if (found) close(found.id);
}

export function focusFromContent(sender: Sender, id: unknown) {
  const found = own(sender, id);
  if (!found) return;
  if (found.port) post(found, { type: 'select-first' });
  else found.selectFirst = true;
}

ext.tabs.onRemoved.addListener((tabId) => {
  for (const [id, found] of sessions) if (found.tabId === tabId) close(id);
});

// ── From the menu's frame ─────────────────────────────────

/** A page of this extension at /menu.html, in a tab: the menu's frame. */
export function isMenuFrame(sender: Sender | undefined): boolean {
  if (!sender || sender.id !== ext.runtime.id || typeof sender.url !== 'string') return false;
  try {
    const url = new URL(sender.url);
    return (
      url.protocol === new URL(ext.runtime.getURL('/')).protocol && url.pathname === '/menu.html'
    );
  } catch {
    return false;
  }
}

async function view(found: Session): Promise<MenuView> {
  const config = await settings();
  const state = await session.vaultState();
  const account = state === 'unlocked' ? await activeAccount() : null;
  const kind = found.kind;
  return {
    state,
    kind,
    items:
      state === 'unlocked' && kind !== 'signup'
        ? await autofill.frameItems(fieldSender(found), kind)
        : [],
    masked: state === 'unlocked' && found.maskable && hasFeature(account, 'masked-addresses'),
    language: resolveLanguage(config.language),
  };
}

async function pick(found: Session, itemId: unknown, confirmedInsecure: boolean) {
  const answer = (value: MenuPickAnswer) => post(found, { type: 'picked', answer: value });
  if (!session.unlockedAccountId()) return answer({ filled: false, reason: 'locked' });
  // Only what this menu lists: the frame's own logins, or its cards and addresses.
  const listed = (await view(found)).items.find((item) => item.id === itemId);
  if (!listed) return answer({ filled: false, reason: 'no-match' });
  if (listed.reprompt) {
    // The master password, for every fill: the popup asks (CL-I3).
    await session.openPopup();
    return answer({ filled: false, reason: 'reprompt' });
  }
  // What lives in the page: was the frame there, uncovered and unchanged, when it was used?
  const seen = await toContent(found, { type: 'bg:menu-guard', session: found.id }).catch(
    () => false,
  );
  if (seen !== true) return answer({ filled: false, reason: 'unseen' });
  if (isInsecureUrl(found.url) && !confirmedInsecure) {
    return answer({ filled: false, reason: 'insecure' });
  }
  await autofill.offer(found.tabId, listed.id, false, confirmedInsecure, false, {
    frameId: found.frameId,
    session: found.id,
  });
  answer({ filled: true });
}

async function masked(found: Session) {
  try {
    const account = await session.requireUnlocked();
    if (!found.maskable || !hasFeature(account, 'masked-addresses')) {
      throw { kind: 'refused', message: 'Not here.' };
    }
    const seen = await toContent(found, { type: 'bg:menu-guard', session: found.id }).catch(
      () => false,
    );
    if (seen !== true) return;
    // For the tab's site, which the background knows; only the address goes to the page.
    const url = autofill.tabUrl(found.tabId) ?? (found.frameId === 0 ? found.url : null);
    if (!url) throw { kind: 'refused', message: 'Not a web page.' };
    const created = await extras.createMasked(account, url, null);
    post(found, { type: 'masked', failure: null });
    await toContent(found, { type: 'bg:fill-text', session: found.id, value: created.email });
  } catch (error) {
    post(found, { type: 'masked', failure: failure(error) });
  }
}

async function onRequest(port: Port, message: MenuRequest) {
  if (message.type === 'hello') {
    const found = typeof message.session === 'string' ? sessions.get(message.session) : undefined;
    // The session's own tab, and the first frame to claim it; another frame, even one the page
    // made itself with the same address, gets nothing.
    if (!found || found.port || found.tabId !== port.sender?.tab?.id) {
      port.postMessage({ type: 'gone' } satisfies MenuMessage);
      return;
    }
    found.port = port;
    port.onDisconnect.addListener(() => {
      if (sessions.get(found.id)?.port === port) sessions.delete(found.id);
    });
    post(found, { type: 'view', view: await view(found) });
    if (found.selectFirst) post(found, { type: 'select-first' });
    found.selectFirst = false;
    return;
  }
  if (message.type === 'open-popup') {
    // Harmless from any menu frame, also one whose session is gone.
    await session.openPopup();
  }
  const found = [...sessions.values()].find((s) => s.port === port);
  if (!found) {
    port.postMessage({ type: 'gone' } satisfies MenuMessage);
    return;
  }
  switch (message.type) {
    case 'pick':
    case 'masked': {
      if (found.busy) return;
      found.busy = true;
      try {
        if (message.type === 'pick') {
          await pick(found, message.itemId, message.confirmedInsecure === true);
        } else {
          await masked(found);
        }
      } catch (error) {
        const kind = failure(error).kind;
        post(found, {
          type: 'picked',
          answer: {
            filled: false,
            reason: kind === 'not-found' ? 'not-found' : kind === 'locked' ? 'locked' : 'refused',
          },
        });
      } finally {
        found.busy = false;
      }
      return;
    }
    case 'open-popup':
      await toContent(found, { type: 'bg:menu-close', session: found.id, refocus: false }).catch(
        () => undefined,
      );
      return;
    case 'open-masked-settings': {
      const account = await activeAccount();
      if (hasFeature(account, 'masked-addresses') && account) {
        await ext.tabs.create({ url: extras.maskedSettingsUrl(account) });
      }
      await toContent(found, { type: 'bg:menu-close', session: found.id, refocus: false }).catch(
        () => undefined,
      );
      return;
    }
    case 'size': {
      const height = Number(message.height);
      if (!Number.isFinite(height)) return;
      await toContent(found, {
        type: 'bg:menu-size',
        session: found.id,
        height: Math.max(0, Math.min(400, Math.round(height))),
      }).catch(() => undefined);
      return;
    }
    case 'close':
      await toContent(found, {
        type: 'bg:menu-close',
        session: found.id,
        refocus: message.refocus === true,
      }).catch(() => undefined);
      return;
  }
}

/** A port from a menu frame (checked by the caller with `isMenuFrame`). */
export function connect(port: Port, restored: Promise<void>) {
  port.onMessage.addListener((message: unknown) => {
    if (!message || typeof message !== 'object') return;
    void restored.then(() => onRequest(port, message as MenuRequest)).catch(() => undefined);
  });
}

/** The vault changed: open menus list again. */
export async function refresh() {
  for (const found of sessions.values()) {
    if (found.port) post(found, { type: 'view', view: await view(found) });
  }
}
