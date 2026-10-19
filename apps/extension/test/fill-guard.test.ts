// @vitest-environment node
/**
 * Which frame gets an item's values (background/autofill.ts): only a frame whose own address
 * matches the item; a pick in the popup reaches the page itself but never its iframes; cards
 * and addresses only the page and frames of its origin; an offer is
 * only good for its tab, its item and a few seconds.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

const sent: { tabId: number; message: { token?: string }; options?: { frameId?: number } }[] = [];
/** What each document's content script says about it (content/frame.ts), by `documentId`. */
const documents = new Map<
  string,
  { origin: string; ancestors: string[] | null; parents: string[] }
>();

vi.mock('../src/shared/browser', () => ({
  isFirefox: false,
  ext: {
    tabs: {
      onRemoved: { addListener: () => undefined },
      sendMessage: async (
        tabId: number,
        message: { token?: string; type?: string },
        options?: { frameId?: number; documentId?: string },
      ) => {
        if (message.type === 'bg:frame-document') return documents.get(options?.documentId ?? '');
        sent.push({ tabId, message, ...(options ? { options } : {}) });
      },
      query: async () => [{ id: 7, url: 'https://bank.example/login' }],
    },
    runtime: { getURL: (path: string) => `chrome-extension://test${path}` },
  },
}));

vi.mock('../src/background/session', () => ({
  unlockedAccountId: () => 'user-1',
  touch: async () => undefined,
  openPopup: async () => undefined,
  vaultState: async () => 'unlocked',
  requireUnlocked: async () => ({ id: 'user-1' }),
}));

vi.mock('../src/background/settings', () => ({
  settings: async () => ({
    defaultMatch: 0,
    inlineMenu: true,
    savePrompt: true,
    copyTotp: true,
    neverSave: [],
    language: 'en',
  }),
  updateSettings: async () => undefined,
}));

vi.mock('../src/background/store', () => ({
  activeAccount: async () => ({ id: 'user-1', uwu: null }),
  session: async () => undefined,
  setSession: async () => undefined,
}));

vi.mock('../src/background/clipboard', () => ({ copy: async () => undefined }));
vi.mock('../src/background/events', () => ({ changed: () => undefined }));

/** The account's own equivalent domains, as the server sends them. */
let equivalents: { global: string[][]; custom: string[][] } = { global: [], custom: [] };

/** How often the vault closed, as `vault.closedCount` says. */
let closings = 0;

vi.mock('../src/background/vault', () => ({
  closedCount: () => closings,
  autofillIndex: () => [
    {
      id: 'bank',
      kind: 'login',
      name: 'Bank',
      subtitle: 'nyu',
      favorite: false,
      reprompt: false,
      hasTotp: false,
      hasPassword: true,
      hasUsername: true,
      uris: [{ uri: 'https://bank.example', match: null }],
    },
    {
      id: 'guarded',
      kind: 'login',
      name: 'Guarded',
      subtitle: 'nyu',
      favorite: false,
      reprompt: true,
      hasTotp: false,
      hasPassword: true,
      hasUsername: true,
      uris: [{ uri: 'https://vault.example.net', match: null }],
    },
    {
      id: 'intranet',
      kind: 'login',
      name: 'Intranet',
      subtitle: 'nyu',
      favorite: false,
      reprompt: false,
      hasTotp: false,
      hasPassword: true,
      hasUsername: true,
      uris: [{ uri: '^https://intra\\.example\\.org/', match: 4 }],
    },
    {
      id: 'card',
      kind: 'card',
      name: 'Visa',
      subtitle: null,
      favorite: false,
      reprompt: false,
      hasTotp: false,
      hasPassword: false,
      hasUsername: false,
    },
  ],
  domains: () => equivalents,
  verifyReprompt: async (_id: string, password: string) => {
    if (password !== 'master') throw { kind: 'wrong-password', message: 'Wrong.' };
  },
  items: async () => [],
}));

vi.mock('../src/background/wasm', () => ({
  callJson: async () => ({ kind: 'login', username: 'nyu', password: 'hunter2', totp: null }),
  call: async () => undefined,
}));

const {
  fill,
  fillBest,
  fillTab,
  frameItems,
  nextStep,
  offer,
  pageInfo,
  submitted,
  usernameStepFilled,
} = await import('../src/background/autofill');

type Sender = chrome.runtime.MessageSender;
let documentCount = 0;
/**
 * A frame's sender, its document as its content script describes it: by default of its
 * address's origin, and a frame's ancestors all of the page's origin, bank.example.
 */
const frame = (
  url: string,
  frameId = 0,
  tabId = 7,
  doc: { origin?: string; ancestors?: string[] | null; senderOrigin?: string } = {},
): Sender => {
  const documentId = `doc-${++documentCount}`;
  const origin = doc.origin ?? new URL(url).origin;
  const ancestors =
    doc.ancestors !== undefined ? doc.ancestors : frameId === 0 ? [] : ['https://bank.example'];
  documents.set(documentId, { origin, ancestors, parents: ancestors ?? [] });
  return {
    id: 'test',
    url,
    frameId,
    documentId,
    ...(doc.senderOrigin !== undefined ? { origin: doc.senderOrigin } : {}),
    tab: { id: tabId } as chrome.tabs.Tab,
  };
};

async function offered(tabId: number, itemId: string, explicit: boolean) {
  sent.length = 0;
  await offer(tabId, itemId, { explicit });
  return sent[0]!.message.token!;
}

/** A pick in the inline menu's frame of `frameId`: an offer for that frame alone. */
async function picked(frameId: number, itemId: string, tabId = 7) {
  sent.length = 0;
  await offer(tabId, itemId, { menu: { frameId, session: 'menu-1' } });
  return sent[0]!.message.token!;
}

const ids = (items: { id: string }[]) => items.map((item) => item.id);

describe('filling', () => {
  beforeEach(async () => {
    // The tab's top frame, as its content script reported it.
    await pageInfo(frame('https://bank.example/login'));
  });

  it('fills nothing without an offer: a pick the page reports is no pick', async () => {
    const page = frame('https://bank.example/login');
    for (const token of [undefined, 'made-up', 42]) {
      expect(await fill(page, 'bank', token)).toEqual({ filled: false, reason: 'expired' });
    }
  });

  it("gives a login to a frame whose own address matches, from that frame's menu", async () => {
    const login = frame('https://login.bank.example/', 2);
    const token = await picked(2, 'bank');
    expect(sent[0]).toMatchObject({ options: { frameId: 2 } });
    expect(await fill(login, 'bank', token)).toEqual({
      filled: true,
      values: expect.objectContaining({ password: 'hunter2' }),
    });
    // Once.
    expect(await fill(login, 'bank', token)).toEqual({ filled: false, reason: 'expired' });
  });

  it("a menu's pick is for its own frame only", async () => {
    const token = await picked(2, 'bank');
    expect(await fill(frame('https://bank.example/', 0), 'bank', token)).toEqual({
      filled: false,
      reason: 'expired',
    });
  });

  it('never to a frame of another site, not even inside the bank’s page', async () => {
    const ad = frame('https://ads.example/frame', 3);
    expect(await fill(ad, 'bank', await picked(3, 'bank'))).toEqual({
      filled: false,
      reason: 'no-match',
    });
    const token = await offered(7, 'bank', true);
    expect(await fill(ad, 'bank', token)).toEqual({ filled: false, reason: 'no-match' });
  });

  it('a pick in the popup reaches the page itself, even where it doesn’t match', async () => {
    const other = frame('https://other.example/');
    const token = await offered(7, 'bank', true);
    expect((await fill(other, 'bank', token)).filled).toBe(true);
    // Without the pick (the shortcut), it doesn't.
    const best = await offered(7, 'bank', false);
    expect(await fill(other, 'bank', best)).toEqual({ filled: false, reason: 'no-match' });
  });

  it('an offer is for its tab and its item only', async () => {
    const token = await offered(7, 'bank', true);
    expect(await fill(frame('https://bank.example/', 0, 8), 'bank', token)).toEqual({
      filled: false,
      reason: 'expired',
    });
    expect(await fill(frame('https://bank.example/'), 'card', token)).toEqual({
      filled: false,
      reason: 'expired',
    });
  });

  it('fills plain http without asking: the menu and the popup warn instead', async () => {
    const plain = frame('http://bank.example/');
    expect((await fill(plain, 'bank', await picked(0, 'bank'))).filled).toBe(true);
    // The frame shows a notice afterwards, from what it is told here.
    expect((await pageInfo(plain)).insecure).toBe(true);
  });

  it('cards only into the page and frames of its own origin, offer or not', async () => {
    const token = await offered(7, 'card', true);
    const payment = frame('https://pay.example/card', 5);
    const refused = { filled: false, reason: 'no-match' };
    expect(await fill(payment, 'card', token)).toEqual(refused);
    // Not from a pick in that frame's own menu either.
    expect(await fill(payment, 'card', await picked(5, 'card'))).toEqual(refused);
    const own = frame('https://bank.example/checkout', 4);
    expect((await fill(own, 'card', await picked(4, 'card'))).filled).toBe(true);
    expect((await fill(frame('https://bank.example/'), 'card', token)).filled).toBe(true);
  });

  it('lists cards and addresses to the page and frames of its own origin only', async () => {
    expect(await frameItems(frame('https://ads.example/', 2), 'card')).toEqual([]);
    expect(ids(await frameItems(frame('https://bank.example/pay', 2), 'card'))).toEqual(['card']);
    expect(ids(await frameItems(frame('https://bank.example/'), 'card'))).toEqual(['card']);
  });

  it('the page learns how many items fit, never names or values', async () => {
    const info = await pageInfo(frame('https://ads.example/', 2));
    expect(info.counts).toEqual({ logins: 0, cards: 0, identities: 0 });
    const own = await pageInfo(frame('https://www.bank.example/'));
    expect(own.counts).toEqual({ logins: 1, cards: 1, identities: 0 });
    expect(JSON.stringify(own)).not.toContain('Bank');
    expect(ids(await frameItems(frame('https://www.bank.example/'), 'login'))).toEqual(['bank']);
  });
});

describe('logins in two steps', () => {
  beforeEach(async () => {
    equivalents = { global: [], custom: [] };
    vi.useRealTimers();
    await pageInfo(frame('https://bank.example/login'));
  });

  /** The username step was filled with `itemId` in tab 7, and the content script said so. */
  async function firstStep(itemId = 'bank', tabId = 7) {
    const page = frame('https://bank.example/login', 0, tabId);
    expect((await fill(page, itemId, await picked(0, itemId, tabId))).filled).toBe(true);
    usernameStepFilled(page, itemId);
  }

  it('offers the same item to the frame where the password field appears, once', async () => {
    await firstStep();
    sent.length = 0;
    const step = frame('https://bank.example/password', 0);
    await nextStep(step);
    expect(sent).toHaveLength(1);
    expect(sent[0]).toMatchObject({ options: { frameId: 0 }, message: { itemId: 'bank' } });
    expect((await fill(step, 'bank', sent[0]!.message.token)).filled).toBe(true);
    sent.length = 0;
    await nextStep(step);
    expect(sent).toEqual([]);
  });

  it('nothing without a fill of that item just before, in that tab', async () => {
    usernameStepFilled(frame('https://bank.example/login', 0, 9), 'bank');
    sent.length = 0;
    await nextStep(frame('https://bank.example/password', 0, 9));
    expect(sent).toEqual([]);
    await firstStep();
    sent.length = 0;
    await nextStep(frame('https://bank.example/password', 0, 8));
    expect(sent).toEqual([]);
  });

  it('nothing on a page the item does not match by itself', async () => {
    equivalents = { global: [], custom: [['bank.example', 'evil.example']] };
    await firstStep();
    sent.length = 0;
    await nextStep(frame('https://evil.example/password'));
    await nextStep(frame('https://ads.example/frame', 3));
    expect(sent).toEqual([]);
  });

  it('nothing in another frame, after another site, a sent password or a lock', async () => {
    await firstStep();
    sent.length = 0;
    await nextStep(frame('https://bank.example/password', 4));
    expect(sent).toEqual([]);
    await pageInfo(frame('https://other.example/'));
    await pageInfo(frame('https://bank.example/login'));
    await nextStep(frame('https://bank.example/password'));
    expect(sent).toEqual([]);
    await firstStep();
    await submitted(frame('https://bank.example/login'), 'me', 'secret', null);
    sent.length = 0;
    await nextStep(frame('https://bank.example/password'));
    expect(sent).toEqual([]);
    await firstStep();
    closings += 1;
    sent.length = 0;
    await nextStep(frame('https://bank.example/password'));
    expect(sent).toEqual([]);
  });

  it('nothing after a minute', async () => {
    await firstStep();
    vi.useFakeTimers({ now: Date.now() + 61_000, toFake: ['Date'] });
    sent.length = 0;
    await nextStep(frame('https://bank.example/password'));
    expect(sent).toEqual([]);
    vi.useRealTimers();
  });
});

describe('address matching (CL-L11)', () => {
  beforeEach(() => {
    equivalents = { global: [], custom: [] };
  });

  it('tries a regular expression in the top frame only', async () => {
    const top = frame('https://intra.example.org/login');
    expect(ids(await frameItems(top, 'login'))).toEqual(['intranet']);
    expect((await fill(top, 'intranet', await picked(0, 'intranet'))).filled).toBe(true);
    const inner = frame('https://intra.example.org/login', 2);
    expect(await frameItems(inner, 'login')).toEqual([]);
    expect(await fill(inner, 'intranet', await picked(2, 'intranet'))).toEqual({
      filled: false,
      reason: 'no-match',
    });
  });

  it("lists a login matched through the account's own equivalent domains, but the shortcut doesn't fill it", async () => {
    equivalents = { global: [], custom: [['bank.example', 'evil.example']] };
    const evil = frame('https://evil.example/login');
    expect(ids(await frameItems(evil, 'login'))).toEqual(['bank']);
    sent.length = 0;
    await fillBest({ id: 7, url: 'https://evil.example/login' } as chrome.tabs.Tab);
    expect(sent).toEqual([]);
    // Bitwarden's global groups count for the shortcut.
    equivalents = { global: [['bank.example', 'evil.example']], custom: [] };
    await fillBest({ id: 7, url: 'https://evil.example/login' } as chrome.tabs.Tab);
    expect(sent).toHaveLength(1);
  });
});

describe('the re-prompt, per fill (CL-I3)', () => {
  beforeEach(async () => {
    equivalents = { global: [], custom: [] };
    await pageInfo(frame('https://vault.example.net/login'));
  });

  it("never fills from the page's menu or an offer without the master password", async () => {
    const page = frame('https://vault.example.net/login');
    expect(await fill(page, 'guarded', await picked(0, 'guarded'))).toEqual({
      filled: false,
      reason: 'reprompt',
    });
    const token = await offered(7, 'guarded', true);
    expect(await fill(page, 'guarded', token)).toEqual({
      filled: false,
      reason: 'reprompt',
    });
  });

  it('fills once with the master password asked for that fill', async () => {
    await expect(fillTab('guarded', undefined)).rejects.toMatchObject({ kind: 'verify' });
    await expect(fillTab('guarded', 'wrong')).rejects.toMatchObject({
      kind: 'wrong-password',
    });
    sent.length = 0;
    await fillTab('guarded', 'master');
    const token = sent[0]!.message.token!;
    const page = frame('https://vault.example.net/login');
    expect((await fill(page, 'guarded', token)).filled).toBe(true);
    // The answer was for that fill: the same offer fills nothing more.
    expect((await fill(page, 'guarded', token)).filled).toBe(false);
  });

  it('the shortcut opens the popup instead', async () => {
    sent.length = 0;
    await fillBest({ id: 7, url: 'https://vault.example.net/login' } as chrome.tabs.Tab);
    expect(sent).toEqual([]);
  });
});

describe('the document behind the frame (R4-2)', () => {
  beforeEach(async () => {
    equivalents = { global: [], custom: [] };
    await pageInfo(frame('https://bank.example/login'));
  });

  const refused = { filled: false, reason: 'refused' };

  it('a sandboxed document on the site’s address gets nothing (Chromium: sender.origin "null")', async () => {
    const sandboxed = frame('https://bank.example/upload/1', 0, 7, { senderOrigin: 'null' });
    expect(await frameItems(sandboxed, 'login')).toEqual([]);
    expect(await frameItems(sandboxed, 'card')).toEqual([]);
    expect((await pageInfo(sandboxed)).counts).toEqual({ logins: 0, cards: 0, identities: 0 });
    expect(await fill(sandboxed, 'bank', await offered(7, 'bank', true))).toEqual(refused);
    expect(await fill(sandboxed, 'card', await picked(0, 'card'))).toEqual(refused);
  });

  it('nor when only its content script says so (Firefox: window.origin "null")', async () => {
    const sandboxed = frame('https://bank.example/upload/1', 0, 7, { origin: 'null' });
    expect(await frameItems(sandboxed, 'login')).toEqual([]);
    expect(await fill(sandboxed, 'bank', await picked(0, 'bank'))).toEqual(refused);
    const framed = frame('https://bank.example/upload/2', 3, 7, { origin: 'null' });
    expect(await frameItems(framed, 'card')).toEqual([]);
  });

  it('nor a document whose origin differs from its address’s', async () => {
    const other = frame('https://bank.example/', 0, 7, { senderOrigin: 'https://evil.example' });
    expect(await frameItems(other, 'login')).toEqual([]);
    const reported = frame('https://bank.example/', 0, 7, { origin: 'https://evil.example' });
    expect(await frameItems(reported, 'login')).toEqual([]);
  });

  it('nor a frame whose content script doesn’t answer', async () => {
    const silent: Sender = { ...frame('https://bank.example/'), documentId: 'gone' };
    expect(await frameItems(silent, 'login')).toEqual([]);
  });

  it('cards only when every frame up to the top is of the same origin (A in B in A is not)', async () => {
    const aba = frame('https://bank.example/pay', 4, 7, {
      ancestors: ['https://ads.example', 'https://bank.example'],
    });
    expect(await frameItems(aba, 'card')).toEqual([]);
    expect(await fill(aba, 'card', await picked(4, 'card'))).toEqual({
      filled: false,
      reason: 'no-match',
    });
    const direct = frame('https://bank.example/pay', 4, 7, {
      ancestors: ['https://bank.example', 'https://bank.example'],
    });
    expect(ids(await frameItems(direct, 'card'))).toEqual(['card']);
  });

  it('a document in a matching frame still gets its logins', async () => {
    const login = frame('https://login.bank.example/', 2, 7, {
      senderOrigin: 'https://login.bank.example',
      ancestors: ['https://bank.example'],
    });
    expect(ids(await frameItems(login, 'login'))).toEqual(['bank']);
  });
});
