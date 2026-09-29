// @vitest-environment node
/** Locking with the computer (background/idle.ts): the screen lock closes an open vault, if the setting says so. */

import { beforeEach, describe, expect, it, vi } from 'vitest';

let unlocked: string | null = 'user-1';
let lockWithSystem = true;

vi.mock('../src/shared/browser', () => ({ ext: {} }));
vi.mock('../src/background/session', () => ({
  restored: Promise.resolve(),
  unlockedAccountId: () => unlocked,
}));
vi.mock('../src/background/settings', () => ({
  settings: async () => ({ lockWithSystem }),
}));

const { onIdleState } = await import('../src/background/idle');

describe('locking with the computer', () => {
  const lock = vi.fn(async () => undefined);
  beforeEach(() => {
    unlocked = 'user-1';
    lockWithSystem = true;
    lock.mockClear();
  });

  it('locks when the screen is locked', async () => {
    await onIdleState('locked', lock);
    expect(lock).toHaveBeenCalledOnce();
  });

  it('does nothing for idle or active', async () => {
    await onIdleState('idle', lock);
    await onIdleState('active', lock);
    expect(lock).not.toHaveBeenCalled();
  });

  it('follows the setting', async () => {
    lockWithSystem = false;
    await onIdleState('locked', lock);
    expect(lock).not.toHaveBeenCalled();
  });

  it('leaves a locked vault alone', async () => {
    unlocked = null;
    await onIdleState('locked', lock);
    expect(lock).not.toHaveBeenCalled();
  });
});
