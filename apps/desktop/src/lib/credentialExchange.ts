/**
 * iOS 26: what Apple Passwords handed over, on its way from Nyu's card
 * (components/CredentialExchangeCard.tsx) to the phone's import page
 * (Settings → Importieren), which needs the phone's navigation to open. The
 * card leaves it here and says so; the page takes it into its preview. Only
 * in memory, and gone once the page has it.
 */

import type { Parsed } from './import/types';

export const HANDED_OVER = 'uwulock:credential-exchange';

let waiting: { generation: number; parsed: Parsed } | null = null;
let generation = 0;

/** Leaves `parsed` for the import page and asks the phone layout to open it. */
export function handOver(parsed: Parsed) {
  generation += 1;
  waiting = { generation, parsed };
  window.dispatchEvent(new Event(HANDED_OVER));
}

/** What waits, with its number (a new hand-over starts the page over). */
export function handedOver(): { generation: number; parsed: Parsed } | null {
  return waiting;
}

/** The page has it: nothing waits any more. */
export function forget(taken: number) {
  if (waiting?.generation === taken) waiting = null;
}
