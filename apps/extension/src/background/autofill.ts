/**
 * Filling pages, and saving what was typed into them.
 *
 * A content script learns names, never values, until somebody picked an item: in the page's
 * inline menu, in the popup, with the shortcut or the context menu. Which page asks is taken
 * from the sender — the frame's own address — and an item is only handed to a frame whose
 * address matches one of its addresses (Bitwarden's match detection, see shared/uri.ts). So a
 * login for `bank.example` never reaches an iframe from `ads.example` on the bank's page. Only
 * when somebody explicitly picks an item in the popup for a page it doesn't match, the top
 * frame may have it — never an iframe. Plain http pages ask first.
 *
 * Cards and addresses have no address to match. They are listed to, and filled into, only the
 * page itself and frames from the same origin as it — never an ad's or another site's frame,
 * whatever that frame asks — and only after somebody picked one, in the page's menu or the popup.
 *
 * Sent logins are kept in the background's memory, and offered to save in the page's
 * notification bar — or, while the vault is locked, in `storage.session` until it is unlocked.
 */

import { ext } from '../shared/browser';
import type {
  FillAnswer,
  FillValues,
  ItemKind,
  ItemSummary,
  PageInfo,
  PageItem,
  PendingSave,
  SaveAnswer,
  SavePrompt,
  TabItems,
  BackgroundMessage,
} from '../shared/protocol';
import { resolveLanguage } from '../shared/i18n';
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

/** The frame is the page itself, or from the same origin as the page's top frame. */
export function sameOriginAsTop(sender: Sender): boolean {
  if (sender.frameId === 0) return true;
  const tabId = sender.tab?.id;
  const top = (tabId !== undefined ? tabUrls.get(tabId) : undefined) ?? sender.tab?.url;
  if (!top || !sender.url) return false;
  try {
    const origin = new URL(sender.url).origin;
    return origin !== 'null' && origin === new URL(top).origin;
  } catch {
    return false;
  }
}

/** What a content script may know about its frame. */
export async function pageInfo(sender: Sender): Promise<PageInfo> {
  const url = sender.url ?? '';
  const tabId = sender.tab?.id;
  if (tabId !== undefined && sender.frameId === 0 && url) {
    tabUrls.set(tabId, url);
    void onTopFrame(tabId);
  }
  const config = await settings();
  const unlocked = Boolean(session.unlockedAccountId());
  const own = unlocked && sameOriginAsTop(sender);
  return {
    state: await state(),
    logins: unlocked
      ? (await matchingLogins(url, { topFrame: sender.frameId === 0 })).map(pageItem)
      : [],
    cards: own ? ofKind('card').map(pageItem) : [],
    identities: own ? ofKind('identity').map(pageItem) : [],
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
export async function offer(tabId: number, itemId: string, explicit: boolean, insecureOk = false) {
  const entry = vault.autofillIndex().find((e) => e.id === itemId);
  if (!entry) throw { kind: 'not-found', message: "This item isn't in the vault any more." };
  for (const [key, old] of offers)
    if (Date.now() - old.created > OFFER_LIFETIME) offers.delete(key);
  const id = token();
  offers.set(id, { tabId, itemId, created: Date.now(), explicit, insecureOk });
  const message: BackgroundMessage = { type: 'bg:fill-offer', token: id, itemId, kind: entry.kind };
  await ext.tabs.sendMessage(tabId, message).catch(() => undefined);
}

export async function fill(
  sender: Sender,
  itemId: string,
  offerToken: string | undefined,
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

  let fromOffer: Offer | null = null;
  if (offerToken !== undefined) {
    const found = offers.get(offerToken);
    if (!found || Date.now() - found.created > OFFER_LIFETIME) return refuse('expired');
    if (found.tabId !== tabId || found.itemId !== itemId) return refuse('expired');
    fromOffer = found;
  }

  if (entry.kind === 'login') {
    const matches = await loginMatches(entry, url, { topFrame: sender.frameId === 0 });
    // An item picked for a page it doesn't match goes to that page itself, not into its frames.
    if (!matches && !(fromOffer?.explicit && sender.frameId === 0)) return refuse('no-match');
  } else if (!sameOriginAsTop(sender)) {
    // Cards and addresses have no address to match: only into the page itself, or a frame of
    // its own origin — with an offer from the popup or without, from the page's own menu.
    return refuse('no-match');
  }

  if (isInsecureUrl(url) && !confirmedInsecure && !fromOffer?.insecureOk) return refuse('insecure');

  let values: FillValues;
  try {
    values = await callJson<FillValues>((core) => core.fillValues(itemId, Date.now() / 1000));
  } catch (error) {
    const kind = (error as { kind?: string }).kind;
    return refuse(kind === 'reprompt' ? 'reprompt' : 'not-found');
  }
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
  if (best) await offer(tab.id, best.id, false);
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

export async function fillTab(itemId: string, confirmedInsecure: boolean) {
  const tab = await activeTab();
  if (tab?.id === undefined) throw { kind: 'no-tab', message: 'No page to fill.' };
  await offer(tab.id, itemId, true, confirmedInsecure);
}

// ── Saving what was sent ──────────────────────────────────

type Sent = {
  id: string;
  tabId: number;
  /** The frame the form was sent in (older entries have none: not the top frame). */
  frameId?: number;
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

/** What to offer for a sent login: nothing (it's known), an update, or a new item. */
async function decide(sent: Sent): Promise<Prompted | null> {
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
  const typed = username?.trim() || null;
  if (!password && !newPassword) {
    // The first step of a login in two: remember who, for the password on the next page.
    if (typed) seenUsernames.set(tabId, { username: typed, host, at: Date.now() });
    return;
  }
  const seen = seenUsernames.get(tabId);
  const sent: Sent = {
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
  if (!session.unlockedAccountId()) {
    // Locked: kept in memory until the vault is unlocked, then the popup asks.
    const pending = ((await sessionStore('pendingSaves')) ?? []) as Sent[];
    const next = [
      ...pending.filter((p) => !(p.host === host && p.username === sent.username)),
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

async function act(sent: Prompted, answer: SaveAnswer): Promise<void> {
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

export async function promptAnswer(sender: Sender, id: string, answer: SaveAnswer) {
  const prompted = prompts.get(id);
  if (!prompted || prompted.tabId !== sender.tab?.id) {
    throw { kind: 'expired', message: 'This question is out of date.' };
  }
  prompts.delete(id);
  await act(prompted, answer === 'update' && !prompted.itemId ? 'save' : answer);
}

/** After unlocking: the logins sent while locked, each decided now. */
export async function pendingSaves(): Promise<PendingSave[]> {
  await session.requireUnlocked();
  const pending = ((await sessionStore('pendingSaves')) ?? []) as Sent[];
  const out: PendingSave[] = [];
  const keep: Sent[] = [];
  for (const sent of pending) {
    const prompted = await decide(sent);
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
  await session.requireUnlocked();
  const pending = ((await sessionStore('pendingSaves')) ?? []) as Sent[];
  const prompted = prompts.get(id);
  const sent = pending.find((p) => p.id === id);
  await setSession(
    'pendingSaves',
    pending.filter((p) => p.id !== id),
  );
  prompts.delete(id);
  changed();
  if (!prompted || !sent) throw { kind: 'expired', message: 'This question is out of date.' };
  await act(prompted, answer === 'update' && !prompted.itemId ? 'save' : answer);
}
