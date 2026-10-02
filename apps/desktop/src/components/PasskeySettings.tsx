import { useEffect, useState, type ReactElement, type ReactNode } from 'react';
import { errorText } from '../lib/errors';
import { t, useLanguage } from '../lib/i18n';
import {
  passkeyProviderStatus,
  setPasskeyProvider,
  type PasskeyProviderSettings,
  type PasskeyProviderStatus,
} from '../lib/passkeys';

type Row = (props: { label: string; description?: ReactNode; children: ReactNode }) => ReactElement;
type Toggle = (props: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) => ReactElement;

/**
 * The switch for passkeys in other apps and browsers on this system
 * (docs/passkeys.md). Android's provider is switched on in Android's own
 * settings, so there is only a hint; systems without one show nothing.
 */
export function PasskeySettings({ Row, Toggle }: { Row: Row; Toggle: Toggle }) {
  useLanguage();
  const [status, setStatus] = useState<PasskeyProviderStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void passkeyProviderStatus()
      .then(setStatus)
      .catch(() => setStatus(null));
  }, []);

  if (!status || status.platform === 'none') return null;

  const change = (next: Partial<PasskeyProviderSettings>) => {
    setError(null);
    void setPasskeyProvider({ ...status.settings, ...next })
      .then(setStatus)
      .catch((failed) => setError(errorText(failed)));
  };

  const on =
    status.platform === 'linux'
      ? status.settings.securityKey
      : status.platform === 'windows'
        ? status.settings.windowsPlugin
        : status.settings.appleExtension;
  const problem = on && status.problem ? status.problem : null;

  if (status.platform === 'android') {
    return (
      <Row
        label={t('Passkeys in anderen Apps')}
        description={t(
          'Ab Android 14: in Android unter Einstellungen → Passwörter, Passkeys und Konten UwULock wählen.',
        )}
      >
        <span />
      </Row>
    );
  }

  const [label, description] =
    status.platform === 'linux'
      ? [
          t('Passkeys in Browsern (Sicherheitsschlüssel)'),
          t(
            'UwULock meldet sich als FIDO2-Sicherheitsschlüssel an. Browser fragen ihn wie einen USB-Schlüssel, und UwULock fragt jedes Mal dich.',
          ),
        ]
      : status.platform === 'windows'
        ? [
            t('Passkeys in Windows (experimentell)'),
            t(
              'UwULock als Passkey-Manager in Windows 11, wo Windows Passkey-Manager anderer Anbieter zulässt. Danach in Windows unter Einstellungen → Konten → Passkeys einschalten.',
            ),
          ]
        : [
            t('Passkeys in anderen Apps (AutoFill)'),
            t(
              'Hinterlegt die Passkeys versiegelt für UwULocks AutoFill-Erweiterung; die öffnet sie nur nach Face ID, Touch ID oder dem Gerätecode.',
            ),
          ];

  return (
    <Row
      label={label}
      description={
        <>
          {description}
          {problem && (
            <>
              <br />
              <span className="form-error">{problem}</span>
            </>
          )}
          {error && (
            <>
              <br />
              <span className="form-error">{error}</span>
            </>
          )}
        </>
      }
    >
      <Toggle
        label={label}
        checked={on}
        onChange={(checked) =>
          change(
            status.platform === 'linux'
              ? { securityKey: checked }
              : status.platform === 'windows'
                ? { windowsPlugin: checked }
                : { appleExtension: checked },
          )
        }
      />
    </Row>
  );
}
