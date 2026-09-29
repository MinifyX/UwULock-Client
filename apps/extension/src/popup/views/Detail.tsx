import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { Icon } from '@desktop/components/Icon';
import { N_, t } from '../../shared/i18n';
import { ItemIcon } from '../icons';
import { FillReprompt } from './FillReprompt';
import type { ItemDetail, ItemKind, TotpCode } from '../../shared/protocol';
import {
  copyField,
  deleteItem,
  fillTab,
  openItemUri,
  restoreItem,
  revealField,
  setFavorite,
  totpCode,
  vaultItem,
  verifyReprompt,
} from '../api';
import {
  Colored,
  copiedText,
  errorText,
  PasswordInput,
  spacedCode,
  toast,
  TotpRing,
  useSettings,
  when,
} from '../lib';

const KIND_LABEL: Record<ItemKind, string> = {
  login: N_('Login'),
  card: N_('Karte'),
  identity: N_('Identität'),
  note: N_('Sichere Notiz'),
  'ssh-key': N_('SSH-Schlüssel'),
};

const IDENTITY_LABEL: Record<string, string> = {
  title: N_('Anrede'),
  firstName: N_('Vorname'),
  middleName: N_('Zweiter Vorname'),
  lastName: N_('Nachname'),
  username: N_('Benutzername'),
  company: N_('Firma'),
  email: N_('E-Mail'),
  phone: N_('Telefon'),
  address1: N_('Adresse'),
  address2: N_('Adresse 2'),
  address3: N_('Adresse 3'),
  postalCode: N_('Postleitzahl'),
  city: N_('Ort'),
  state: N_('Bundesland'),
  country: N_('Land'),
  ssn: N_('Sozialversicherungsnummer'),
  passportNumber: N_('Reisepassnummer'),
  licenseNumber: N_('Führerscheinnummer'),
};

export { IDENTITY_LABEL, KIND_LABEL };

/** One item: every value with its copy button, secrets as dots until the eye is clicked. */
export function Detail({
  id,
  onBack,
  onEdit,
  onShare,
}: {
  id: string;
  onBack: () => void;
  onEdit: (id: string, kind: ItemKind) => void;
  onShare: (id: string) => void;
}) {
  const [item, setItem] = useState<ItemDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setItem(await vaultItem(id));
    } catch (e) {
      setError(errorText(e));
    }
  }, [id]);

  useEffect(() => {
    void load();
  }, [load]);

  if (error) {
    return (
      <div className="popup-scroll">
        <BackBar onBack={onBack} />
        <p className="form-error">{error}</p>
      </div>
    );
  }
  if (!item) return <div className="popup-scroll" aria-busy />;

  const summary = item.summary;

  const toggleFavorite = async () => {
    try {
      await setFavorite(id, !summary.favorite);
      await load();
    } catch (e) {
      toast(errorText(e), 'error');
    }
  };

  const remove = async () => {
    const permanent = summary.deleted;
    const question = permanent
      ? t('„{name}“ endgültig löschen? Das lässt sich nicht rückgängig machen.', {
          name: summary.name,
        })
      : t('„{name}“ in den Papierkorb legen?', { name: summary.name });
    if (!window.confirm(question)) return;
    try {
      await deleteItem(id, permanent);
      toast(permanent ? t('Endgültig gelöscht') : t('In den Papierkorb gelegt'));
      onBack();
    } catch (e) {
      toast(errorText(e), 'error');
    }
  };

  return (
    <div className="popup-scroll detail-view">
      <BackBar onBack={onBack}>
        {!summary.deleted && !item.locked && (
          <>
            <button
              type="button"
              className="icon-button"
              onClick={() => void toggleFavorite()}
              aria-pressed={summary.favorite}
              aria-label={summary.favorite ? t('Aus Favoriten entfernen') : t('Zu Favoriten')}
              title={summary.favorite ? t('Aus Favoriten entfernen') : t('Zu Favoriten')}
            >
              <Icon name="star" size={16} className={summary.favorite ? 'badge-star' : undefined} />
            </button>
            {!summary.broken && (
              <button
                type="button"
                className="icon-button"
                onClick={() => onShare(id)}
                aria-label={t('Als Send teilen')}
                title={t('Als Send teilen')}
              >
                <Icon name="export" size={16} />
              </button>
            )}
            {!summary.broken && summary.kind !== 'ssh-key' && (
              <button
                type="button"
                className="icon-button"
                onClick={() => onEdit(id, summary.kind)}
                aria-label={t('Bearbeiten')}
                title={t('Bearbeiten')}
              >
                <Icon name="pencil" size={16} />
              </button>
            )}
          </>
        )}
        {summary.deleted && (
          <button
            type="button"
            className="quiet"
            onClick={() =>
              void restoreItem(id)
                .then(() => {
                  toast(t('Wiederhergestellt ✧'));
                  return load();
                })
                .catch((e) => toast(errorText(e), 'error'))
            }
          >
            {t('Wiederherstellen')}
          </button>
        )}
        <button
          type="button"
          className="icon-button"
          onClick={() => void remove()}
          aria-label={summary.deleted ? t('Endgültig löschen') : t('Löschen')}
          title={summary.deleted ? t('Endgültig löschen') : t('Löschen')}
        >
          <Icon name="trash" size={16} />
        </button>
      </BackBar>

      <div className="detail-head">
        <ItemIcon item={summary} size="large" />
        <div className="detail-title">
          <h2>{summary.name || t('(ohne Namen)')}</h2>
          <span className="muted">{t(KIND_LABEL[summary.kind])}</span>
        </div>
      </div>

      {summary.broken && (
        <p className="notice" data-tone="error">
          {t(
            'Ein Teil dieses Eintrags ließ sich nicht entschlüsseln. UwULock ändert ihn deshalb nicht.',
          )}
        </p>
      )}

      {item.locked ? <Reprompt id={id} onDone={() => void load()} /> : <Body item={item} />}
    </div>
  );
}

export function BackBar({ onBack, children }: { onBack: () => void; children?: ReactNode }) {
  return (
    <div className="back-bar">
      <button type="button" className="quiet back" onClick={onBack}>
        <Icon name="chevron" size={14} className="flip" /> {t('Zurück')}
      </button>
      <span className="spacer" />
      {children}
    </div>
  );
}

function Reprompt({ id, onDone }: { id: string; onDone: () => void }) {
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await verifyReprompt(id, password);
      setPassword('');
      onDone();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <form className="detail-card reprompt form" onSubmit={submit}>
      <h3 className="detail-card-title">
        <Icon name="shield" size={13} /> {t('Master-Passwort bestätigen')}
      </h3>
      <p className="dialog-lead">
        {t('Dieser Eintrag ist geschützt. Gib dein Master-Passwort ein, um ihn zu sehen.')}
      </p>
      <PasswordInput value={password} onChange={setPassword} autoFocus disabled={busy} />
      {error && <p className="form-error">{error}</p>}
      <div className="form-actions">
        <span className="spacer" />
        <button className="primary" type="submit" disabled={busy || !password}>
          {t('Bestätigen')}
        </button>
      </div>
    </form>
  );
}

function Body({ item }: { item: ItemDetail }) {
  const id = item.summary.id;
  const login = item.login;
  const card = item.card;
  // An item with the re-prompt asks for the master password again for every fill.
  const [asking, setAsking] = useState(false);
  return (
    <div className="detail-cards">
      {asking && (
        <FillReprompt
          name={item.summary.name}
          onFill={async (password) => {
            await fillTab(id, false, password);
            window.close();
          }}
          onCancel={() => setAsking(false)}
        />
      )}
      {login && (
        <section className="detail-card">
          {item.summary.deleted === false && (login.hasPassword || login.username) && (
            <div className="detail-row fill-row">
              <button
                type="button"
                className="primary wide"
                onClick={() =>
                  item.summary.reprompt
                    ? setAsking(true)
                    : void fillTab(id)
                        .then(() => window.close())
                        .catch((e) => toast(errorText(e), 'error'))
                }
              >
                {t('Auf dieser Seite ausfüllen')}
              </button>
            </div>
          )}
          {login.username && (
            <Row id={id} label={t('Benutzername')} field="username" value={login.username} />
          )}
          {login.hasPassword && (
            <SecretRow id={id} label={t('Passwort')} field="password" colored />
          )}
          {login.hasTotp && <TotpRow id={id} />}
          {login.passkeys > 0 && (
            <div className="detail-row">
              <span className="detail-text">
                <span className="detail-label">{t('Passkey')}</span>
                <span className="detail-value">{t('Gespeichert ✧')}</span>
              </span>
            </div>
          )}
        </section>
      )}

      {login && login.uris.length > 0 && (
        <section className="detail-card">
          <h3 className="detail-card-title">{t('Websites')}</h3>
          {login.uris.map((uri, index) => (
            <div className="detail-row" key={index}>
              <span className="detail-text">
                <span className="detail-value uri">{uri.uri}</span>
              </span>
              <span className="detail-actions">
                {uri.openable && (
                  <button
                    type="button"
                    className="icon-button"
                    onClick={() => void openItemUri(id, index)}
                    aria-label={t('Öffnen')}
                    title={t('Öffnen')}
                  >
                    <Icon name="external" size={15} />
                  </button>
                )}
                <CopyButton id={id} field={`uri:${index}`} />
              </span>
            </div>
          ))}
        </section>
      )}

      {card && (
        <section className="detail-card">
          {card.cardholderName && (
            <Row id={id} label={t('Karteninhaber')} field="card-name" value={card.cardholderName} />
          )}
          {card.brand && <Row id={id} label={t('Marke')} value={card.brand} />}
          {card.numberEnding && (
            <SecretRow
              id={id}
              label={t('Kartennummer')}
              field="card-number"
              hint={`•••• ${card.numberEnding}`}
            />
          )}
          {(card.expMonth || card.expYear) && (
            <Row
              id={id}
              label={t('Gültig bis')}
              field="card-expiry"
              value={`${(card.expMonth ?? '').padStart(2, '0')}/${card.expYear ?? ''}`}
            />
          )}
          {card.hasCode && <SecretRow id={id} label={t('Prüfnummer')} field="card-code" />}
        </section>
      )}

      {item.identity && item.identity.length > 0 && (
        <section className="detail-card">
          {item.identity.map((entry) =>
            entry.sensitive ? (
              <SecretRow
                key={entry.name}
                id={id}
                label={t(IDENTITY_LABEL[entry.name] ?? entry.name)}
                field={`identity:${entry.name}`}
              />
            ) : (
              <Row
                key={entry.name}
                id={id}
                label={t(IDENTITY_LABEL[entry.name] ?? entry.name)}
                field={`identity:${entry.name}`}
                value={entry.value ?? ''}
              />
            ),
          )}
        </section>
      )}

      {item.sshKey && (
        <section className="detail-card">
          {item.sshKey.publicKey && (
            <Row
              id={id}
              label={t('Öffentlicher Schlüssel')}
              field="ssh-public"
              value={item.sshKey.publicKey}
              mono
            />
          )}
          {item.sshKey.fingerprint && (
            <Row
              id={id}
              label={t('Fingerabdruck')}
              field="ssh-fingerprint"
              value={item.sshKey.fingerprint}
              mono
            />
          )}
          {item.sshKey.hasPrivateKey && (
            <SecretRow id={id} label={t('Privater Schlüssel')} field="ssh-private" />
          )}
        </section>
      )}

      {item.notes && (
        <section className="detail-card">
          <h3 className="detail-card-title">{t('Notizen')}</h3>
          <div className="detail-row">
            <p className="detail-value notes-text">{item.notes}</p>
            <span className="detail-actions">
              <CopyButton id={id} field="notes" />
            </span>
          </div>
        </section>
      )}

      {item.fields && item.fields.length > 0 && (
        <section className="detail-card">
          <h3 className="detail-card-title">{t('Eigene Felder')}</h3>
          {item.fields.map((field) =>
            field.kind === 'hidden' ? (
              <SecretRow
                key={field.index}
                id={id}
                label={field.name ?? t('Feld')}
                field={`field:${field.index}`}
              />
            ) : field.kind === 'linked' ? (
              <div className="detail-row" key={field.index}>
                <span className="detail-text">
                  <span className="detail-label">{field.name ?? t('Feld')}</span>
                  <span className="detail-value muted">{t('Verknüpft')}</span>
                </span>
              </div>
            ) : (
              <Row
                key={field.index}
                id={id}
                label={field.name ?? t('Feld')}
                field={field.hasValue ? `field:${field.index}` : undefined}
                value={
                  field.kind === 'boolean'
                    ? field.value === 'true'
                      ? t('Ja')
                      : t('Nein')
                    : (field.value ?? '')
                }
              />
            ),
          )}
        </section>
      )}

      {item.passwordHistory && item.passwordHistory.length > 0 && (
        <History id={id} entries={item.passwordHistory} />
      )}

      <p className="detail-foot muted">
        {item.summary.revisionDate &&
          t('Geändert: {date}', { date: when(item.summary.revisionDate) ?? '' })}
        {item.login?.passwordRevisionDate &&
          ` · ${t('Passwort geändert: {date}', { date: when(item.login.passwordRevisionDate) ?? '' })}`}
      </p>
    </div>
  );
}

function CopyButton({ id, field }: { id: string; field: string }) {
  const settings = useSettings();
  return (
    <button
      type="button"
      className="icon-button"
      onClick={() =>
        void copyField(id, field)
          .then(() => toast(copiedText(field, settings?.clipboardClear ?? 30)))
          .catch((e) => toast(errorText(e), 'error'))
      }
      aria-label={t('Kopieren')}
      title={t('Kopieren')}
    >
      <Icon name="copy" size={15} />
    </button>
  );
}

function Row({
  id,
  label,
  value,
  field,
  mono,
}: {
  id: string;
  label: string;
  value: string;
  field?: string;
  mono?: boolean;
}) {
  return (
    <div className="detail-row">
      <span className="detail-text">
        <span className="detail-label">{label}</span>
        <span className={mono ? 'detail-value mono' : 'detail-value'}>{value}</span>
      </span>
      {field && (
        <span className="detail-actions">
          <CopyButton id={id} field={field} />
        </span>
      )}
    </div>
  );
}

function SecretRow({
  id,
  label,
  field,
  colored,
  hint,
}: {
  id: string;
  label: string;
  field: string;
  colored?: boolean;
  hint?: string;
}) {
  const [value, setValue] = useState<string | null>(null);
  const reveal = async () => {
    if (value !== null) {
      setValue(null);
      return;
    }
    try {
      setValue(await revealField(id, field));
    } catch (e) {
      toast(errorText(e), 'error');
    }
  };
  return (
    <div className="detail-row">
      <span className="detail-text">
        <span className="detail-label">{label}</span>
        <span className="detail-value mono">
          {value === null ? (
            <span className="masked">{hint ?? '••••••••••'}</span>
          ) : colored ? (
            <Colored text={value} />
          ) : (
            value
          )}
        </span>
      </span>
      <span className="detail-actions">
        <button
          type="button"
          className="icon-button"
          onClick={() => void reveal()}
          aria-pressed={value !== null}
          aria-label={value === null ? t('Zeigen') : t('Verbergen')}
          title={value === null ? t('Zeigen') : t('Verbergen')}
        >
          <Icon name={value === null ? 'eye' : 'eyeOff'} size={15} />
        </button>
        <CopyButton id={id} field={field} />
      </span>
    </div>
  );
}

function TotpRow({ id }: { id: string }) {
  const [code, setCode] = useState<TotpCode | null>(null);
  useEffect(() => {
    let alive = true;
    let timer: number | undefined;
    const tick = async () => {
      try {
        const next = await totpCode(id);
        if (!alive) return;
        setCode(next);
      } catch {
        // Shown as dashes; the next tick tries again.
      }
      if (alive) timer = window.setTimeout(() => void tick(), 1000);
    };
    void tick();
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [id]);
  return (
    <div className="detail-row">
      <span className="detail-text">
        <span className="detail-label">{t('Einmal-Code')}</span>
        {code ? (
          <span className="totp" data-soon={code.remaining <= 5 || undefined}>
            <span className="totp-code mono">{spacedCode(code.code)}</span>
            <TotpRing remaining={code.remaining} period={code.period} />
            <span className="totp-seconds">{code.remaining}</span>
          </span>
        ) : (
          <span className="detail-value muted">– – –</span>
        )}
      </span>
      <span className="detail-actions">
        <CopyButton id={id} field="totp" />
      </span>
    </div>
  );
}

function History({
  id,
  entries,
}: {
  id: string;
  entries: { index: number; lastUsed: string | null }[];
}) {
  const [open, setOpen] = useState(false);
  return (
    <section className="detail-card">
      <button
        type="button"
        className="history-toggle quiet"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
      >
        <Icon name="history" size={13} /> {t('Frühere Passwörter ({n})', { n: entries.length })}
      </button>
      {open &&
        entries.map((entry) => (
          <SecretRow
            key={entry.index}
            id={id}
            label={when(entry.lastUsed) ?? t('Früher')}
            field={`history:${entry.index}`}
            colored
          />
        ))}
    </section>
  );
}
