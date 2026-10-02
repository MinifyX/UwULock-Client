/**
 * The generator's minimums per character set, as uwulock-core reads them
 * (`minLowercase`, `minUppercase`, `minNumber`, `minSpecial`; 0 or 1 means at
 * least one). Shared by the app's generator and the extension's: no words in
 * here.
 */

export type CharSet = 'lowercase' | 'uppercase' | 'digits' | 'symbols';

export const MIN_KEY = {
  lowercase: 'minLowercase',
  uppercase: 'minUppercase',
  digits: 'minNumber',
  symbols: 'minSpecial',
} as const;

export type MinKey = (typeof MIN_KEY)[CharSet];

export type WithMinimums = Record<CharSet, boolean> &
  Partial<Record<MinKey, number>> & {
    length: number;
  };

/** The longest password uwulock-core makes, and so the most the minimums may add up to. */
export const MAX_LENGTH = 128;

const SETS: CharSet[] = ['lowercase', 'uppercase', 'digits', 'symbols'];

/** How many of a set the password gets at least (each set that is on: one). */
export function minimumOf(options: WithMinimums, set: CharSet): number {
  return Math.max(1, Math.floor(options[MIN_KEY[set]] ?? 0));
}

/** What the minimums of the sets that are on add up to (lower case alone when none is). */
export function required(options: WithMinimums): number {
  const on = SETS.filter((set) => options[set]);
  return (on.length ? on : (['lowercase'] as CharSet[])).reduce(
    (sum, set) => sum + minimumOf(options, set),
    0,
  );
}

/** The length the password gets: raised to the minimums when they need more. */
export function effectiveLength(options: WithMinimums): number {
  return Math.min(MAX_LENGTH, Math.max(5, options.length, required(options)));
}

/**
 * `options` with the minimum of `set` at `value`, kept so the minimums of the
 * sets that are on fit into {@link MAX_LENGTH} characters together.
 */
export function withMinimum<T extends WithMinimums>(options: T, set: CharSet, value: number): T {
  const others = required({ ...options, [set]: true, [MIN_KEY[set]]: 1 }) - 1;
  const clean = Number.isFinite(value) ? Math.floor(value) : 1;
  const max = Math.max(1, MAX_LENGTH - others);
  return { ...options, [MIN_KEY[set]]: Math.min(Math.max(clean, 1), max) };
}

/** Stored minimums, checked: whole numbers 0–128, anything else left out. */
export function cleanMinimums(raw: Record<string, unknown>): Partial<Record<MinKey, number>> {
  const out: Partial<Record<MinKey, number>> = {};
  for (const key of Object.values(MIN_KEY)) {
    const value = raw[key];
    if (typeof value === 'number' && Number.isFinite(value))
      out[key] = Math.min(MAX_LENGTH, Math.max(0, Math.floor(value)));
  }
  return out;
}
