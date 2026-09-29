/**
 * The computer's screen was locked: the vault locks with it, like the desktop app's "lock with
 * the computer" (a setting, on by default). The lock timeout alone would go on counting through
 * the time away, sleep included. The browser reports the state (`idle` permission); where it
 * never reports `locked`, only the timeout applies.
 */

import { ext } from '../shared/browser';
import * as session from './session';
import { settings } from './settings';

export async function onIdleState(state: string, lock: () => Promise<void>): Promise<void> {
  if (state !== 'locked') return;
  await session.restored;
  if (!session.unlockedAccountId() || !(await settings()).lockWithSystem) return;
  await lock();
}

/** Added at the top level of the background, as Manifest V3 wants its listeners. */
export function watchIdle(lock: () => Promise<void>) {
  ext.idle?.onStateChanged.addListener((state) => {
    void onIdleState(state, lock).catch(() => undefined);
  });
}
