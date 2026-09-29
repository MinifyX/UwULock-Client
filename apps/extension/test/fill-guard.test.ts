// @vitest-environment node
/**
 * Which frame gets an item's values (background/autofill.ts): only a frame whose own address
 * matches the item; a pick in the popup reaches the page itself but never its iframes; cards
 * and addresses only the page and frames of its origin; plain http asks first; an offer is
 * only good for its tab, its item and a few seconds.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

const sent: { tabId: number; message: { token?: string } }[] = [];

vi.mock('../src/shared/browser', () => ({
  ext: {
    tabs: {
      onRemoved: { addListener: () => undefined },
      sendMessage: async (tabId: number, message: { token?: string }) => {
        sent.push({ tabId, message });
      },
      query: async () => [],
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
  domains: () => [],
  items: async () => [],
}));

vi.mock('../src/background/wasm', () => ({
  callJson: async () => ({ kind: 'login', username: 'nyu', password: 'hunter2', totp: null }),
  call: async () => undefined,
}));

const { fill, offer, pageInfo } = await import('../src/background/autofill');

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

describe('filling', () => {
  beforeEach(async () => {
    // The tab's top frame, as its content script reported it.
    await pageInfo(frame('https://bank.example/login'));
  });

  it('gives a login to a frame whose own address matches', async () => {
    const answer = await fill(frame('https://login.bank.example/'), 'bank', undefined, false);
    expect(answer).toEqual({
      filled: true,
      values: expect.objectContaining({ password: 'hunter2' }),
    });
  });

  it('never to a frame of another site, not even inside the bank’s page', async () => {
    const ad = frame('https://ads.example/frame', 3);
    expect(await fill(ad, 'bank', undefined, false)).toEqual({ filled: false, reason: 'no-match' });
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
    expect(await fill(frame('https://bank.example/'), 'bank', 'made-up', false)).toEqual({
      filled: false,
      reason: 'expired',
    });
  });

  it('asks first on plain http', async () => {
    const plain = { ...frame('http://bank.example/'), url: 'http://bank.example/' };
    expect(await fill(plain, 'bank', undefined, false)).toEqual({
      filled: false,
      reason: 'insecure',
    });
    expect((await fill(plain, 'bank', undefined, true)).filled).toBe(true);
  });

  it('cards only into the page and frames of its own origin, offer or not', async () => {
    const token = await offered(7, 'card', true);
    const payment = frame('https://pay.example/card', 5);
    const refused = { filled: false, reason: 'no-match' };
    expect(await fill(payment, 'card', token, false)).toEqual(refused);
    // Not from a pick in that frame's own menu either: an ad's frame could fake one.
    expect(await fill(payment, 'card', undefined, false)).toEqual(refused);
    const own = frame('https://bank.example/checkout', 4);
    expect((await fill(own, 'card', undefined, false)).filled).toBe(true);
    expect((await fill(frame('https://bank.example/'), 'card', undefined, false)).filled).toBe(
      true,
    );
  });

  it('lists cards and addresses to the page and frames of its own origin only', async () => {
    expect((await pageInfo(frame('https://ads.example/', 2))).cards).toEqual([]);
    expect((await pageInfo(frame('https://bank.example/pay', 2))).cards.map((c) => c.id)).toEqual([
      'card',
    ]);
    expect((await pageInfo(frame('https://bank.example/'))).cards.map((c) => c.id)).toEqual([
      'card',
    ]);
  });

  it('lists names, never values, and only the frame’s own logins', async () => {
    const info = await pageInfo(frame('https://ads.example/', 2));
    expect(info.logins).toEqual([]);
    const own = await pageInfo(frame('https://www.bank.example/'));
    expect(own.logins.map((l) => l.id)).toEqual(['bank']);
    expect(JSON.stringify(own)).not.toContain('hunter2');
  });
});
