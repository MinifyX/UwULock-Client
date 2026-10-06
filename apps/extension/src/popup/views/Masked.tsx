import { useEffect, useState } from 'react';
import { Icon } from '../../legacy/Icon';
import { t } from '../../shared/i18n';
import type { MaskedAddress, MaskedConnection } from '../../shared/protocol';
import { copyText, createMasked, maskedConnection } from '../api';
import { errorText, toast, toastError } from '../lib';

/**
 * The generator's masked addresses: a new address from the account's UwUMail for the site in
 * the tab, made only on a click (every address stays at UwUMail until it is deleted there).
 */
export function MaskedPanel() {
  const [connection, setConnection] = useState<MaskedConnection | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<MaskedAddress | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void maskedConnection().then(setConnection, (e) => setError(errorText(e)));
  }, []);

  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      setCreated(await createMasked(null));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const copy = async (email: string) => {
    try {
      await copyText(email);
      toast(`${t('Kopiert')} ✧`);
    } catch (e) {
      toastError(e);
    }
  };

  if (!connection && !error) return <div aria-busy />;

  const usable = connection?.connected && connection.status !== 'revoked';
  return (
    <div className="setting-list">
      {connection && !usable && (
        <p className="notice">
          {connection.connected
            ? t(
                'Die Verbindung zu UwUMail wurde beendet. Verbinde dein Konto im Web-Tresor noch einmal.',
              )
            : t(
                'Verbinde dein Konto zuerst im Web-Tresor mit UwUMail, dann legt UwULock hier maskierte Adressen an.',
              )}{' '}
          <a href={connection.settingsUrl} target="_blank" rel="noreferrer">
            {t('Im Web-Tresor verbinden')}
          </a>
        </p>
      )}
      {usable && (
        <p className="dialog-lead">
          {t(
            'Eine neue Adresse bei {server} für die Seite in diesem Tab. Mails an sie landen bei {user}.',
            {
              server: connection.server ?? 'UwUMail',
              user: connection.username ?? '…',
            },
          )}
        </p>
      )}
      {created && (
        <div className="masked-result">
          <div className="generated mono">{created.email}</div>
          <button
            type="button"
            className="icon-button"
            onClick={() => void copy(created.email)}
            aria-label={t('Kopieren')}
            title={t('Kopieren')}
          >
            <Icon name="copy" size={15} />
          </button>
        </div>
      )}
      {error && <p className="form-error">{error}</p>}
      {usable && (
        <div className="form-actions">
          <span className="spacer" />
          <button type="button" className="primary" disabled={busy} onClick={() => void create()}>
            {busy ? t('Legt an …') : t('Neue maskierte Adresse')}
          </button>
        </div>
      )}
    </div>
  );
}
