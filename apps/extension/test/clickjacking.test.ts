/**
 * The guard in front of every click on the inline menu and the save bar (content/ui.ts): only a
 * real pointer, pressed on the very button after it was shown, unchanged and uncovered for a
 * moment, and still over it; or the keyboard with the focus in it.
 *
 * jsdom makes every dispatched event untrusted and has no hit testing: the tests mark their
 * events trusted the way the browser would, and say what is under the pointer.
 */

// @ts-expect-error jsdom's internals have no types.
import { implSymbol } from 'jsdom/lib/jsdom/living/generated/utils.js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  createGuard,
  createHost,
  hidingFilter,
  MIN_SHOW_MS,
  type Guard,
  type Host,
} from '../src/content/ui';

type Impl = { isTrusted: boolean; _dispatch: (event: unknown) => void };
const impl = (value: object) => (value as Record<symbol, Impl>)[implSymbol as symbol]!;

function trusted<T extends Event>(event: T): T {
  impl(event).isTrusted = true;
  return event;
}

/** Dispatches `event` as the browser would: trusted. */
function fire(target: EventTarget, event: Event) {
  impl(target)._dispatch(impl(trusted(event)));
}

let clock = 0;
let ui: Host;
let button: HTMLButtonElement;
let other: HTMLButtonElement;
let guard: Guard;
/** What the page has on top at a point, if anything. */
let cover: Element | null = null;
/** The page's `html::after`; jsdom has no styles for pseudo-elements. */
let after = { content: 'none', display: 'block' };
const computed = window.getComputedStyle.bind(window);
window.getComputedStyle = ((el: Element, pseudo?: string | null) =>
  pseudo ? (after as unknown as CSSStyleDeclaration) : computed(el)) as typeof getComputedStyle;

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function press(on: HTMLElement = button) {
  fire(on, new MouseEvent('pointerdown', { bubbles: true, composed: true }));
}

function click(x = 50, y = 25) {
  return trusted(
    new MouseEvent('click', { bubbles: true, composed: true, detail: 1, clientX: x, clientY: y }),
  );
}

beforeEach(() => {
  clock = 1000;
  cover = null;
  after = { content: 'none', display: 'block' };
  document.documentElement.removeAttribute('style');
  document.documentElement.removeAttribute('class');
  (document as Document & { elementFromPoint: unknown }).elementFromPoint = () => cover ?? ui.host;
  ui = createHost('');
  button = document.createElement('button');
  other = document.createElement('button');
  for (const [el, top] of [
    [button, 10],
    [other, 60],
  ] as const) {
    el.getBoundingClientRect = () => new DOMRect(10, top, 100, 30);
    ui.root.append(el);
  }
  guard = createGuard(ui, () => clock);
});

afterEach(() => {
  guard.dispose();
  ui.host.remove();
});

describe('a click on our UI', () => {
  it('counts when pressed after the UI was shown for a moment, and still over it', () => {
    clock += MIN_SHOW_MS;
    press();
    expect(guard.accepts(click(), button)).toBe(true);
  });

  it('not when pressed too soon after it appeared or moved', () => {
    clock += MIN_SHOW_MS - 100;
    press();
    clock += 200;
    expect(guard.accepts(click(), button)).toBe(false);
    clock += MIN_SHOW_MS;
    guard.shown();
    clock += 100;
    press();
    expect(guard.accepts(click(), button)).toBe(false);
  });

  it('not when made up by the page', () => {
    clock += MIN_SHOW_MS;
    press();
    const fake = new MouseEvent('click', { detail: 1, clientX: 50, clientY: 25 });
    expect(guard.accepts(fake, button)).toBe(false);
  });

  it('not without a press on that very button', () => {
    clock += MIN_SHOW_MS;
    expect(guard.accepts(click(), button)).toBe(false);
    press(other);
    expect(guard.accepts(click(), button)).toBe(false);
    // One press, one click.
    press();
    expect(guard.accepts(click(), button)).toBe(true);
    expect(guard.accepts(click(), button)).toBe(false);
  });

  it('not when the page has something on top, at the press or at the click', () => {
    clock += MIN_SHOW_MS;
    cover = document.body;
    press();
    cover = null;
    expect(guard.accepts(click(), button)).toBe(false);
    press();
    cover = document.body;
    expect(guard.accepts(click(), button)).toBe(false);
  });

  it('not when the pointer is elsewhere by the click', () => {
    clock += MIN_SHOW_MS;
    press();
    (document as Document & { elementFromPoint: unknown }).elementFromPoint = (x: number) =>
      x > 500 ? document.body : ui.host;
    expect(guard.accepts(click(600, 25), button)).toBe(false);
  });

  it('not when the page makes itself or our host see-through, until it has been back a moment', async () => {
    clock += MIN_SHOW_MS;
    document.documentElement.style.opacity = '0.05';
    await flush();
    clock += MIN_SHOW_MS;
    press();
    expect(guard.accepts(click(), button)).toBe(false);
    document.documentElement.style.opacity = '';
    await flush();
    press();
    expect(guard.accepts(click(), button)).toBe(false);
    clock += MIN_SHOW_MS;
    press();
    expect(guard.accepts(click(), button)).toBe(true);
  });

  it('not under the page’s html::after, which is drawn after our host', () => {
    clock += MIN_SHOW_MS;
    after = { content: '""', display: 'block' };
    press();
    expect(guard.accepts(click(), button)).toBe(false);
  });

  it('puts back our host’s styles when the page changes them', async () => {
    ui.host.style.setProperty('opacity', '0.01', 'important');
    await flush();
    expect(ui.host.style.getPropertyValue('opacity')).toBe('1');
    expect(ui.host.style.getPropertyPriority('opacity')).toBe('important');
    // Once: putting them back is no change of the page's to answer.
    const before = ui.host.style.cssText;
    await flush();
    expect(ui.host.style.cssText).toBe(before);
    clock += MIN_SHOW_MS;
    press();
    expect(guard.accepts(click(), button)).toBe(true);
  });

  it('goes back on top of what the page adds after it', async () => {
    const overlay = document.createElement('div');
    document.documentElement.append(overlay);
    await flush();
    expect(document.documentElement.lastElementChild).toBe(ui.host);
    overlay.remove();
  });

  it('by keyboard only with the focus in the button, after a moment', () => {
    const enter = () => trusted(new KeyboardEvent('keydown', { key: 'Enter' }));
    button.focus();
    expect(guard.accepts(enter(), button)).toBe(false);
    clock += MIN_SHOW_MS;
    expect(guard.accepts(enter(), button)).toBe(true);
    expect(guard.accepts(enter(), other)).toBe(false);
  });
});

describe('page filters', () => {
  it('that make things faint or blurry count as hiding; colour filters don’t', () => {
    for (const filter of [
      'opacity(0.1)',
      'blur(4px)',
      'brightness(0.2)',
      'contrast(10%)',
      'invert(0.5)',
      'url(#x)',
    ]) {
      expect(hidingFilter(filter), filter).toBe(true);
    }
    for (const filter of [
      'none',
      '',
      'invert(1) hue-rotate(180deg)',
      'grayscale(1)',
      'contrast(0.9)',
      'drop-shadow(0 0 2px red)',
    ]) {
      expect(hidingFilter(filter), filter).toBe(false);
    }
  });
});

describe("the inline menu's frame, as the page shows it (CL-L8)", () => {
  let frame: HTMLIFrameElement;
  beforeEach(() => {
    frame = document.createElement('iframe');
    frame.getBoundingClientRect = () => new DOMRect(10, 100, 200, 120);
    ui.root.append(frame);
  });

  it('counts as seen once it was shown, unchanged and uncovered for a moment', () => {
    expect(guard.frameSeen(frame)).toBe(false);
    clock += MIN_SHOW_MS;
    expect(guard.frameSeen(frame)).toBe(true);
    cover = document.body;
    expect(guard.frameSeen(frame)).toBe(false);
  });

  it('a decoy that was over it a moment ago still counts, though gone at the check', () => {
    vi.useFakeTimers();
    try {
      guard.watchFrame(frame);
      clock += MIN_SHOW_MS;
      cover = document.body;
      vi.advanceTimersByTime(100);
      // Taken away just before the pick is checked.
      cover = null;
      expect(guard.frameSeen(frame)).toBe(false);
      clock += MIN_SHOW_MS;
      vi.advanceTimersByTime(100);
      expect(guard.frameSeen(frame)).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it('not once it is gone', () => {
    clock += MIN_SHOW_MS;
    frame.remove();
    expect(guard.frameSeen(frame)).toBe(false);
  });
});

describe('the top layer, where the browser tracks no visibility (CL-L13)', () => {
  /** A page's element with a closed shadow root, seen the way Firefox shows it to extensions. */
  function closedHost(): {
    host: HTMLElement;
    inner: HTMLElement;
    open: (selector: string) => boolean;
  } {
    const host = document.createElement('div');
    const root = host.attachShadow({ mode: 'closed' });
    const inner = document.createElement('div');
    root.append(inner);
    Object.defineProperty(host, 'openOrClosedShadowRoot', { value: root });
    let popover = false;
    // jsdom knows no popovers: the root says what is open in it.
    root.querySelectorAll = ((selector: string) =>
      selector === ':popover-open' && popover
        ? [inner]
        : []) as unknown as typeof root.querySelectorAll;
    document.body.append(host);
    return {
      host,
      inner,
      open: (selector) => (popover = selector === ':popover-open'),
    };
  }

  afterEach(() => {
    document.body.replaceChildren();
    Object.defineProperty(document, 'fullscreenElement', { value: null, configurable: true });
  });

  it('refuses a click while a popover is open in a closed shadow root of the page', () => {
    const page = closedHost();
    clock += MIN_SHOW_MS;
    page.open(':popover-open');
    press();
    expect(guard.accepts(click(), button)).toBe(false);
    page.open('');
    press();
    expect(guard.accepts(click(), button)).toBe(true);
  });

  it('looks into shadow roots inside shadow roots', () => {
    const outer = document.createElement('div');
    const outerRoot = outer.attachShadow({ mode: 'closed' });
    Object.defineProperty(outer, 'openOrClosedShadowRoot', { value: outerRoot });
    document.body.append(outer);
    const page = closedHost();
    outerRoot.append(page.host);
    page.open(':popover-open');
    clock += MIN_SHOW_MS;
    press();
    expect(guard.accepts(click(), button)).toBe(false);
  });

  it('refuses a click while something of the page is in full screen', () => {
    const page = closedHost();
    Object.defineProperty(document, 'fullscreenElement', {
      value: page.host,
      configurable: true,
    });
    clock += MIN_SHOW_MS;
    press();
    expect(guard.accepts(click(), button)).toBe(false);
    Object.defineProperty(document, 'fullscreenElement', { value: null, configurable: true });
    press();
    expect(guard.accepts(click(), button)).toBe(true);
  });
});
