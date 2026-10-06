import { Button, ICONS } from '@uwusuite/design';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { failure, logout, switchAccount, unlock, unlockWithHello, type Status } from '../lib/api';
import { errorText, toastError } from '../lib/errors';
import { ago } from '../lib/format';
import { t, useLanguage } from '../lib/i18n';
import { isMobile } from '../lib/platform';
import { unlockLabel, unlockPrompt } from '../lib/unlock';
import { Modal } from './Modal';
import { NyuScene } from './nyu/scenes';
import { PasswordInput } from './PasswordInput';

type Props = {
  status: Status;
  onUnlocked: (status: Status) => void;
  onLoggedOut: () => void;
  onAddAccount: () => void;
};

/**
 * The locked vault. The master password opens it right here, from the copy
 * on this device — no server needed; the sync follows in the background.
 *
 * With more than one account, the others are a click away: an account that is
 * still open shows its vault straight away, a locked one asks here.
 */
export function LockScreen({ status, onUnlocked, onLoggedOut, onAddAccount }: Props) {
  useLanguage();
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmLogout, setConfirmLogout] = useState(false);
  const others = status.accounts.filter((account) => !account.active);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const next = await unlock(password);
      setPassword('');
      onUnlocked(next);
    } catch (e) {
      setError(errorText(e));
      setPassword('');
    } finally {
      setBusy(false);
    }
  };

  const hello = async () => {
    setBusy(true);
    setError(null);
    try {
      onUnlocked(await unlockWithHello(unlockPrompt()));
    } catch (e) {
      // Cancelled on purpose: the master password field is right there.
      if (failure(e).kind !== 'biometric-cancelled') setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  // On a phone the fingerprint or face is asked for right away, once per
  // account, the way phone apps do; the master password stays below it.
  const asked = useRef<string | null>(null);
  useEffect(() => {
    if (!isMobile() || !status.hello || asked.current === status.accountId) return;
    asked.current = status.accountId;
    void hello();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status.hello, status.accountId]);

  return (
    <div className="lock">
      <form className="lock-card" onSubmit={submit} aria-busy={busy}>
        <NyuScene name="sleepy" className="lock-scene" />
        <h1 className="card-title">{t('Dein Tresor ist gesperrt')}</h1>
        <p className="lock-account">
          <b>{status.email}</b>
          <span>
            {status.label !== status.server ? `${status.label} · ` : ''}
            {status.server}
          </span>
        </p>
        <label className="field">
          <span>{t('Master-Passwort')}</span>
          <PasswordInput
            value={password}
            onChange={setPassword}
            autoFocus={!(isMobile() && status.hello)}
            disabled={busy}
          />
        </label>
        {error && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
        <Button variant="primary" size="lg" type="submit" disabled={busy || !password}>
          {busy ? t('Entsperrt …') : t('Entsperren')}
        </Button>
        {status.hello && (
          <Button size="lg" disabled={busy} onClick={() => void hello()}>
            {unlockLabel(status.helloKind)}
          </Button>
        )}
        <p className="lock-meta">
          {t('Zuletzt synchronisiert: {when}', { when: ago(status.lastSync) })}
          {' · '}
          <button type="button" className="link-button" onClick={() => setConfirmLogout(true)}>
            {t('Abmelden')}
          </button>
        </p>

        <div className="mt-1 grid gap-1">
          {others.map((account) => (
            <Button
              key={account.id}
              variant="ghost"
              size="sm"
              icon={account.unlocked ? ICONS.unlocked : ICONS.locked}
              className="w-full justify-start!"
              onClick={() => void switchAccount(account.id).catch((e) => toastError(e))}
            >
              <span className="truncate">{account.label}</span>
              <small className="ml-auto text-caption text-muted">
                {account.unlocked ? t('offen') : t('gesperrt')}
              </small>
            </Button>
          ))}
          <Button
            variant="ghost"
            size="sm"
            icon={ICONS.add}
            className="w-full justify-start!"
            onClick={onAddAccount}
          >
            <span>{t('Konto hinzufügen')}</span>
          </Button>
        </div>
      </form>

      {confirmLogout && (
        <Modal
          title={t('Von diesem Gerät abmelden?')}
          size="small"
          onCancel={() => setConfirmLogout(false)}
          footer={
            <>
              <span className="spacer" />
              <Button
                variant="danger"
                data-secondary
                onClick={async () => {
                  try {
                    await logout();
                    onLoggedOut();
                  } catch (e) {
                    setError(errorText(e));
                  }
                  setConfirmLogout(false);
                }}
              >
                {t('Abmelden')}
              </Button>
              <Button variant="primary" data-autofocus onClick={() => setConfirmLogout(false)}>
                {t('Abbrechen')}
              </Button>
            </>
          }
        >
          <p className="dialog-lead">
            {t(
              'Die Anmeldung und die verschlüsselte Kopie des Tresors werden von diesem Gerät gelöscht. Dein Tresor auf dem Server bleibt, wie er ist.',
            )}
          </p>
        </Modal>
      )}
    </div>
  );
}
