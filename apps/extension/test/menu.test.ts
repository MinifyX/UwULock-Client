// @vitest-environment node
/**
 * The inline menu's list in an extension frame (background/menu.ts, CL-L8): the names go only to
 * the menu's frame of the session's tab, the first frame to claim a session keeps it, and a pick
 * there becomes an offer for the field's frame alone — after the page's side said the frame was
 * seen uncovered, and never for an item the menu doesn't list.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MenuMessage, PageItem } from '../src/shared/protocol';

const toContent: { tabId: number; message: { type: string }; frameId?: number }[] = [];
let guardAnswer: unknown = true;
const offers: unknown[][] = [];
let popups = 0;

vi.mock('../src/shared/browser', () => ({
  ext: {
    runtime: { id: 'uwulock', getURL: (path: string) => `chrome-extension://uwulock${path}` },
    tabs: {
      onRemoved: { addListener: () => undefined },
      create: async () => undefined,
      sendMessage: async (
        tabId: number,
        message: { type: string },
        options?: { frameId?: number },
      ) => {
        toContent.push({ tabId, message, frameId: options?.frameId });
        return message.type === 'bg:menu-guard' ? guardAnswer : null;
      },
    },
  },
}));

const item = (id: string, reprompt = false): PageItem => ({
  id,
  kind: 'login',
  name: id,
  subtitle: 'nyu',
  favorite: false,
  hasTotp: false,
  reprompt,
});

vi.mock('../src/background/autofill', () => ({
  frameItems: async (sender: { url: string; frameId: number }) =>
    sender.url.startsWith('https://bank.example/') ? [item('bank'), item('guarded', true)] : [],
  offer: async (...args: unknown[]) => void offers.push(args),
  tabUrl: () => 'https://bank.example/login',
}));
vi.mock('../src/background/session', () => ({
  vaultState: async () => 'unlocked',
  unlockedAccountId: () => 'user-1',
  requireUnlocked: async () => ({ id: 'user-1' }),
  openPopup: async () => void popups++,
}));
vi.mock('../src/background/settings', () => ({ settings: async () => ({ language: 'en' }) }));
vi.mock('../src/background/store', () => ({ activeAccount: async () => ({ id: 'user-1' }) }));
vi.mock('../src/background/extras', () => ({
  createMasked: async () => ({ email: 'x@example.com' }),
  maskedSettingsUrl: () => 'https://vault.example.com/#/settings/masked',
}));

const menu = await import('../src/background/menu');

type Sender = chrome.runtime.MessageSender;
const content = (url: string, frameId = 0, tabId = 7): Sender => ({
  id: 'uwulock',
  url,
  frameId,
  tab: { id: tabId } as chrome.tabs.Tab,
});

/** A port from a menu frame in tab `tabId`; what it got, and a way to send. */
function menuPort(tabId = 7, frameId = 9) {
  const got: MenuMessage[] = [];
  let listener: (message: unknown) => void = () => undefined;
  const port = {
    name: 'menu',
    sender: {
      id: 'uwulock',
      url: 'chrome-extension://dynamic-id/menu.html#x',
      frameId,
      tab: { id: tabId },
    },
    postMessage: (message: MenuMessage) => got.push(message),
    disconnect: () => undefined,
    onMessage: { addListener: (fn: (message: unknown) => void) => (listener = fn) },
    onDisconnect: { addListener: () => undefined },
  } as unknown as chrome.runtime.Port;
  menu.connect(port, Promise.resolve());
  const send = async (message: unknown) => {
    listener(message);
    // The handler runs after the `restored` promise and a few awaits.
    for (let i = 0; i < 20; i++) await Promise.resolve();
  };
  return { port, got, send };
}

beforeEach(() => {
  toContent.length = 0;
  offers.length = 0;
  guardAnswer = true;
  popups = 0;
});

describe('the menu frame', () => {
  it('is a page of this extension at /menu.html, whatever its host (dynamic addresses)', () => {
    expect(menu.isMenuFrame(menuPort().port.sender)).toBe(true);
    expect(menu.isMenuFrame(content('https://bank.example/menu.html'))).toBe(false);
    expect(
      menu.isMenuFrame({ id: 'uwulock', url: 'chrome-extension://uwulock/popup.html' } as Sender),
    ).toBe(false);
    expect(
      menu.isMenuFrame({ id: 'other', url: 'chrome-extension://other/menu.html' } as Sender),
    ).toBe(false);
  });

  it("gets the names for its session's frame, and only the first frame of that tab does", async () => {
    const { session } = menu.open(content('https://bank.example/login'), 'login', false);
    const stranger = menuPort(8);
    await stranger.send({ type: 'hello', session });
    expect(stranger.got).toEqual([{ type: 'gone' }]);

    const own = menuPort(7);
    await own.send({ type: 'hello', session });
    expect(own.got[0]).toMatchObject({
      type: 'view',
      view: { state: 'unlocked', kind: 'login', items: [{ id: 'bank' }, { id: 'guarded' }] },
    });

    const second = menuPort(7, 10);
    await second.send({ type: 'hello', session });
    expect(second.got).toEqual([{ type: 'gone' }]);
  });

  it('turns a pick into an offer for the field’s frame alone, after the page’s side saw it', async () => {
    const { session } = menu.open(content('https://bank.example/login', 3), 'login', false);
    const own = menuPort();
    await own.send({ type: 'hello', session });
    await own.send({ type: 'pick', itemId: 'bank' });
    expect(toContent[0]).toMatchObject({
      tabId: 7,
      frameId: 3,
      message: { type: 'bg:menu-guard', session },
    });
    expect(offers).toEqual([[7, 'bank', false, false, false, { frameId: 3, session }]]);
    expect(own.got.at(-1)).toEqual({ type: 'picked', answer: { filled: true } });
  });

  it('takes no pick the page’s side didn’t see', async () => {
    const { session } = menu.open(content('https://bank.example/login'), 'login', false);
    const own = menuPort();
    await own.send({ type: 'hello', session });
    guardAnswer = false;
    await own.send({ type: 'pick', itemId: 'bank' });
    expect(offers).toEqual([]);
    expect(own.got.at(-1)).toEqual({ type: 'picked', answer: { filled: false, reason: 'unseen' } });
  });

  it('never offers what it doesn’t list, and asks in the popup for a re-prompt', async () => {
    const { session } = menu.open(content('https://bank.example/login'), 'login', false);
    const own = menuPort();
    await own.send({ type: 'hello', session });
    await own.send({ type: 'pick', itemId: 'someone-elses' });
    expect(own.got.at(-1)).toEqual({
      type: 'picked',
      answer: { filled: false, reason: 'no-match' },
    });
    await own.send({ type: 'pick', itemId: 'guarded' });
    expect(own.got.at(-1)).toEqual({
      type: 'picked',
      answer: { filled: false, reason: 'reprompt' },
    });
    expect(popups).toBe(1);
    expect(offers).toEqual([]);
  });

  it('a session belongs to the frame that opened it', async () => {
    const opened = menu.open(content('https://bank.example/login', 3), 'login', false);
    menu.focusFromContent(content('https://bank.example/login', 4), opened.session);
    menu.closeFromContent(content('https://bank.example/login', 4), opened.session);
    const own = menuPort();
    await own.send({ type: 'hello', session: opened.session });
    // Not closed by another frame; and no ↓ from it either.
    expect(own.got.map((m) => m.type)).toEqual(['view']);
  });

  it('refuses a field that is no web page, and unknown kinds', () => {
    expect(() => menu.open(content('chrome://settings'), 'login', false)).toThrow();
    expect(() => menu.open(content('https://bank.example/'), 'secrets', false)).toThrow();
  });
});
