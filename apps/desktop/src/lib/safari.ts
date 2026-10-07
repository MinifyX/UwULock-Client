/**
 * The Safari extension the Apple builds carry (src-tauri/src/safari.rs, docs/extension.md): the
 * same extension as in Chrome and Firefox, with its own login. The app only says whether it is
 * switched on and, on macOS, opens Safari's settings on it; iOS has no way to ask.
 */

import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useState } from 'react';

export interface SafariView {
  platform: 'macos' | 'ios' | 'none';
  /** This build carries the extension. */
  available: boolean;
  /** Switched on in Safari; null when Safari doesn't say (iOS). */
  enabled: boolean | null;
}

export const safariExtensionStatus = () => invoke<SafariView>('safari_extension_status');
export const safariExtensionOpen = () => invoke<void>('safari_extension_open');

export function useSafariStatus(): [SafariView | null, () => void] {
  const [view, setView] = useState<SafariView | null>(null);
  const load = useCallback(() => {
    void safariExtensionStatus()
      .then(setView)
      .catch(() => setView(null));
  }, []);
  useEffect(() => {
    load();
    // Back from Safari's settings: ask again.
    window.addEventListener('focus', load);
    return () => window.removeEventListener('focus', load);
  }, [load]);
  return [view, load];
}
