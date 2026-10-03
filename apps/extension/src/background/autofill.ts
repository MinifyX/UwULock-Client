/**
 * Filling pages, and saving what was typed into them.
 *
 * A content script learns how many items fit, never their names or values, until somebody
 * picked an item: in the inline menu's frame (an extension page, menu.ts), in the popup, with
 * the shortcut or the context menu. Every pick becomes an offer with a one-time token, and
 * nothing is filled without one — a page's renderer can't make up a pick. Which page asks is taken
 * from the sender — the frame's own address — and an item is only handed to a frame whose
 * address matches one of its addresses (Bitwarden's match detection, see shared/uri.ts). So a
 * login for `bank.example` never reaches an iframe from `ads.example` on the bank's page. Only
 * when somebody explicitly picks an item in the popup for a page it doesn't match, the top
 * frame may have it — never an iframe. Plain http pages ask first.
 *
 * Cards and addresses have no address to match. They are listed to, and filled into, only the
 * page itself and frames from the same origin as it and as every frame in between — never an
 * ad's or another site's frame, whatever that frame asks — and only after somebody picked one,
 * in the page's menu or the popup.
 *
 * A frame's address isn't always its origin: a sandboxed document (an iframe with `sandbox`, a
 * page served with `Content-Security-Policy: sandbox`) has the site's address but an opaque
 * origin, and no business with the site's items (R4-2). So before anything is listed or filled,
 * the background asks the frame's content script for its document's real origin and its
 * ancestors' (`frameDocument`), and on Chromium also checks the sender's `origin`.
 *
 * Sent logins are kept in the background's memory, and offered to save in the page's
 * notification bar — or, while the vault is locked, in `storage.session` until it is unlocked.
 */

import { ext, isFirefox } from '../shared/browser';
import type {
  FillAnswer,
  FillValues,
  ItemKind,
  ItemSummary,
  MenuKind,
  PageInfo,
  PageItem,
  PendingSave,
  SaveAnswer,
  SavePrompt,
  TabItems,
  BackgroundMessage,
} from '../shared/protocol';
import { resolveLanguage } from '../shared/i18n';
import { FRAME_DOCUMENT, type FrameDocument } from '../content/frame';
import { hostnameOf, isFillableUrl, isInsecureUrl, itemMatches, pageDomains } from '../shared/uri';
import * as clipboard from './clipboard';
import { changed } from './events';
import * as session from './session';
import { settings, updateSettings } from './settings';
import { activeAccount, session as sessionStore, setSession } from './store';
import * as vault from './vault';
import { callJson } from './wasm';

type Sender = chrome.runtime.MessageSender;

// ── Which items belong to a page ──────────────────────────

/** The top frame's address per tab, as its content script reported it. */
const tabUrls = new Map<number, string>();
/** When an item was last filled, for putting it first. */
const lastUsed = new Map<string, number>();

export function tabUrl(tabId: number): string | undefined {
  return tabUrls.get(tabId);
}

ext.tabs.onRemoved.addListener((tabId) => {
  tabUrls.delete(tabId);
  seenUsernames.delete(tabId);
});

function order(a: vault.IndexEntry, b: vault.IndexEntry): number {
  return (
    (lastUsed.get(b.id) ?? 0) - (lastUsed.get(a.id) ?? 0) ||
    Number(b.favorite) - Number(a.favorite) ||
    a.name.localeCompare(b.name)
  );
}

export type MatchScope = {
  /** The address is a top frame's: regular expressions are tried (never in frames, CL-L11). */
  topFrame: boolean;
  /**
   * Leave out logins that match only through the account's own equivalent domains, which the
   * server hands out unencrypted: for fills nobody picked an item for (the shortcut).
   */
  strict?: boolean;
};

/** Whether a login's addresses match `url`. */
async function loginMatches(entry: vault.IndexEntry, url: string, scope: MatchScope) {
  const { defaultMatch } = await settings();
  const { strict, wide } = pageDomains(url, vault.domains());
  return itemMatches(entry.uris ?? [], url, scope.strict ? strict : wide, defaultMatch, {
    regex: scope.topFrame,
  });
}

/** The logins whose addresses match `url`, best first. */
export async function matchingLogins(url: string, scope: MatchScope): Promise<vault.IndexEntry[]> {
  if (!session.unlockedAccountId() || !isFillableUrl(url)) return [];
  const { defaultMatch } = await settings();
  const { strict, wide } = pageDomains(url, vault.domains());
  const domains = scope.strict ? strict : wide;
  return vault
    .autofillIndex()
    .filter(
      (e) =>
        e.kind === 'login' &&
        !e.archived &&
        itemMatches(e.uris ?? [], url, domains, defaultMatch, { regex: scope.topFrame }),
    )
    .sort(order);
}

function pageItem(entry: vault.IndexEntry): PageItem {
  return {
    id: entry.id,
    kind: entry.kind,
    name: entry.name,
    subtitle: entry.subtitle,
    favorite: entry.favorite,
    hasTotp: entry.hasTotp,
    reprompt: entry.reprompt,
  };
}

function ofKind(kind: ItemKind): vault.IndexEntry[] {
  return vault
    .autofillIndex()
    .filter((e) => e.kind === kind && !e.archived)
    .sort(order);
}

function state(): Promise<PageInfo['state']> {
  return session.vaultState();
}

// ── Which document asks ───────────────────────────────────

/** A frame's document, checked: its origin is its address's, and not opaque. */
export type CheckedFrame = FrameDocument & { url: string };

function strings(value: unknown): string[] | null {
  return Array.isArray(value) && value.every((v) => typeof v === 'string')
    ? (value as string[])
    : null;
}

/**
 * The document behind a sender, or `null` when it gets nothing: no address, an opaque origin
 * (a sandboxed document), or an origin that isn't its address's. Chromium names the document's
 * origin in `sender.origin` (`"null"` when opaque); in every browser the frame's content script
 * reports `window.origin` and its ancestors' origins (content/frame.ts), asked by `documentId`
 * where the browser has it, so the answer is from that very document.
 */
export async function frameDocument(sender: Sender): Promise<CheckedFrame | null> {
  const url = sender.url;
  const tabId = sender.tab?.id;
  if (!url || tabId === undefined || sender.frameId === undefined) return null;
  let origin: string;
  try {
    origin = new URL(url).origin;
  } catch {
    return null;
  }
  if (origin === 'null') return null;
  if (sender.origin !== undefined && sender.origin !== origin) return null;
  // Chromium: that very document (verified on Chromium); Firefox: by frame.
  const target =
    !isFirefox && typeof sender.documentId === 'string'
      ? { documentId: sender.documentId }
      : { frameId: sender.frameId };
  const reply = (await ext.tabs
    .sendMessage(tabId, { type: FRAME_DOCUMENT }, target)
    .catch(() => null)) as Partial<FrameDocument> | null | undefined;
  if (!reply || typeof reply !== 'object' || reply.origin !== origin) return null;
  return {
    origin,
    ancestors: strings(reply.ancestors),
    parents: strings(reply.parents),
    url,
  };
}

/**
 * The frame is the page itself, or of the same origin as the page and every frame in between
 * (A in B in A is not). `walked`: on browsers without `location.ancestorOrigins` (Firefox), the
 * origins read up the `window.parent` chain count too for sub-frames; the page itself needs no
 * ancestors either way.
 */
export function sameOriginAncestors(
  sender: Sender,
  frame: CheckedFrame,
  { walked }: { walked: boolean },
): boolean {
  // The page itself: `window.parent === window`, so the walked chain is `[]` and reliable even
  // without `location.ancestorOrigins` (Firefox).
  if (sender.frameId === 0) {
    const top = frame.ancestors ?? frame.parents;
    return top !== null && top.length === 0;
  }
  const chain = frame.ancestors ?? (walked ? frame.parents : null);
  if (!chain) return false;
  if (chain.length === 0 || !chain.every((origin) => origin === frame.origin)) return false;
  // Where the browser tells the tab's address, it agrees.
  const top = sender.tab?.url;
  if (top) {
    try {
      if (new URL(top).origin !== frame.origin) return false;
    } catch {
      return false;
    }
  }
  return true;
}

/** The frame is the page itself, or from the same origin as the page and all frames between. */
export async function sameOriginAsTop(
  sender: Sender,
  known?: CheckedFrame | null,
): Promise<boolean> {
  const frame = known === undefined ? await frameDocument(sender) : known;
  return frame !== null && sameOriginAncestors(sender, frame, { walked: true });
}

/**
 * The items a frame's menu may list, for a field of `kind`: the logins matching the frame's own
 * address, or — only in the page itself and frames of its origin — cards and addresses. Nothing
 * for a document whose origin isn't its address's (`known`: already asked).
 */
export async function frameItems(
  sender: Sender,
  kind: MenuKind,
  known?: CheckedFrame | null,
): Promise<PageItem[]> {
  if (!session.unlockedAccountId()) return [];
  const frame = known === undefined ? await frameDocument(sender) : known;
  if (!frame) return [];
  if (kind === 'login') {
    return (await matchingLogins(frame.url, { topFrame: sender.frameId === 0 })).map(pageItem);
  }
  if ((kind === 'card' || kind === 'identity') && (await sameOriginAsTop(sender, frame))) {
    return ofKind(kind).map(pageItem);
  }
  return [];
}

/** What a content script may know about its frame: how many items fit, not which. */
export async function pageInfo(sender: Sender): Promise<PageInfo> {
  const url = sender.url ?? '';
  const tabId = sender.tab?.id;
  if (tabId !== undefined && sender.frameId === 0 && url) {
    tabUrls.set(tabId, url);
    void onTopFrame(tabId);
  }
  const config = await settings();
  const unlocked = Boolean(session.unlockedAccountId());
  // Asked once for the three counts.
  const frame = unlocked ? await frameDocument(sender) : null;
  return {
    state: await state(),
    counts: {
      logins: (await frameItems(sender, 'login', frame)).length,
      cards: (await frameItems(sender, 'card', frame)).length,
      identities: (await frameItems(sender, 'identity', frame)).length,
    },
    insecure: isInsecureUrl(url),
    inlineMenu: config.inlineMenu && isFillableUrl(url),
    savePrompt: config.savePrompt && isFillableUrl(url),
    language: resolveLanguage(config.language),
    uwuFeatures: unlocked ? ((await activeAccount())?.uwu?.features ?? []) : [],
  };
}

let topFrameListener: (tabId: number) => void | Promise<void> = () => undefined;

/** The menus and the badge follow what the top frame of a tab is. */
export function onTopFrameChange(listener: (tabId: number) => void | Promise<void>) {
  topFrameListener = listener;
}

async function onTopFrame(tabId: number) {
  await topFrameListener(tabId);
}

// ── Filling ───────────────────────────────────────────────

type Offer = {
  tabId: number;
  itemId: string;
  created: number;
  /** Picked in the popup for this tab: the top frame may have it even if it doesn't match. */
  explicit: boolean;
  /** The popup asked about plain http already. */
  insecureOk: boolean;
  /**
   * The master password was asked for this very fill (an item with the re-prompt): good for one
   * fill, as in Bitwarden, which asks every time.
   */
  reprompted: boolean; /**
   * Picked in a frame's inline menu: only that frame may claim it, once. (Offers from the popup
   * go to every frame of the tab, each judged on its own address.)
   */
  frameId?: number;
};

const OFFER_LIFETIME = 15_000;
const offers = new Map<string, Offer>();
/** The last fill per tab, for copying its one-time code afterwards. */
const lastFill = new Map<number, { itemId: string; at: number }>();

function token(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

/** Ask every frame of a tab to claim an item's values; each is judged on its own address. */
export async function offer(
  tabId: number,
  itemId: string,
  explicit: boolean,
  insecureOk = false,
  reprompted = false,
  menu?: { frameId: number; session: string },
) {
  const entry = vault.autofillIndex().find((e) => e.id === itemId);
  if (!entry) throw { kind: 'not-found', message: "This item isn't in the vault any more." };
  for (const [key, old] of offers)
    if (Date.now() - old.created > OFFER_LIFETIME) offers.delete(key);
  const id = token();
  offers.set(id, {
    tabId,
    itemId,
    created: Date.now(),
    explicit,
    insecureOk,
    reprompted,
    ...(menu ? { frameId: menu.frameId } : {}),
  });
  const message: BackgroundMessage = {
    type: 'bg:fill-offer',
    token: id,
    itemId,
    kind: entry.kind,
    ...(menu ? { session: menu.session } : {}),
  };
  const options = menu ? { frameId: menu.frameId } : undefined;
  await (
    options ? ext.tabs.sendMessage(tabId, message, options) : ext.tabs.sendMessage(tabId, message)
  ).catch(() => undefined);
}

export async function fill(
  sender: Sender,
  itemId: string,
  offerToken: unknown,
  confirmedInsecure: boolean,
): Promise<FillAnswer> {
  const refuse = (reason: Extract<FillAnswer, { filled: false }>['reason']): FillAnswer => ({
    filled: false,
    reason,
  });
  if (!session.unlockedAccountId()) return refuse('locked');
  const url = sender.url ?? '';
  const tabId = sender.tab?.id;
  if (tabId === undefined || !isFillableUrl(url)) return refuse('refused');
  const entry = vault.autofillIndex().find((e) => e.id === itemId);
  if (!entry) return refuse('not-found');

  // Nothing without an offer: a pick the background saw, never one the page's side reports.
  if (typeof offerToken !== 'string') return refuse('expired');
  const fromOffer = offers.get(offerToken);
  if (!fromOffer || Date.now() - fromOffer.created > OFFER_LIFETIME) return refuse('expired');
  if (fromOffer.tabId !== tabId || fromOffer.itemId !== itemId) return refuse('expired');
  if (fromOffer.frameId !== undefined && fromOffer.frameId !== sender.frameId) {
    return refuse('expired');
  }

  // A sandboxed document, or one whose origin isn't its address's, gets nothing (R4-2).
  const frame = await frameDocument(sender);
  if (!frame) return refuse('refused');

  if (entry.kind === 'login') {
    const matches = await loginMatches(entry, url, { topFrame: sender.frameId === 0 });
    // An item picked for a page it doesn't match goes to that page itself, not into its frames.
    if (!matches && !(fromOffer.explicit && sender.frameId === 0)) return refuse('no-match');
  } else if (!(await sameOriginAsTop(sender, frame))) {
    // Cards and addresses have no address to match: only into the page itself, or a frame of
    // its own origin — from the popup or from the page's own menu.
    return refuse('no-match');
  }

  if (isInsecureUrl(url) && !confirmedInsecure && !fromOffer.insecureOk) return refuse('insecure');

  // An item with the re-prompt is filled only right after the master password was asked for
  // this fill — never on the strength of an earlier answer (CL-I3).
  if (entry.reprompt && !fromOffer.reprompted) return refuse('reprompt');

  let values: FillValues;
  try {
    values = await callJson<FillValues>((core) => core.fillValues(itemId, Date.now() / 1000));
  } catch (error) {
    const kind = (error as { kind?: string }).kind;
    return refuse(kind === 'reprompt' ? 'reprompt' : 'not-found');
  }
  // A menu's pick and an answered re-prompt are good for one fill.
  if (fromOffer.reprompted || fromOffer.frameId !== undefined) offers.delete(offerToken);
  lastUsed.set(itemId, Date.now());
  lastFill.set(tabId, { itemId, at: Date.now() });
  await session.touch();
  return { filled: true, values };
}

/** The page had no field for the one-time code: copy it, if that item was just filled here. */
export async function copyTotp(sender: Sender, itemId: string) {
  const tabId = sender.tab?.id;
  if (tabId === undefined) return;
  const last = lastFill.get(tabId);
  if (!last || last.itemId !== itemId || Date.now() - last.at > 30_000) return;
  if (!(await settings()).copyTotp) return;
  const code = await vault.reveal(itemId, 'totp').catch(() => null);
  if (code) await clipboard.copy(code);
}

/** The shortcut, or the context menu without a pick: the best match of the tab. */
export async function fillBest(tab: chrome.tabs.Tab) {
  if (tab.id === undefined) return;
  if (!session.unlockedAccountId()) {
    await session.openPopup();
    return;
  }
  const url = tab.url ?? tabUrls.get(tab.id);
  if (!url) return;
  // Nobody picked an item: only a match by the page's own domain or Bitwarden's global list.
  const best = (await matchingLogins(url, { topFrame: true, strict: true }))[0];
  // An item with the re-prompt asks for the master password in the popup first.
  if (best?.reprompt) await session.openPopup();
  else if (best) await offer(tab.id, best.id, false);
}

// ── The popup's view of the tab ───────────────────────────

async function activeTab(): Promise<chrome.tabs.Tab | undefined> {
  const [tab] = await ext.tabs.query({ active: true, lastFocusedWindow: true });
  if (tab && !tab.url?.startsWith(ext.runtime.getURL('/'))) return tab;
  // The popup opened as a window of its own: the tab it was opened for is in another window.
  const [normal] = await ext.tabs.query({ active: true, windowType: 'normal' });
  return normal;
}

/** The address of the tab the popup was opened for, if it is a web page. */
export async function activeTabUrl(): Promise<string | null> {
  const tab = await activeTab();
  const url = (tab?.id !== undefined ? (tab.url ?? tabUrls.get(tab.id)) : undefined) ?? null;
  return isFillableUrl(url) ? url : null;
}

/** The address of the tab a content script runs in — its top frame's, not the frame's own. */
export function senderTabUrl(sender: Sender): string | null {
  const tabId = sender.tab?.id;
  const url =
    sender.tab?.url ??
    (tabId !== undefined ? tabUrls.get(tabId) : undefined) ??
    (sender.frameId === 0 ? sender.url : undefined) ??
    null;
  return isFillableUrl(url) ? url : null;
}

export async function tabItems(): Promise<TabItems> {
  const tab = await activeTab();
  const url = (tab?.id !== undefined ? (tab.url ?? tabUrls.get(tab.id)) : undefined) ?? null;
  const fillable = isFillableUrl(url);
  const summaries = session.unlockedAccountId() ? await vault.items() : [];
  const byId = new Map(summaries.map((s) => [s.id, s]));
  const pick = (entries: vault.IndexEntry[]) =>
    entries.map((e) => byId.get(e.id)).filter((s): s is ItemSummary => Boolean(s));
  return {
    url,
    host: url ? hostnameOf(url) : null,
    insecure: url ? isInsecureUrl(url) : false,
    fillable,
    logins: url && fillable ? pick(await matchingLogins(url, { topFrame: true })) : [],
    cards: pick(ofKind('card')),
    identities: pick(ofKind('identity')),
  };
}

/**
 * A pick in the popup. An item with the re-prompt needs the master password with every fill
 * (`password`), checked here before the page is offered anything.
 */
export async function fillTab(
  itemId: string,
  confirmedInsecure: boolean,
  password: string | undefined,
) {
  const tab = await activeTab();
  if (tab?.id === undefined) throw { kind: 'no-tab', message: 'No page to fill.' };
  const entry = vault.autofillIndex().find((e) => e.id === itemId);
  if (!entry) throw { kind: 'not-found', message: "This item isn't in the vault any more." };
  let reprompted = false;
  if (entry.reprompt) {
    if (typeof password !== 'string' || !password) {
      throw { kind: 'verify', message: 'This item asks for the master password.' };
    }
    await vault.verifyReprompt(itemId, password);
    reprompted = true;
  }
  await offer(tab.id, itemId, true, confirmedInsecure, reprompted);
}

// ── Saving what was sent ──────────────────────────────────

type Sent = {
  id: string;
  tabId: number;
  /** The frame the form was sent in (older entries have none: not the top frame). */
  frameId?: number;
  /**
   * The account it is for: the one open when it was sent, or while locked the one that was to
   * be unlocked. It is saved into that account only; entries without one are dropped.
   */
  accountId?: string | null;
  url: string;
  host: string;
  username: string | null;
  password: string;
  /** For a change of password: the one it was. */
  previous: string | null;
  at: number;
};

type Prompted = Sent & {
  action: 'save' | 'update';
  itemId: string | null;
  itemName: string | null;
};

const prompts = new Map<string, Prompted>();
const seenUsernames = new Map<number, { username: string; host: string; at: number }>();
const PROMPT_LIFETIME = 5 * 60_000;

function forgetOld() {
  const now = Date.now();
  for (const [id, prompt] of prompts) if (now - prompt.at > PROMPT_LIFETIME) prompts.delete(id);
  for (const [tab, seen] of seenUsernames)
    if (now - seen.at > PROMPT_LIFETIME) seenUsernames.delete(tab);
}

/**
 * What to offer for a sent login in the vault open now: nothing (it's known), an update, or a
 * new item. The question is then for the open account.
 */
async function decide(sent: Sent): Promise<Prompted | null> {
  const open = session.unlockedAccountId();
  if (!open) return null;
  return decideIn({ ...sent, accountId: open });
}

async function decideIn(sent: Sent): Promise<Prompted | null> {
  const candidates = await matchingLogins(sent.url, { topFrame: sent.frameId === 0 });
  const wanted = sent.username?.trim().toLowerCase() ?? '';
  for (const entry of candidates) {
    let values: FillValues;
    try {
      values = await callJson<FillValues>((core) => core.fillValues(entry.id, Date.now() / 1000));
    } catch {
      continue;
    }
    if (values.kind !== 'login') continue;
    const sameUser = (values.username?.trim().toLowerCase() ?? '') === wanted;
    if (sent.previous !== null) {
      // A change of password: the item is the one with the old password.
      if (values.password === sent.previous) {
        return { ...sent, action: 'update', itemId: entry.id, itemName: entry.name };
      }
      if (sameUser && values.password === sent.password) return null;
      continue;
    }
    if (!sameUser) continue;
    if (values.password === sent.password) return null;
    return { ...sent, action: 'update', itemId: entry.id, itemName: entry.name };
  }
  return { ...sent, action: 'save', itemId: null, itemName: null };
}

function promptOf(prompted: Prompted): SavePrompt {
  return {
    id: prompted.id,
    action: prompted.action,
    host: prompted.host,
    username: prompted.username,
    itemName: prompted.itemName,
  };
}

export async function submitted(
  sender: Sender,
  username: string | null,
  password: string | null,
  newPassword: string | null,
) {
  forgetOld();
  const url = sender.url ?? '';
  const tabId = sender.tab?.id;
  const config = await settings();
  if (tabId === undefined || !isFillableUrl(url) || !config.savePrompt) return;
  const host = hostnameOf(url);
  if (!host || config.neverSave.includes(host)) return;
  // Nothing typed into a sandboxed document is the site's login.
  if (!(await frameDocument(sender))) return;
  const typed = username?.trim() || null;
  if (!password && !newPassword) {
    // The first step of a login in two: remember who, for the password on the next page.
    if (typed) seenUsernames.set(tabId, { username: typed, host, at: Date.now() });
    return;
  }
  const seen = seenUsernames.get(tabId);
  const open = session.unlockedAccountId();
  const accountId = open ?? (await activeAccount())?.id ?? null;
  if (!accountId) return;
  const sent: Sent = {
    accountId,
    id: token(),
    tabId,
    frameId: sender.frameId,
    url,
    host,
    username: typed ?? (seen && seen.host === host ? seen.username : null),
    password: (newPassword ?? password)!,
    previous: newPassword ? password : null,
    at: Date.now(),
  };
  if (!open) {
    // Locked: kept in memory until this account's vault is unlocked, then the popup asks.
    const pending = ((await sessionStore('pendingSaves')) ?? []) as Sent[];
    const next = [
      ...pending.filter(
        (p) => !(p.host === host && p.username === sent.username && p.accountId === accountId),
      ),
      sent,
    ];
    await setSession('pendingSaves', next.slice(-10));
    changed();
    return;
  }
  const prompted = await decide(sent);
  if (!prompted) return;
  // One prompt per tab: a newer login replaces what wasn't answered yet.
  for (const [id, old] of prompts) if (old.tabId === tabId) prompts.delete(id);
  prompts.set(prompted.id, prompted);
  const message: BackgroundMessage = { type: 'bg:save-prompt', prompt: promptOf(prompted) };
  // A page that stays (a single-page app) shows it now; one that navigates asks when it loaded.
  await ext.tabs.sendMessage(tabId, message, { frameId: 0 }).catch(() => undefined);
}

/** A page loaded: the prompt for its tab, if one waits. */
export function pendingPrompt(sender: Sender): SavePrompt | null {
  forgetOld();
  if (sender.frameId !== 0 || sender.tab?.id === undefined) return null;
  for (const prompted of prompts.values()) {
    if (prompted.tabId === sender.tab.id) return promptOf(prompted);
  }
  return null;
}

/** The question was asked for another account than the one open now. */
function otherAccount(prompted: Prompted): boolean {
  return !prompted.accountId || prompted.accountId !== session.unlockedAccountId();
}

async function act(sent: Prompted, answer: SaveAnswer): Promise<void> {
  if ((answer === 'save' || answer === 'update') && otherAccount(sent)) {
    throw { kind: 'account-changed', message: 'Another account is open now. Nothing was saved.' };
  }
  if (answer === 'never') {
    const config = await settings();
    await updateSettings({ neverSave: [...new Set([...config.neverSave, sent.host])] });
    return;
  }
  if (answer === 'dismiss') return;
  if (answer === 'update' && sent.itemId) {
    await vault.savePassword(sent.itemId, sent.password);
    return;
  }
  const origin = new URL(sent.url).origin;
  await vault.saveItem(null, {
    kind: 'login',
    name: sent.host,
    notes: null,
    favorite: false,
    reprompt: false,
    folderId: null,
    login: {
      username: sent.username ?? '',
      password: sent.password,
      totp: null,
      uris: [{ uri: origin, match: null }],
    },
    fields: [],
  });
}

/**
 * The bar's answer. `null` when it was acted on; a new question when another account was opened
 * since it was asked — the bar asks again, for the account open now, instead of saving into it.
 */
export async function promptAnswer(
  sender: Sender,
  id: string,
  answer: SaveAnswer,
): Promise<SavePrompt | null> {
  const prompted = prompts.get(id);
  if (!prompted || prompted.tabId !== sender.tab?.id) {
    throw { kind: 'expired', message: 'This question is out of date.' };
  }
  prompts.delete(id);
  if ((answer === 'save' || answer === 'update') && otherAccount(prompted)) {
    if (!session.unlockedAccountId()) throw { kind: 'locked', message: 'The vault is locked.' };
    const again = await decide(prompted);
    if (!again) return null;
    const next = { ...again, id: token(), at: Date.now() };
    prompts.set(next.id, next);
    return promptOf(next);
  }
  await act(prompted, answer === 'update' && !prompted.itemId ? 'save' : answer);
  return null;
}

/**
 * After unlocking: the logins sent while locked for the account open now, each decided now.
 * Those for another account wait until it is open; those for none are dropped.
 */
export async function pendingSaves(): Promise<PendingSave[]> {
  const open = (await session.requireUnlocked()).id;
  const pending = ((await sessionStore('pendingSaves')) ?? []) as Sent[];
  const out: PendingSave[] = [];
  const keep: Sent[] = [];
  for (const sent of pending) {
    if (!sent.accountId) continue;
    if (sent.accountId !== open) {
      keep.push(sent);
      continue;
    }
    const prompted = await decideIn(sent);
    if (!prompted) continue;
    keep.push(sent);
    prompts.set(prompted.id, { ...prompted, at: Date.now() });
    out.push({
      id: prompted.id,
      host: prompted.host,
      username: prompted.username,
      action: prompted.action,
      itemName: prompted.itemName,
    });
  }
  await setSession('pendingSaves', keep);
  return out;
}

export async function answerPendingSave(id: string, answer: SaveAnswer) {
  const open = (await session.requireUnlocked()).id;
  const pending = ((await sessionStore('pendingSaves')) ?? []) as Sent[];
  const prompted = prompts.get(id);
  const sent = pending.find((p) => p.id === id);
  // Asked for another account than the one open now: it stays for that one.
  if (sent && sent.accountId !== open) {
    prompts.delete(id);
    changed();
    throw { kind: 'account-changed', message: 'Another account is open now. Nothing was saved.' };
  }
  await setSession(
    'pendingSaves',
    pending.filter((p) => p.id !== id),
  );
  prompts.delete(id);
  changed();
  if (!prompted || !sent) throw { kind: 'expired', message: 'This question is out of date.' };
  await act(prompted, answer === 'update' && !prompted.itemId ? 'save' : answer);
}
