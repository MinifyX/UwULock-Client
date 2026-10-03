// @vitest-environment node
/**
 * Who may ask for a passkey (background/passkeys.ts, R4-2/R4-3/R4-5/R4-6): the document's real
 * origin, not its address's — a sandboxed document gets the browser's authenticator; a frame
 * only when it and every frame above are of one origin, as `location.ancestorOrigins` shows;
 * one window per tab, a pause after a cancel; an existing login only when its own address is on
 * the RP id.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

/** What each document's content script says about it (content/frame.ts), by `documentId`. */
const documents = new Map<
  string,
  { origin: string; ancestors: string[] | null; parents: string[] | null }
>();
const windows: string[] = [];
const removedListeners: ((id: number) => void)[] = [];

vi.mock('../src/shared/browser', () => ({
  isFirefox: false,
  ext: {
    tabs: {
      onRemoved: { addListener: () => undefined },
      sendMessage: async (
        _tab: number,
        message: { type?: string },
        options?: { documentId?: string },
      ) =>
        message.type === 'bg:frame-document' ? documents.get(options?.documentId ?? '') : undefined,
    },
    windows: {
      create: async ({ url }: { url: string }) => {
        windows.push(url);
        return { id: windows.length };
      },
      remove: async (id: number) => {
        for (const listener of removedListeners) listener(id);
      },
      onRemoved: {
        addListener: (listener: (id: number) => void) => removedListeners.push(listener),
      },
    },
    runtime: { getURL: (path: string) => `chrome-extension://test/${path}` },
  },
}));

vi.mock('../src/background/session', () => ({
  unlockedAccountId: () => 'user-1',
  vaultState: async () => 'unlocked',
  requireUnlocked: async () => ({ id: 'user-1' }),
  touch: async () => undefined,
}));
vi.mock('../src/background/settings', () => ({
  settings: async () => ({ passkeys: true, defaultMatch: 0 }),
  updateSettings: async () => undefined,
}));
vi.mock('../src/background/store', () => ({
  activeAccount: async () => ({ id: 'user-1', uwu: null }),
  session: async () => undefined,
  setSession: async () => undefined,
}));
vi.mock('../src/background/clipboard', () => ({ copy: async () => undefined }));
vi.mock('../src/background/events', () => ({ changed: () => undefined }));

const passkey = {
  credentialId: 'Y3JlZA',
  rpId: 'bank.example',
  discoverable: true,
  userName: 'nyu',
};
const login = (id: string, uri: string, passkeys: unknown[] = []) => ({
  id,
  kind: 'login',
  name: id,
  subtitle: 'nyu',
  favorite: false,
  reprompt: false,
  hasTotp: false,
  uris: [{ uri, match: null }],
  passkeys,
});
vi.mock('../src/background/vault', () => ({
  autofillIndex: () => [
    login('bank', 'https://bank.example/login', [passkey]),
    login('forum', 'https://forum.bank.example/'),
    login('www', 'https://www.bank.example/'),
  ],
  domains: () => ({ global: [], custom: [] }),
}));
vi.mock('../src/background/wasm', () => ({ callJson: async () => ({}) }));

const passkeys = await import('../src/background/passkeys');
const { loginFitsRpId } = passkeys;
const { passkeyLoginName } = await import('../src/prompt/names');

type Sender = chrome.runtime.MessageSender;
let count = 0;
function frame(
  url: string,
  frameId = 0,
  doc: {
    origin?: string;
    ancestors?: string[] | null;
    parents?: string[] | null;
    senderOrigin?: string;
    tab?: number;
  } = {},
): Sender {
  const documentId = `doc-${++count}`;
  const ancestors = doc.ancestors !== undefined ? doc.ancestors : [];
  documents.set(documentId, {
    origin: doc.origin ?? new URL(url).origin,
    ancestors,
    parents: doc.parents !== undefined ? doc.parents : (ancestors ?? []),
  });
  return {
    id: 'test',
    url,
    frameId,
    documentId,
    ...(doc.senderOrigin !== undefined ? { origin: doc.senderOrigin } : {}),
    tab: { id: doc.tab ?? 7 } as chrome.tabs.Tab,
  };
}

const getOptions = { rpId: 'bank.example', challenge: 'AAAA', allowCredentials: [] };
const createOptions = {
  rp: { id: 'bank.example', name: 'PayPal' },
  user: { id: 'dQ', name: 'nyu', displayName: 'Nyu' },
  challenge: 'AAAA',
  pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
};

/** Starts a request; whether it opened the window, and its prompt id. */
async function start(sender: Sender, kind: 'get' | 'create' = 'get') {
  const before = windows.length;
  const answer =
    kind === 'get'
      ? passkeys.get(sender, `page-${++count}`, getOptions)
      : passkeys.create(sender, `page-${++count}`, createOptions);
  // Let the checks (one round trip to the frame) and the window's creation run.
  for (let i = 0; i < 20 && windows.length === before; i++) await Promise.resolve();
  // …and the window's id reach the request.
  for (let i = 0; i < 5; i++) await Promise.resolve();
  const url = windows[before];
  return { answer, id: url ? new URL(url).searchParams.get('id')! : null };
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
  windows.length = 0;
});
afterEach(async () => {
  vi.useRealTimers();
});

const FALLBACK = { kind: 'fallback' };

describe('the asking document (R4-2)', () => {
  it('a sandboxed document gets the browser: Chromium says sender.origin "null"', async () => {
    const { answer, id } = await start(
      frame('https://bank.example/upload', 0, { senderOrigin: 'null' }),
    );
    expect(id).toBeNull();
    expect(await answer).toEqual(FALLBACK);
  });

  it('…and Firefox: its content script reports window.origin "null"', async () => {
    const { answer, id } = await start(
      frame('https://bank.example/upload', 0, { origin: 'null' }),
      'create',
    );
    expect(id).toBeNull();
    expect(await answer).toEqual(FALLBACK);
  });

  it('a document of another origin than its address gets the browser', async () => {
    const other = frame('https://bank.example/', 0, { senderOrigin: 'https://evil.example' });
    expect(await (await start(other)).answer).toEqual(FALLBACK);
  });

  it('a frame inside another origin inside the page (A in B in A) gets the browser', async () => {
    const aba = frame('https://bank.example/inner', 3, {
      senderOrigin: 'https://bank.example',
      ancestors: ['https://ads.example', 'https://bank.example'],
    });
    const { answer, id } = await start(aba);
    expect(id).toBeNull();
    expect(await answer).toEqual(FALLBACK);
  });

  it('a frame without ancestorOrigins (Firefox) gets the browser', async () => {
    const { answer, id } = await start(
      frame('https://bank.example/inner', 3, {
        ancestors: null,
        parents: ['https://bank.example'],
        tab: 9,
      }),
    );
    expect(id).toBeNull();
    expect(await answer).toEqual(FALLBACK);
  });

  it('the page itself without ancestorOrigins (Firefox) asks UwULock (R6 F1)', async () => {
    const top = await start(frame('https://bank.example/', 0, { ancestors: null, tab: 10 }));
    expect(top.id).not.toBeNull();
    await passkeys.decide({ id: top.id!, choice: 'browser' } as never);
    expect(await top.answer).toEqual(FALLBACK);

    const created = await start(
      frame('https://bank.example/', 0, { ancestors: null, tab: 11 }),
      'create',
    );
    expect(created.id).not.toBeNull();
    await passkeys.decide({ id: created.id!, choice: 'browser' } as never);
    expect(await created.answer).toEqual(FALLBACK);
  });

  it('a top frame reporting no chain at all gets the browser', async () => {
    const { answer, id } = await start(
      frame('https://bank.example/', 0, { ancestors: null, parents: null, tab: 12 }),
    );
    expect(id).toBeNull();
    expect(await answer).toEqual(FALLBACK);
  });

  it('the page itself, and a frame of one origin all the way up, ask UwULock', async () => {
    const top = await start(
      frame('https://bank.example/', 0, { senderOrigin: 'https://bank.example' }),
    );
    expect(top.id).not.toBeNull();
    await passkeys.decide({ id: top.id!, choice: 'browser' } as never);
    expect(await top.answer).toEqual(FALLBACK);

    const inner = await start(
      frame('https://bank.example/inner', 3, {
        ancestors: ['https://bank.example', 'https://bank.example'],
        tab: 8,
      }),
    );
    expect(inner.id).not.toBeNull();
    const prompt = await passkeys.prompt(inner.id!);
    expect(prompt.origin).toBe('https://bank.example');
    passkeys.abort(`page-${count - 1}`);
  });
});

describe('one window per tab, a pause after a cancel (R4-5)', () => {
  it('a second request while the window is open is refused', async () => {
    const first = await start(frame('https://bank.example/', 0, { tab: 20 }));
    expect(first.id).not.toBeNull();
    const second = await start(
      frame('https://bank.example/inner', 1, { ancestors: ['https://bank.example'], tab: 20 }),
    );
    expect(second.id).toBeNull();
    expect(await second.answer).toMatchObject({ kind: 'error', name: 'NotAllowedError' });
    await passkeys.decide({ id: first.id!, choice: 'cancel' } as never);
    expect(await first.answer).toEqual(FALLBACK);
  });

  it('after a cancel, that tab and origin get the browser for 10 s', async () => {
    const first = await start(frame('https://bank.example/', 0, { tab: 30 }));
    await passkeys.decide({ id: first.id!, choice: 'cancel' } as never);
    await first.answer;

    const again = await start(frame('https://bank.example/', 0, { tab: 30 }));
    expect(again.id).toBeNull();
    expect(await again.answer).toEqual(FALLBACK);
    // Another tab isn't affected.
    const elsewhere = await start(frame('https://bank.example/', 0, { tab: 31 }));
    expect(elsewhere.id).not.toBeNull();
    await passkeys.decide({ id: elsewhere.id!, choice: 'cancel' } as never);

    vi.advanceTimersByTime(passkeys.CANCEL_BACKOFF + 1);
    const later = await start(frame('https://bank.example/', 0, { tab: 30 }));
    expect(later.id).not.toBeNull();
    await passkeys.decide({ id: later.id!, choice: 'cancel' } as never);
  });

  it('closing the window counts as a cancel', async () => {
    const first = await start(frame('https://bank.example/', 0, { tab: 40 }));
    for (const listener of removedListeners) listener(windows.length);
    expect(await first.answer).toEqual(FALLBACK);
    expect((await start(frame('https://bank.example/', 0, { tab: 40 }))).id).toBeNull();
  });
});

describe('into an existing login (R4-3)', () => {
  it('offers only logins whose own address is on the RP id', async () => {
    const created = await start(frame('https://forum.bank.example/', 0, { tab: 50 }), 'create');
    // rpId bank.example from forum.bank.example: the bank's own logins and the forum's.
    const prompt = await passkeys.prompt(created.id!);
    expect(prompt.kind === 'create' && prompt.candidates.map((c) => c.id).sort()).toEqual([
      'bank',
      'forum',
      'www',
    ]);
    await passkeys.decide({ id: created.id!, choice: 'cancel' } as never);
  });

  it('matches hosts strictly', () => {
    const at = (uri: string) => ({ uris: [{ uri }] });
    expect(loginFitsRpId(at('https://bank.example/login'), 'bank.example')).toBe(true);
    expect(loginFitsRpId(at('https://www.bank.example/'), 'bank.example')).toBe(true);
    expect(loginFitsRpId(at('bank.example'), 'bank.example')).toBe(true);
    // The base domain's login is not the forum's.
    expect(loginFitsRpId(at('https://bank.example/'), 'forum.bank.example')).toBe(false);
    expect(loginFitsRpId(at('https://notbank.example/'), 'bank.example')).toBe(false);
    expect(loginFitsRpId(at('https://bank.example.net/'), 'bank.example')).toBe(false);
    expect(loginFitsRpId({ uris: [] }, 'bank.example')).toBe(false);
  });
});

describe('names (R4-6)', () => {
  it('names a new login after the RP id, with the name the site gave itself', () => {
    expect(passkeyLoginName('evil.example', 'PayPal')).toBe('PayPal (evil.example)');
    expect(passkeyLoginName('bank.example', 'bank.example')).toBe('bank.example');
    expect(passkeyLoginName('bank.example', '  ')).toBe('bank.example');
    expect(passkeyLoginName('bank.example', null)).toBe('bank.example');
  });
});
