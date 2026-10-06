import { useSyncExternalStore } from 'react';

/**
 * Phone width: one pane at a time instead of the three side by side — the
 * list, the item over it, the folders in a drawer. Every phone gets it, and so
 * does a very narrow window (the desktop window can't get that narrow). Below
 * 700 px, the same as @uwusuite/design's `phone:` variant.
 */
const QUERY = '(max-width: 699px)';

function query(): MediaQueryList | null {
  return typeof window !== 'undefined' && window.matchMedia ? window.matchMedia(QUERY) : null;
}

function subscribe(onChange: () => void) {
  const list = query();
  list?.addEventListener('change', onChange);
  return () => list?.removeEventListener('change', onChange);
}

export function usePhoneLayout(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => query()?.matches ?? false,
    () => false,
  );
}
