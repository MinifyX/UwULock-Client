/**
 * Short notes: "Copied", "Synced". @uwusuite/design's toasts (`<Toaster>` in
 * App.tsx, bottom centre, at the top on a phone), one at a time — a new one
 * replaces the old — and quicker than the package's default, because most of
 * them answer a click that was just made.
 */

import { createToasts } from '@uwusuite/design';

export const toasts = createToasts({ infoMs: 2600, errorMs: 6000, max: 1 });

export function toast(text: string, tone: 'info' | 'error' = 'info') {
  toasts.show(text, { tone });
}
