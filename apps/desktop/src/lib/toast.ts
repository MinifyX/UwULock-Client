/**
 * Short notes: "Copied", "Synced". @uwusuite/design's toasts (`<Toaster>` in
 * App.tsx on a computer, `<MobileToaster>` on phones and iPads: a glass toast
 * at the top, Android's snackbar at the bottom), one at a time — a new one
 * replaces the old — and quicker than the package's default, because most of
 * them answer a click that was just made.
 */

import { createToasts } from '@uwusuite/design';
import type { ReactNode } from 'react';

export const toasts = createToasts({ infoMs: 2600, errorMs: 6000, max: 1 });

export function toast(text: string, tone: 'info' | 'error' = 'info') {
  toasts.show(text, { tone });
}

export type NoteOptions = {
  tone?: 'info' | 'success' | 'error';
  /** A second line ("Wird nach 30 s geleert"). */
  detail?: ReactNode;
  /** One action, e.g. "Rückgängig" (the snackbar's button on Android). */
  action?: { label: string; run: () => void };
};

/** A toast with a second line or an action. */
export function note(text: string, options: NoteOptions = {}) {
  toasts.show(text, {
    tone: options.tone ?? 'info',
    detail: options.detail,
    action: options.action,
  });
}
