/**
 * The inline menu's frame (menu/main.ts, CL-L8): it says hello with its session, lists what the
 * background sends, and sends a pick only for a real pointer pressed on the entry after the list
 * had been shown for a moment, or for Enter on an entry a key in here (or the field's ↓)
 * selected — not for the page focusing the frame and waiting for an Enter.
 */

// @ts-expect-error jsdom's internals have no types.
import { implSymbol } from 'jsdom/lib/jsdom/living/generated/utils.js';
import { afterAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { MenuMessage, MenuRequest } from '../src/shared/protocol';

type Impl = { isTrusted: boolean; _dispatch: (event: unknown) => void };
const impl = (value: object) => (value as Record<symbol, Impl>)[implSymbol as symbol]!;
function fire(target: EventTarget, event: Event) {
  impl(event).isTrusted = true;
  impl(target)._dispatch(impl(event));
}

const posted: MenuRequest[] = [];
let deliver: (message: MenuMessage) => void = () => undefined;

vi.mock('../src/shared/browser', () => ({
  ext: {
    runtime: {
      connect: () => ({
        postMessage: (message: MenuRequest) => posted.push(message),
        onMessage: { addListener: (fn: (m: MenuMessage) => void) => (deliver = fn) },
        onDisconnect: { addListener: () => undefined },
      }),
    },
  },
}));

vi.useFakeTimers({ toFake: ['performance'] });
window.location.hash = '#session-1';
await import('../src/menu/main');
const hello = [...posted];

afterAll(() => vi.useRealTimers());

const picks = () => posted.filter((m) => m.type === 'pick');
const options = () => Array.from(document.querySelectorAll<HTMLElement>('[role="option"]'));

function show() {
  deliver({
    type: 'view',
    view: {
      state: 'unlocked',
      kind: 'login',
      language: 'en',
      masked: false,
      items: [
        {
          id: 'bank',
          kind: 'login',
          name: 'Bank',
          subtitle: 'nyu',
          favorite: false,
          hasTotp: false,
          reprompt: false,
        },
      ],
    },
  });
}

async function settled() {
  // The MutationObserver's notice of the new list.
  await Promise.resolve();
}

function pointerOn(el: HTMLElement) {
  (document as Document & { elementFromPoint: unknown }).elementFromPoint = () => el;
  fire(el, new MouseEvent('pointerdown', { bubbles: true, composed: true }));
  fire(el, new MouseEvent('click', { bubbles: true, detail: 1, clientX: 5, clientY: 5 }));
}

function key(name: string) {
  fire(
    document.activeElement ?? document.body,
    new KeyboardEvent('keydown', { key: name, bubbles: true }),
  );
}

beforeEach(async () => {
  // Whatever an earlier test picked has been answered.
  deliver({ type: 'picked', answer: { filled: false, reason: 'unseen' } });
  posted.length = 0;
  show();
  await settled();
});

describe("the menu's frame", () => {
  it('said hello with its session', () => {
    expect(hello).toEqual([{ type: 'hello', session: 'session-1' }]);
  });

  it('lists the names it was sent', () => {
    expect(options().map((o) => o.textContent)).toEqual(['BBanknyu']);
  });

  it('takes a pointer only after the list was shown for a moment', () => {
    pointerOn(options()[0]!);
    expect(picks()).toEqual([]);
    vi.advanceTimersByTime(600);
    pointerOn(options()[0]!);
    expect(picks()).toEqual([{ type: 'pick', itemId: 'bank' }]);
  });

  it('takes no click without a real press on the entry', () => {
    vi.advanceTimersByTime(600);
    const option = options()[0]!;
    fire(option, new MouseEvent('click', { bubbles: true, detail: 1 }));
    option.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }));
    option.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 1 }));
    expect(picks()).toEqual([]);
  });

  it('Enter picks only what a key in here or the field’s ↓ selected', () => {
    vi.advanceTimersByTime(600);
    // The page focused the frame, somebody hits Enter: nothing is selected.
    window.dispatchEvent(new FocusEvent('focus'));
    options()[0]!.focus();
    key('Enter');
    expect(picks()).toEqual([]);
    deliver({ type: 'select-first' });
    key('Enter');
    expect(picks()).toEqual([{ type: 'pick', itemId: 'bank' }]);
  });
});
