/**
 * When the card "Als Standard festlegen" shows (components/AutofillCard.tsx): after an unlock,
 * while UwULock isn't the system's AutoFill provider. "Später" puts it off for a week, at most
 * three times; once UwULock is the provider it never comes back. Pure, for node --test; the
 * state lives in the page's storage (not secret, per device).
 */

export type ProviderView = {
  platform: 'ios' | 'macos' | 'android' | 'none';
  /** This system can have UwULock as its provider. */
  supported: boolean;
  /** Switched on in the system; null when it doesn't say. */
  enabled: boolean | null;
  /** The system asks itself (iOS 18+, macOS 15+, Android); otherwise the settings open. */
  direct: boolean;
  /** Android: the autofill service is UwULock's too. */
  autofill: boolean | null;
  /** iOS and macOS: UwULock's own setting that leaves the extension its list. */
  list: boolean;
};

export type PromptState = {
  /** How often "Später" was chosen. */
  later: number;
  /** Not before then (ms). */
  until: number;
  /** UwULock was the provider once: never again. */
  done: boolean;
};

export const PROMPT_KEY = 'uwulock.autofillPrompt';
export const LATER_DAYS = 7;
export const MAX_LATER = 3;
const DAY = 86_400_000;

export const FRESH: PromptState = { later: 0, until: 0, done: false };

export function readPrompt(raw: string | null): PromptState {
  if (!raw) return FRESH;
  try {
    const value = JSON.parse(raw) as Partial<PromptState>;
    return {
      later: Number.isInteger(value.later) && value.later! >= 0 ? value.later! : 0,
      until: typeof value.until === 'number' && Number.isFinite(value.until) ? value.until : 0,
      done: value.done === true,
    };
  } catch {
    return FRESH;
  }
}

/**
 * What is still missing: `credentials` (the provider for passwords and passkeys, and on iOS and
 * macOS UwULock's own list), `autofill` (Android's autofill service), or null when nothing is.
 */
export function missing(view: ProviderView | null): 'credentials' | 'autofill' | null {
  if (!view || !view.supported || view.platform === 'none') return null;
  if (view.enabled !== true) return 'credentials';
  if ((view.platform === 'ios' || view.platform === 'macos') && !view.list) return 'credentials';
  if (view.platform === 'android' && view.autofill === false) return 'autofill';
  return null;
}

/** Whether the card shows now. */
export function shouldPrompt(state: PromptState, view: ProviderView | null, now: number): boolean {
  if (state.done || state.later >= MAX_LATER || now < state.until) return false;
  return missing(view) !== null;
}

export function promptLater(state: PromptState, now: number): PromptState {
  return { ...state, later: state.later + 1, until: now + LATER_DAYS * DAY };
}

/** The provider is on: the card is done for good. */
export function promptSeen(state: PromptState, view: ProviderView | null): PromptState {
  if (state.done) return state;
  return view?.supported && missing(view) === null ? { ...state, done: true } : state;
}
