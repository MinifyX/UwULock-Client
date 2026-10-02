import { useEffect, useState, type FormEvent } from 'react';
import { Icon } from '@desktop/components/Icon';
import { playNyu } from '@desktop/components/nyu/stage';
import { N_, t, locale } from '../../shared/i18n';
import type { ItemDetail, ShareableField, SharedSend } from '../../shared/protocol';
import { copyText, shareFields, shareItem, vaultItem } from '../api';
import { errorText, PasswordInput, toast, toastError, useSettings } from '../lib';
import { BackBar, IDENTITY_LABEL } from './Detail';

const LABELS: Record<string, string> = {
  username: N_('Benutzername'),
  password: N_('Passwort'),
  totp: N_('Einmal-Code'),
  notes: N_('Notizen'),
  'card-name': N_('Karteninhaber'),
  'card-number': N_('Kartennummer'),
  'card-expiry': N_('Gültig bis'),
  'card-code': N_('Prüfnummer'),
  'ssh-public': N_('Öffentlicher Schlüssel'),
  'ssh-fingerprint': N_('Fingerabdruck'),
  'ssh-private': N_('Privater Schlüssel'),
};

const DELETION: { hours: number; label: string }[] = [
  { hours: 1, label: N_('Nach 1 Stunde') },
  { hours: 24, label: N_('Nach 1 Tag') },
  { hours: 7 * 24, label: N_('Nach 7 Tagen') },
  { hours: 30 * 24, label: N_('Nach 30 Tagen') },
];

const ACCESSES: { value: number | null; label: string }[] = [
  { value: 1, label: N_('Einmal') },
  { value: 3, label: N_('3-mal') },
  { value: 10, label: N_('10-mal') },
  { value: null, label: N_('Beliebig oft') },
];

/** What the recipient reads before a value, in the language of the popup. */
export function shareLabel(field: ShareableField, websites = 1): string {
  if (field.name.startsWith('field:')) return field.label ?? t('Feld');
  if (field.name.startsWith('uri:'))
    return websites > 1 ? t('Website {n}', { n: Number(field.name.slice(4)) + 1 }) : t('Website');
  if (field.name.startsWith('identity:')) {
    const name = field.name.slice('identity:'.length);
    return t(IDENTITY_LABEL[name] ?? name);
  }
  return t(LABELS[field.name] ?? field.name);
}

/** Checked at first: what "share this login" means; for the other kinds, everything. */
function chosenFirst(item: ItemDetail, field: ShareableField): boolean {
  if (item.summary.kind !== 'login') return true;
  return field.name === 'username' || field.name === 'password' || field.name.startsWith('uri:');
}

/** What the list shows for a value: a website's own address, else its label. */
function shownLabel(item: ItemDetail | null, field: ShareableField, websites: number): string {
  if (field.name.startsWith('uri:')) {
    const uri = item?.login?.uris[Number(field.name.slice(4))]?.uri;
    if (uri) return uri;
  }
  return shareLabel(field, websites);
}

/**
 * Shares chosen values of an item as a Send: a link that opens once (by default) and is gone
 * after a day, with an optional password. On UwULock Server (`entry`) it is an entry Send, whose
 * page shows the item with copy buttons — and, if chosen, the live one-time codes, never the
 * key. Elsewhere the authenticator key is never among the values.
 */
export function ShareView({
  id,
  entry,
  onBack,
}: {
  id: string;
  entry: boolean;
  onBack: () => void;
}) {
  const settings = useSettings();
  const [item, setItem] = useState<ItemDetail | null>(null);
  const [fields, setFields] = useState<ShareableField[] | null>(null);
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [hours, setHours] = useState(24);
  const [accesses, setAccesses] = useState<number | null>(1);
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [shared, setShared] = useState<SharedSend | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const found = await vaultItem(id);
        const shareable = await shareFields(id);
        // The one-time code only in an entry Send; the core leaves it out of the shareable ones.
        const available =
          entry && found.login?.hasTotp && found.summary.viewPassword !== false
            ? [...shareable, { name: 'totp' }]
            : shareable;
        setItem(found);
        setFields(available);
        setChosen(new Set(available.filter((f) => chosenFirst(found, f)).map((f) => f.name)));
      } catch (e) {
        setError(errorText(e));
      }
    })();
  }, [id, entry]);

  const websites = item?.login?.uris.length ?? 0;

  const toggle = (name: string, on: boolean) => {
    const next = new Set(chosen);
    if (on) next.add(name);
    else next.delete(name);
    setChosen(next);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!fields) return;
    setBusy(true);
    setError(null);
    try {
      const picked = fields.filter((f) => chosen.has(f.name));
      setShared(
        await shareItem(id, {
          fields: picked.map((f) => [f.name, shareLabel(f, websites)]),
          deletionHours: hours,
          maxAccessCount: accesses,
          password: password || null,
          entry,
        }),
      );
      setPassword('');
      playNyu('shared');
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const copy = async (link: string) => {
    try {
      await copyText(link);
      toast(
        (settings?.clipboardClear ?? 0) > 0
          ? t('{what} ✧ – wird nach {n} s geleert', {
              what: t('Link kopiert'),
              n: settings?.clipboardClear ?? 0,
            })
          : `${t('Link kopiert')} ✧`,
      );
    } catch (e) {
      toastError(e);
    }
  };

  if (shared) {
    return (
      <div className="popup-scroll form">
        <BackBar onBack={onBack} />
        <h2 className="card-title">{t('Link erstellt ✧')}</h2>
        <p className="dialog-lead">
          {t('Wer diesen Link hat, kann die Werte sehen. Er verschwindet am {date}.', {
            date: new Date(shared.deletionDate).toLocaleString(locale(), {
              dateStyle: 'medium',
              timeStyle: 'short',
            }),
          })}
        </p>
        <div className="detail-card">
          <div className="detail-row">
            <span className="detail-text">
              <span className="detail-value mono share-link">{shared.link}</span>
            </span>
            <span className="detail-actions">
              <button
                type="button"
                className="icon-button"
                onClick={() => void copy(shared.link)}
                aria-label={t('Kopieren')}
                title={t('Kopieren')}
              >
                <Icon name="copy" size={15} />
              </button>
            </span>
          </div>
        </div>
        <div className="form-actions">
          <span className="spacer" />
          <button type="button" className="primary" onClick={() => void copy(shared.link)}>
            <Icon name="copy" size={14} /> {t('Link kopieren')}
          </button>
        </div>
      </div>
    );
  }

  return (
    <form className="popup-scroll form" onSubmit={submit}>
      <BackBar onBack={onBack} />
      <h2 className="card-title">{t('Als Send teilen')}</h2>
      {item && (
        <p className="dialog-lead">
          {entry
            ? t('„{name}“ als Link teilen. Die Send-Seite zeigt den Eintrag mit Kopier-Knöpfen.', {
                name: item.summary.name,
              })
            : t('„{name}“ als Link teilen. Der Einmal-Code-Schlüssel wird nie geteilt.', {
                name: item.summary.name,
              })}
        </p>
      )}
      {!fields && !error && <div aria-busy />}
      {fields && fields.length === 0 && (
        <p className="notice">{t('Dieser Eintrag hat nichts, was sich teilen lässt.')}</p>
      )}
      {fields && fields.length > 0 && (
        <fieldset className="share-fields">
          <legend className="detail-label">{t('Was geteilt wird')}</legend>
          {fields.map((field) => (
            <label className="check" key={field.name}>
              <input
                type="checkbox"
                checked={chosen.has(field.name)}
                onChange={(e) => toggle(field.name, e.target.checked)}
              />
              <span className={field.name.startsWith('uri:') ? 'uri' : undefined}>
                {shownLabel(item, field, websites)}
              </span>
            </label>
          ))}
          {chosen.has('totp') && (
            <p className="field-hint">
              {t(
                'Die Send-Seite zeigt nur die laufenden Codes, nie den Schlüssel. Wer den Send öffnen kann, bekommt aber Codes, solange es ihn gibt.',
              )}
            </p>
          )}
        </fieldset>
      )}
      <label className="field">
        <span>{t('Löschen')}</span>
        <select className="select" value={hours} onChange={(e) => setHours(Number(e.target.value))}>
          {DELETION.map((option) => (
            <option key={option.hours} value={option.hours}>
              {t(option.label)}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>{t('Wie oft er sich öffnen lässt')}</span>
        <select
          className="select"
          value={accesses ?? ''}
          onChange={(e) => setAccesses(e.target.value ? Number(e.target.value) : null)}
        >
          {ACCESSES.map((option) => (
            <option key={option.label} value={option.value ?? ''}>
              {t(option.label)}
            </option>
          ))}
        </select>
      </label>
      <div className="field">
        <span>{t('Passwort (freiwillig)')}</span>
        <PasswordInput
          value={password}
          onChange={setPassword}
          autoComplete="new-password"
          label={t('Passwort (freiwillig)')}
        />
      </div>
      {error && <p className="form-error">{error}</p>}
      <div className="form-actions">
        <span className="spacer" />
        <button className="primary" type="submit" disabled={busy || !fields || chosen.size === 0}>
          {busy ? t('Erstellt …') : t('Link erstellen')}
        </button>
      </div>
    </form>
  );
}
