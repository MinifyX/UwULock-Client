// @vitest-environment jsdom
/**
 * The page can't pop the passkey window at will, nor click it as it opens (R4-5): requests
 * that aren't conditional need the person's activation (content/activation.ts), and the
 * window's main button is live only after the window was focused and visible for 500 ms
 * (prompt/armed.ts). And what a frame reports about its document (content/frame.ts, R4-2).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { activated, needsActivation } from '../src/content/activation';
import { ARM_DELAY, watchArmed } from '../src/prompt/armed';

vi.mock('../src/shared/browser', () => ({ ext: { runtime: { id: 'uwulock' } } }));
const { answerFrameDocument, frameDocument } = await import('../src/content/frame');

describe('user activation', () => {
  afterEach(() => {
    delete (navigator as unknown as { userActivation?: unknown }).userActivation;
  });

  it('needs it for create and for get, not for conditional get', () => {
    expect(needsActivation('create', {})).toBe(true);
    expect(needsActivation('create', { mediation: 'conditional' })).toBe(true);
    expect(needsActivation('get', {})).toBe(true);
    expect(needsActivation('get', { mediation: 'optional' })).toBe(true);
    expect(needsActivation('get', { mediation: 'conditional' })).toBe(false);
  });

  it('is active only when the browser says so', () => {
    expect(activated()).toBe(false);
    Object.defineProperty(navigator, 'userActivation', {
      value: { isActive: false },
      configurable: true,
    });
    expect(activated()).toBe(false);
    Object.defineProperty(navigator, 'userActivation', {
      value: { isActive: true },
      configurable: true,
    });
    expect(activated()).toBe(true);
  });
});

describe('the main button', () => {
  let focused = true;
  beforeEach(() => {
    vi.useFakeTimers();
    focused = true;
    vi.spyOn(document, 'hasFocus').mockImplementation(() => focused);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it(`is armed ${ARM_DELAY} ms after the window has focus, and disarmed when it loses it`, () => {
    const states: boolean[] = [];
    const stop = watchArmed(window, (armed) => states.push(armed));
    expect(states.at(-1)).toBe(false);
    vi.advanceTimersByTime(ARM_DELAY - 1);
    expect(states.at(-1)).toBe(false);
    vi.advanceTimersByTime(1);
    expect(states.at(-1)).toBe(true);

    focused = false;
    window.dispatchEvent(new Event('blur'));
    expect(states.at(-1)).toBe(false);
    vi.advanceTimersByTime(ARM_DELAY * 4);
    expect(states.at(-1)).toBe(false);

    // Focus again: another full delay.
    focused = true;
    window.dispatchEvent(new Event('focus'));
    vi.advanceTimersByTime(ARM_DELAY - 1);
    expect(states.at(-1)).toBe(false);
    vi.advanceTimersByTime(1);
    expect(states.at(-1)).toBe(true);
    stop();
  });

  it('a window opened without focus waits for it', () => {
    focused = false;
    const states: boolean[] = [];
    const stop = watchArmed(window, (armed) => states.push(armed));
    vi.advanceTimersByTime(ARM_DELAY * 4);
    expect(states).toEqual([false]);
    stop();
  });
});

describe("a frame's document", () => {
  it('reports window.origin and the ancestors', () => {
    const report = frameDocument();
    expect(report.origin).toBe(window.origin);
    // The top frame: no ancestors either way.
    expect(report.parents).toEqual([]);
    expect(report.ancestors === null || report.ancestors.length === 0).toBe(true);
  });

  it('answers the background only', () => {
    const respond = vi.fn();
    const question = { type: 'bg:frame-document' };
    expect(answerFrameDocument(question, { id: 'other' }, respond)).toBe(false);
    expect(answerFrameDocument(question, { id: 'uwulock', tab: { id: 1 } as never }, respond)).toBe(
      false,
    );
    expect(answerFrameDocument({ type: 'bg:fill-offer' }, { id: 'uwulock' }, respond)).toBe(false);
    expect(respond).not.toHaveBeenCalled();
    expect(answerFrameDocument(question, { id: 'uwulock' }, respond)).toBe(true);
    expect(respond).toHaveBeenCalledWith(frameDocument());
  });
});
