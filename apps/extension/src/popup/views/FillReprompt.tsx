import { useState, type FormEvent } from 'react';
import { Icon } from '@desktop/components/Icon';
import { t } from '../../shared/i18n';
import { errorText, PasswordInput } from '../lib';

/**
 * An item with the re-prompt asks for the master password before every fill, as Bitwarden
 * does: an answer given once doesn't fill it again (security review 0.3, CL-I3).
 */
export function FillReprompt({
  name,
  onFill,
  onCancel,
}: {
  name: string;
  /** Fills with the password; throws what went wrong. */
  onFill: (password: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await onFill(password);
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
    }
  };
  return (
    <form className="detail-card reprompt form fill-reprompt" onSubmit={submit}>
      <h3 className="detail-card-title">
        <Icon name="shield" size={13} /> {t('Master-Passwort bestätigen')}
      </h3>
      <p className="dialog-lead">
        {t(
          '{name} ist geschützt: Zum Ausfüllen fragt UwULock jedes Mal nach dem Master-Passwort.',
          {
            name: name || t('(ohne Namen)'),
          },
        )}
      </p>
      <PasswordInput value={password} onChange={setPassword} autoFocus disabled={busy} />
      {error && <p className="form-error">{error}</p>}
      <div className="form-actions">
        <button type="button" className="quiet" onClick={onCancel} disabled={busy}>
          {t('Abbrechen')}
        </button>
        <span className="spacer" />
        <button className="primary" type="submit" disabled={busy || !password}>
          {t('Ausfüllen')}
        </button>
      </div>
    </form>
  );
}
