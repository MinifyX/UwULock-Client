import { useEffect, useState } from 'react';
import { Icon } from '@desktop/components/Icon';
import { t } from '../../shared/i18n';
import type { FileRequestEntry, FileRequests } from '../../shared/protocol';
import { copyFileRequestLink, fileRequests } from '../api';
import { errorText, toast, toastError, useSettings, when } from '../lib';
import { BackBar } from './Detail';

function state(request: FileRequestEntry): string {
  if (request.disabled) return t('Ausgeschaltet');
  if (request.expired) return t('Abgelaufen');
  if (request.maxSubmissions !== null && request.submissionCount >= request.maxSubmissions)
    return t('Voll');
  return request.expirationDate
    ? t('Bis {date}', { date: when(request.expirationDate) ?? '' })
    : t('Offen');
}

/**
 * The owner's file requests, read only: label, until when, what arrived, and the link to copy
 * again. Making, changing and reading what arrived happens in the web vault.
 */
export function FileRequestsView({ onBack }: { onBack: () => void }) {
  const settings = useSettings();
  const [found, setFound] = useState<FileRequests | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void fileRequests().then(setFound, (e) => setError(errorText(e)));
  }, []);

  const copy = async (id: string) => {
    try {
      await copyFileRequestLink(id);
      const seconds = settings?.clipboardClear ?? 0;
      toast(
        seconds > 0
          ? t('{what} ✧ – wird nach {n} s geleert', { what: t('Link kopiert'), n: seconds })
          : `${t('Link kopiert')} ✧`,
      );
    } catch (e) {
      toastError(e);
    }
  };

  return (
    <div className="popup-scroll">
      <BackBar onBack={onBack}>
        {found && (
          <a className="quiet" href={found.webUrl} target="_blank" rel="noreferrer">
            {t('Im Web-Tresor verwalten')} <Icon name="external" size={13} />
          </a>
        )}
      </BackBar>
      <h2 className="card-title">{t('Dateianfragen')}</h2>
      {error && <p className="form-error">{error}</p>}
      {!found && !error && <div aria-busy />}
      {found?.state === 'none' && (
        <p className="notice">
          {t(
            'Für Dateianfragen braucht dein Konto einen Schlüssel für UwULocks Extras. Leg deine erste Dateianfrage im Web-Tresor an.',
          )}
        </p>
      )}
      {found?.state === 'lost' && (
        <p className="notice" data-tone="error">
          {t(
            'Der Schlüssel für UwULocks Extras lässt sich nicht mehr öffnen (neues Schlüsselpaar). Im Web-Tresor kannst du neu anfangen.',
          )}
        </p>
      )}
      {found?.state === 'open' && found.requests.length === 0 && (
        <p className="empty-line">{t('Noch keine Dateianfragen.')}</p>
      )}
      {found?.state === 'open' && found.requests.length > 0 && (
        <ul className="item-list plain">
          {found.requests.map((request) => (
            <li key={request.id} className="item-row file-request-row">
              <span className="item-text">
                <span className="item-name">{request.label ?? t('(ohne Namen)')}</span>
                <span className="item-sub">
                  {state(request)} · {t('{n} erhalten', { n: request.submissionCount })}
                  {request.unseen > 0 && ` · ${t('{n} neu', { n: request.unseen })}`}
                </span>
              </span>
              <span className="detail-actions">
                <button
                  type="button"
                  className="icon-button"
                  onClick={() => void copy(request.id)}
                  aria-label={t('Link kopieren')}
                  title={t('Link kopieren')}
                >
                  <Icon name="copy" size={15} />
                </button>
                <a
                  className="icon-button"
                  href={request.manageUrl}
                  target="_blank"
                  rel="noreferrer"
                  aria-label={t('Im Web-Tresor öffnen')}
                  title={t('Im Web-Tresor öffnen')}
                >
                  <Icon name="external" size={15} />
                </a>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
