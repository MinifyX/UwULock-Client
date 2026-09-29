// @vitest-environment node
/**
 * Which frame gets an item's values (background/autofill.ts): only a frame whose own address
 * matches the item; a pick in the popup reaches the page itself but never its iframes; cards
 * and addresses only the page and frames of its origin; plain http asks first; an offer is
 * only good for its tab, its item and a few seconds.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

const sent: { tabId: number; message: { token?: string }; options?: { frameId?: number } }[] = [];

vi.mock('../src/shared/browser', () => ({
  ext: {
    tabs: {
      onRemoved: { addListener: () => undefined },
      sendMessage: async (
        tabId: number,
        message: { token?: string },
        options?: { frameId?: number },
      ) => {
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

vi.mock('../src/background/vault', () => ({
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

const { fill, fillBest, fillTab, frameItems, offer, pageInfo } =
  await import('../src/background/autofill');

type Sender = chrome.runtime.MessageSender;
const frame = (url: string, frameId = 0, tabId = 7): Sender => ({
  id: 'test',
  url,
  frameId,
  tab: { id: tabId } as chrome.tabs.Tab,
});

async function offered(tabId: number, itemId: string, explicit: boolean, insecureOk = false) {
  sent.length = 0;
  await offer(tabId, itemId, explicit, insecureOk);
  return sent[0]!.message.token!;
}

/** A pick in the inline menu's frame of `frameId`: an offer for that frame alone. */
async function picked(frameId: number, itemId: string, tabId = 7, insecureOk = false) {
  sent.length = 0;
  await offer(tabId, itemId, false, insecureOk, false, { frameId, session: 'menu-1' });
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
      expect(await fill(page, 'bank', token, false)).toEqual({ filled: false, reason: 'expired' });
    }
  });

  it("gives a login to a frame whose own address matches, from that frame's menu", async () => {
    const login = frame('https://login.bank.example/', 2);
    const token = await picked(2, 'bank');
    expect(sent[0]).toMatchObject({ options: { frameId: 2 } });
    expect(await fill(login, 'bank', token, false)).toEqual({
      filled: true,
      values: expect.objectContaining({ password: 'hunter2' }),
    });
    // Once.
    expect(await fill(login, 'bank', token, false)).toEqual({ filled: false, reason: 'expired' });
  });

  it("a menu's pick is for its own frame only", async () => {
    const token = await picked(2, 'bank');
    expect(await fill(frame('https://bank.example/', 0), 'bank', token, false)).toEqual({
      filled: false,
      reason: 'expired',
    });
  });

  it('never to a frame of another site, not even inside the bank’s page', async () => {
    const ad = frame('https://ads.example/frame', 3);
    expect(await fill(ad, 'bank', await picked(3, 'bank'), false)).toEqual({
      filled: false,
      reason: 'no-match',
    });
    const token = await offered(7, 'bank', true);
    expect(await fill(ad, 'bank', token, false)).toEqual({ filled: false, reason: 'no-match' });
  });

  it('a pick in the popup reaches the page itself, even where it doesn’t match', async () => {
    const other = frame('https://other.example/');
    const token = await offered(7, 'bank', true);
    expect((await fill(other, 'bank', token, false)).filled).toBe(true);
    // Without the pick (the shortcut), it doesn't.
    const best = await offered(7, 'bank', false);
    expect(await fill(other, 'bank', best, false)).toEqual({ filled: false, reason: 'no-match' });
  });

  it('an offer is for its tab and its item only', async () => {
    const token = await offered(7, 'bank', true);
    expect(await fill(frame('https://bank.example/', 0, 8), 'bank', token, false)).toEqual({
      filled: false,
      reason: 'expired',
    });
    expect(await fill(frame('https://bank.example/'), 'card', token, false)).toEqual({
      filled: false,
      reason: 'expired',
    });
  });

  it('asks first on plain http', async () => {
    const plain = { ...frame('http://bank.example/'), url: 'http://bank.example/' };
    expect(await fill(plain, 'bank', await picked(0, 'bank'), false)).toEqual({
      filled: false,
      reason: 'insecure',
    });
    expect((await fill(plain, 'bank', await picked(0, 'bank'), true)).filled).toBe(true);
    expect((await fill(plain, 'bank', await picked(0, 'bank', 7, true), false)).filled).toBe(true);
  });

  it('cards only into the page and frames of its own origin, offer or not', async () => {
    const token = await offered(7, 'card', true);
    const payment = frame('https://pay.example/card', 5);
    const refused = { filled: false, reason: 'no-match' };
    expect(await fill(payment, 'card', token, false)).toEqual(refused);
    // Not from a pick in that frame's own menu either.
    expect(await fill(payment, 'card', await picked(5, 'card'), false)).toEqual(refused);
    const own = frame('https://bank.example/checkout', 4);
    expect((await fill(own, 'card', await picked(4, 'card'), false)).filled).toBe(true);
    expect((await fill(frame('https://bank.example/'), 'card', token, false)).filled).toBe(true);
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

describe('address matching (CL-L11)', () => {
  beforeEach(() => {
    equivalents = { global: [], custom: [] };
  });

  it('tries a regular expression in the top frame only', async () => {
    const top = frame('https://intra.example.org/login');
    expect(ids(await frameItems(top, 'login'))).toEqual(['intranet']);
    expect((await fill(top, 'intranet', await picked(0, 'intranet'), false)).filled).toBe(true);
    const inner = frame('https://intra.example.org/login', 2);
    expect(await frameItems(inner, 'login')).toEqual([]);
    expect(await fill(inner, 'intranet', await picked(2, 'intranet'), false)).toEqual({
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
    expect(await fill(page, 'guarded', await picked(0, 'guarded'), false)).toEqual({
      filled: false,
      reason: 'reprompt',
    });
    const token = await offered(7, 'guarded', true);
    expect(await fill(page, 'guarded', token, false)).toEqual({
      filled: false,
      reason: 'reprompt',
    });
  });

  it('fills once with the master password asked for that fill', async () => {
    await expect(fillTab('guarded', false, undefined)).rejects.toMatchObject({ kind: 'verify' });
    await expect(fillTab('guarded', false, 'wrong')).rejects.toMatchObject({
      kind: 'wrong-password',
    });
    sent.length = 0;
    await fillTab('guarded', false, 'master');
    const token = sent[0]!.message.token!;
    const page = frame('https://vault.example.net/login');
    expect((await fill(page, 'guarded', token, false)).filled).toBe(true);
    // The answer was for that fill: the same offer fills nothing more.
    expect((await fill(page, 'guarded', token, false)).filled).toBe(false);
  });

  it('the shortcut opens the popup instead', async () => {
    sent.length = 0;
    await fillBest({ id: 7, url: 'https://vault.example.net/login' } as chrome.tabs.Tab);
    expect(sent).toEqual([]);
  });
});
