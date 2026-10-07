import { Button } from '@uwusuite/design';
import { useEffect, useState, type ReactElement, type ReactNode } from 'react';
import {
  AUTOFILL_CHANGED,
  autofillProviderRequest,
  providerSettingsPath,
  providerStateText,
  useProviderStatus,
} from '../lib/autofill';
import { errorText } from '../lib/errors';
import { t, useLanguage } from '../lib/i18n';
import {
  onPasskeyProviderWarning,
  passkeyProviderStatus,
  setPasskeyProvider,
  warningText,
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
    const load = () =>
      void passkeyProviderStatus()
        .then(setStatus)
        .catch(() => setStatus(null));
    load();
    const stop = onPasskeyProviderWarning(load);
    return () => void stop.then((unlisten) => unlisten());
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
  const warning = on && status.warning ? warningText(status.warning) : null;

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
            t('Passwörter und Passkeys für AutoFill'),
            t(
              'Hinterlegt Passwörter und Passkeys versiegelt für UwULocks AutoFill-Erweiterung; die öffnet sie nur nach Face ID, Touch ID oder dem Gerätecode. Logins mit erneuter Master-Passwort-Abfrage bleiben draußen.',
            ),
          ];

  return (
    <>
      {status.platform === 'apple' && <ProviderRow Row={Row} />}
      <Row
        label={label}
        description={
          <>
            {description}
            {warning && (
              <>
                <br />
                <span className="form-error" role="alert">
                  {warning}
                </span>
              </>
            )}
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
    </>
  );
}

/**
 * macOS (the Mac App Store build, which has the extension): whether UwULock is the AutoFill
 * provider in System Settings, and the button that asks macOS (15+) or opens those settings.
 */
function ProviderRow({ Row }: { Row: Row }) {
  useLanguage();
  const [view, reload] = useProviderStatus();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!view?.supported || view.platform !== 'macos') return null;
  const ask = () => {
    setBusy(true);
    setError(null);
    void autofillProviderRequest('credentials')
      .then(() => {
        window.dispatchEvent(new Event(AUTOFILL_CHANGED));
        reload();
      })
      .catch((failed) => setError(errorText(failed)))
      .finally(() => setBusy(false));
  };
  return (
    <Row
      label={t('Standard für AutoFill')}
      description={
        <>
          {providerStateText(view)}
          {view.enabled !== true && (
            <>
              {' · '}
              {t('UwULock dort einschalten: {path}', { path: providerSettingsPath(view) })}
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
      {view.enabled === true ? (
        <span />
      ) : (
        <Button size="sm" busy={busy} onClick={ask}>
          {view.direct ? t('Als Standard festlegen') : t('Einstellungen öffnen')}
        </Button>
      )}
    </Row>
  );
}
