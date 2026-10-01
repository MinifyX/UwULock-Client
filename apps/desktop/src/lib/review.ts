/**
 * The review one login at a time, without React: moving through the stack,
 * what counts as a swipe, putting a card off for later. Kept apart so it is
 * tested on its own (`apps/desktop/test/review.test.ts`, plain `node --test`).
 * No imports with effects in here: Node runs this file as it is.
 */

/** How far a card has to be dragged to count as a swipe. */
export const SWIPE = 80;

/** How far a pointer moves sideways before it is a drag and not a tap. */
export const DRAG_START = 10;

/** `index` within a stack of `total` cards. */
export function clampIndex(index: number, total: number): number {
  return Math.min(Math.max(index, 0), Math.max(total - 1, 0));
}

/** The card `delta` away, never past either end. */
export function step(index: number, delta: number, total: number): number {
  return clampIndex(index + delta, total);
}

/** ← is the previous card, → the next; anything else nothing. */
export function keyStep(key: string): -1 | 0 | 1 {
  if (key === 'ArrowRight') return 1;
  if (key === 'ArrowLeft') return -1;
  return 0;
}

/** Whether a pointer that moved by (dx, dy) is dragging the card sideways. */
export function dragStarts(dx: number, dy: number): boolean {
  return Math.abs(dx) > DRAG_START && Math.abs(dx) > Math.abs(dy);
}

/**
 * Where a card let go after a drag of `dx` goes: to the left is the next one
 * (as on a pile of cards), to the right the previous; a short drag stays.
 */
export function swipeStep(dx: number): -1 | 0 | 1 {
  if (dx <= -SWIPE) return 1;
  if (dx >= SWIPE) return -1;
  return 0;
}

/** How a card is drawn while it is dragged. */
export function dragTransform(dx: number): string {
  return `translateX(${dx}px) rotate(${dx / 40}deg)`;
}

type Carded = { finding: { id: string } };

/** The stack without the cards put off for later. */
export function withoutLater<T extends Carded>(cards: T[], later: ReadonlySet<string>): T[] {
  return cards.filter((card) => !later.has(card.finding.id));
}

/**
 * "Later": the card leaves the stack for this session; the one after it
 * takes its place (or the one before, at the end).
 */
export function skipCard<T extends Carded>(
  cards: T[],
  index: number,
): { cards: T[]; index: number; skipped: string | null } {
  const card = cards[index];
  if (!card) return { cards, index: clampIndex(index, cards.length), skipped: null };
  const rest = cards.filter((_, at) => at !== index);
  return { cards: rest, index: clampIndex(index, rest.length), skipped: card.finding.id };
}

/** The problem ids every UwULock app uses (UwULock-Server's docs/uwu-api.md §15.6). */
export type ProblemKind = 'breached' | 'siteBreach' | 'reused' | 'weak' | 'unsecured' | 'twofa';

/** Whether a new password solves the problem. */
export const aboutThePassword = (kind: ProblemKind) =>
  kind === 'breached' || kind === 'siteBreach' || kind === 'reused' || kind === 'weak';

export type IgnoredEntry = { itemId: string; kind: ProblemKind; since: string };

export const isIgnored = (
  list: readonly IgnoredEntry[] | null,
  itemId: string,
  kind: ProblemKind,
) => Boolean(list?.some((entry) => entry.itemId === itemId && entry.kind === kind));

/** "3 of 12": the card on screen counted from one, never past the total. */
export function progress(index: number, total: number): { n: number; total: number } {
  return { n: total ? Math.min(index + 1, total) : 0, total };
}

/** The ids put off for later, as the session keeps them. */
export function parseLater(text: string | null): Set<string> {
  try {
    const ids: unknown = JSON.parse(text ?? '[]');
    return new Set(Array.isArray(ids) ? ids.filter((id) => typeof id === 'string') : []);
  } catch {
    return new Set();
  }
}
