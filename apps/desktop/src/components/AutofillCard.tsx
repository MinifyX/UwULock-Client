/**
 * Nyu's card after an unlock while UwULock isn't the system's AutoFill provider: "Als Standard
 * festlegen" opens the system's own flow (iOS 18+ and macOS 15+ ask in a sheet, older systems
 * open the AutoFill settings, Android its provider sheets). "Später" waits a week, at most three
 * times (lib/autofillPrompt.ts). iPhone, iPad, Android and the Mac App Store build; never on
 * Windows or Linux.
 */

import { Button } from '@uwusuite/design';
import { useState } from 'react';
import {
  AUTOFILL_CHANGED,
  autofillProviderRequest,
  missing,
  providerSettingsPath,
  useAutofillPrompt,
} from '../lib/autofill';
import { errorText } from '../lib/errors';
import { t, useLanguage } from '../lib/i18n';
import { Nyu } from './nyu/Nyu';

export function AutofillCard({ unlocked }: { unlocked: boolean }) {
  useLanguage();
  const { view, show, later, reload } = useAutofillPrompt(unlocked);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!show || !view) return null;
  const target = missing(view) ?? 'credentials';

  const [title, text] =
    target === 'autofill'
      ? [
          t('Auch in Apps ohne Passkey-Support ausfüllen'),
          t(
            'Als Autofill-Dienst füllt UwULock Logins auch in Apps und Browsern aus, die Androids Anmeldeverwaltung nicht nutzen.',
          ),
        ]
      : [
          t('UwULock als Standard für AutoFill'),
          t(
            'Dann schlägt dein System UwULocks Passwörter und Passkeys direkt beim Anmelden vor – in Apps und im Browser. Jedes Ausfüllen fragt nach Face ID, Touch ID oder deiner Gerätesperre.',
          ),
        ];

  const turnOn = async () => {
    setBusy(true);
    setError(null);
    try {
      const after = await autofillProviderRequest(target);
      window.dispatchEvent(new Event(AUTOFILL_CHANGED));
      // Declined in the system's own sheet (or the settings opened and nothing changed yet):
      // counts as "Später", so the card keeps to its week and its three times.
      if (missing(after) !== null) later();
      else reload();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <aside className="update-hint autofill-card" aria-live="polite">
      <div className="update-hint-head">
        <Nyu size={40} mood="sparkle" title="Nyu" />
        <div>
          <p className="update-hint-title">{title}</p>
          <p className="update-hint-meta">{text}</p>
        </div>
      </div>
      {!view.direct && (
        <p className="update-hint-meta">
          {t('UwULock dort einschalten: {path}', { path: providerSettingsPath(view) })}
        </p>
      )}
      {error && (
        <p className="update-hint-warning" role="alert">
          {error}
        </p>
      )}
      <div className="update-hint-actions">
        <Button variant="ghost" size="sm" onClick={later}>
          {t('Später')}
        </Button>
        <Button variant="primary" size="sm" busy={busy} onClick={() => void turnOn()}>
          {view.direct ? t('Als Standard festlegen') : t('Einstellungen öffnen')}
        </Button>
      </div>
    </aside>
  );
}
