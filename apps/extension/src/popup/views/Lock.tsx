import { Button } from '@uwusuite/design';
import { useState, type FormEvent } from 'react';
import { NyuScene } from '@desktop/components/nyu/scenes';
import { t } from '../../shared/i18n';
import type { Status } from '../../shared/protocol';
import { logout, switchAccount, unlock, unlockWithPin } from '../api';
import { errorText, PasswordInput } from '../lib';

/**
 * The vault is locked: the master password — or the PIN, if one is set — opens it again. The
 * other accounts on this browser are one click away.
 */
export function LockView({
  status,
  onDone,
  onAddAccount,
  compact,
}: {
  status: Status;
  onDone: () => void;
  onAddAccount?: () => void;
  /** In the passkey window: no account switching. */
  compact?: boolean;
}) {
  const [usePin, setUsePin] = useState(status.pinSet);
  const [secret, setSecret] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      if (usePin) await unlockWithPin(secret);
      else await unlock(secret);
      setSecret('');
      onDone();
    } catch (e) {
      const kind = (e as { kind?: string }).kind;
      setError(kind === 'wrong-password' && usePin ? t('Die PIN ist falsch.') : errorText(e));
      if (kind === 'pin-cleared') setUsePin(false);
    } finally {
      setBusy(false);
    }
  };

  const others = status.accounts.filter((a) => !a.active);

  return (
    <div className="popup-scroll lock-view">
      {!compact && <NyuScene name="sleepy" className="lock-scene" />}
      <form className="form" onSubmit={submit}>
        <h1 className="card-title">{t('Tresor gesperrt')}</h1>
        <p className="lock-account">
          <strong>{status.email}</strong>
          <span className="muted"> · {status.server}</span>
        </p>
        <label className="field">
          <span>{usePin ? t('PIN') : t('Master-Passwort')}</span>
          <PasswordInput
            value={secret}
            onChange={setSecret}
            autoFocus
            disabled={busy}
            inputMode={usePin ? 'numeric' : undefined}
          />
        </label>
        {error && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
        <div className="form-actions">
          {status.pinSet && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setUsePin(!usePin);
                setSecret('');
                setError(null);
              }}
            >
              {usePin ? t('Master-Passwort verwenden') : t('PIN verwenden')}
            </Button>
          )}
          <span className="spacer" />
          <Button variant="primary" type="submit" busy={busy} disabled={!secret}>
            {busy ? t('Entsperrt …') : t('Entsperren')}
          </Button>
        </div>
      </form>
      {!compact && (
        <div className="lock-more">
          {others.map((account) => (
            <Button
              key={account.id}
              variant="ghost"
              size="sm"
              onClick={() =>
                void switchAccount(account.id).then(onDone, (e) => setError(errorText(e)))
              }
            >
              {t('Zu {email} wechseln', { email: account.email })}
            </Button>
          ))}
          {onAddAccount && (
            <Button variant="ghost" size="sm" onClick={onAddAccount}>
              {t('Anderes Konto hinzufügen')}
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void logout().then(onDone, (e) => setError(errorText(e)))}
          >
            {t('Abmelden')}
          </Button>
        </div>
      )}
    </div>
  );
}
