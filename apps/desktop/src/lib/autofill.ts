/**
 * UwULock as the system's AutoFill provider for passwords and passkeys (src-tauri/src/autofill.rs):
 * whether it is, asking the system to make it so, and the card that asks after an unlock
 * (lib/autofillPrompt.ts decides when). iOS, iPadOS, macOS (builds with the extension) and
 * Android; Windows and Linux have no such provider.
 */

import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useState } from 'react';
import {
  PROMPT_KEY,
  promptLater,
  promptSeen,
  readPrompt,
  shouldPrompt,
  type PromptState,
  type ProviderView,
} from './autofillPrompt';
import { t } from './i18n';

export type { ProviderView } from './autofillPrompt';
export { hasCredentialManager, missing } from './autofillPrompt';

export const autofillProviderStatus = () => invoke<ProviderView>('autofill_provider_status');

/** `credentials`: passwords and passkeys; `autofill`: Android's autofill service. */
export const autofillProviderRequest = (target: 'credentials' | 'autofill') =>
  invoke<ProviderView>('autofill_provider_request', { target });

/** Something changed the provider or UwULock's own switch: settings and the card reload. */
export const AUTOFILL_CHANGED = 'uwulock-autofill-changed';

export function useProviderStatus(active = true): [ProviderView | null, () => void] {
  const [view, setView] = useState<ProviderView | null>(null);
  const load = useCallback(() => {
    void autofillProviderStatus()
      .then(setView)
      .catch(() => setView(null));
  }, []);
  useEffect(() => {
    if (!active) return;
    load();
    // Back from the system's settings: ask again.
    const visible = () => document.visibilityState === 'visible' && load();
    window.addEventListener(AUTOFILL_CHANGED, load);
    document.addEventListener('visibilitychange', visible);
    return () => {
      window.removeEventListener(AUTOFILL_CHANGED, load);
      document.removeEventListener('visibilitychange', visible);
    };
  }, [active, load]);
  return [view, load];
}

function readState(): PromptState {
  try {
    return readPrompt(localStorage.getItem(PROMPT_KEY));
  } catch {
    return readPrompt(null);
  }
}

function writeState(state: PromptState) {
  try {
    localStorage.setItem(PROMPT_KEY, JSON.stringify(state));
  } catch {
    // Without storage the card simply asks again next time.
  }
}

/** The card's state while the vault is open: whether it shows, and its two answers. */
export function useAutofillPrompt(unlocked: boolean) {
  const [view, load] = useProviderStatus(unlocked);
  const [state, setState] = useState(readState);
  useEffect(() => {
    const seen = promptSeen(state, view);
    if (seen !== state) {
      writeState(seen);
      setState(seen);
    }
  }, [state, view]);
  const later = () => {
    const next = promptLater(state, Date.now());
    writeState(next);
    setState(next);
  };
  return {
    view,
    show: unlocked && shouldPrompt(state, view, Date.now()),
    later,
    reload: load,
  };
}

/** The settings' words for the provider's state. */
export function providerStateText(view: ProviderView): string {
  if (!view.supported) return t('Nicht verfügbar');
  if (view.enabled === true) return t('Eingeschaltet');
  if (view.enabled === false) return t('Ausgeschaltet');
  return t('Unbekannt');
}

/** Where the person switches it on by hand, for the hint under the button. */
export function providerSettingsPath(view: ProviderView): string {
  switch (view.platform) {
    case 'ios':
      return t('Einstellungen → Allgemein → AutoFill & Passwörter');
    case 'macos':
      return t('Systemeinstellungen → Allgemein → AutoFill & Passwörter');
    case 'android':
      return t('Einstellungen → Passwörter, Passkeys und Konten');
    default:
      return '';
  }
}
