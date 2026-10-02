// @vitest-environment node
/**
 * A login sent in a page is saved only into the account it was sent for (background/autofill.ts):
 * the bar asks again when another account was opened meanwhile, and logins sent while locked
 * wait for their own account.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

const memory = new Map<string, unknown>();
const saved: { account: string | null; name: string }[] = [];
let open: string | null = 'user-a';
let active = 'user-a';

vi.mock('../src/shared/browser', () => ({
  isFirefox: false,
  ext: {
    tabs: {
      onRemoved: { addListener: () => undefined },
      // Only the content script's answer about its document (content/frame.ts) matters here.
      sendMessage: async (_tab: number, message: { type?: string }) =>
        message.type === 'bg:frame-document'
          ? { origin: 'https://shop.example', ancestors: [], parents: [] }
          : undefined,
    },
    runtime: { getURL: (path: string) => `chrome-extension://test${path}` },
  },
}));
vi.mock('../src/background/session', () => ({
  unlockedAccountId: () => open,
  touch: async () => undefined,
  vaultState: async () => (open ? 'unlocked' : 'locked'),
  requireUnlocked: async () => {
    if (!open) throw { kind: 'locked', message: 'Locked.' };
    return { id: open };
  },
}));
vi.mock('../src/background/settings', () => ({
  settings: async () => ({ defaultMatch: 0, savePrompt: true, neverSave: [], language: 'en' }),
  updateSettings: async () => undefined,
}));
vi.mock('../src/background/store', () => ({
  activeAccount: async () => ({ id: active, uwu: null }),
  session: async (key: string) => memory.get(key),
  setSession: async (key: string, value: unknown) => void memory.set(key, value),
}));
vi.mock('../src/background/clipboard', () => ({ copy: async () => undefined }));
vi.mock('../src/background/events', () => ({ changed: () => undefined }));
vi.mock('../src/background/vault', () => ({
  autofillIndex: () => [],
  domains: () => ({ global: [], custom: [] }),
  items: async () => [],
  saveItem: async (_id: null, draft: { name: string }) => {
    saved.push({ account: open, name: draft.name });
    return 'new';
  },
  savePassword: async () => undefined,
}));
vi.mock('../src/background/wasm', () => ({
  callJson: async () => ({}),
  call: async () => undefined,
}));

const autofill = await import('../src/background/autofill');

type Sender = chrome.runtime.MessageSender;
const top: Sender = {
  id: 'test',
  url: 'https://shop.example/login',
  frameId: 0,
  tab: { id: 7 } as chrome.tabs.Tab,
};

beforeEach(() => {
  memory.clear();
  saved.length = 0;
  open = 'user-a';
  active = 'user-a';
});

describe('a login waiting to be saved', () => {
  it('is saved into the account it was sent in', async () => {
    await autofill.submitted(top, 'nyu', 'pw', null);
    const prompt = autofill.pendingPrompt(top)!;
    expect(await autofill.promptAnswer(top, prompt.id, 'save')).toBeNull();
    expect(saved).toEqual([{ account: 'user-a', name: 'shop.example' }]);
  });

  it('is asked again when another account was opened meanwhile', async () => {
    await autofill.submitted(top, 'nyu', 'pw', null);
    const prompt = autofill.pendingPrompt(top)!;
    open = 'user-b';
    const again = await autofill.promptAnswer(top, prompt.id, 'save');
    expect(saved).toEqual([]);
    expect(again).toMatchObject({ action: 'save', host: 'shop.example' });
    expect(again!.id).not.toBe(prompt.id);
    // Answered again, it goes into the account open now, which the new question was for.
    expect(await autofill.promptAnswer(top, again!.id, 'save')).toBeNull();
    expect(saved).toEqual([{ account: 'user-b', name: 'shop.example' }]);
  });

  it('sent while locked, waits for its own account', async () => {
    open = null;
    await autofill.submitted(top, 'nyu', 'pw', null);
    open = 'user-b';
    active = 'user-b';
    expect(await autofill.pendingSaves()).toEqual([]);
    open = 'user-a';
    active = 'user-a';
    const [waiting] = await autofill.pendingSaves();
    expect(waiting).toMatchObject({ host: 'shop.example', username: 'nyu' });
    open = 'user-b';
    await expect(autofill.answerPendingSave(waiting!.id, 'save')).rejects.toMatchObject({
      kind: 'account-changed',
    });
    expect(saved).toEqual([]);
    open = 'user-a';
    const [still] = await autofill.pendingSaves();
    await autofill.answerPendingSave(still!.id, 'save');
    expect(saved).toEqual([{ account: 'user-a', name: 'shop.example' }]);
  });
});
