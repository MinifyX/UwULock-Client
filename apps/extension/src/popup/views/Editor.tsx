import { useEffect, useState, type FormEvent } from 'react';
import { Icon } from '@desktop/components/Icon';
import { N_, t } from '../../shared/i18n';
import type { Draft, FieldKind, ItemDetail, ItemKind, Overview } from '../../shared/protocol';
import { generate, revealField, saveItem, vaultItem, vaultOverview } from '../api';
import { errorText, PasswordInput, toast, useSettings } from '../lib';
import { BackBar, IDENTITY_LABEL, KIND_LABEL } from './Detail';

export type EditorTarget = { id: string | null; kind: ItemKind; name?: string; uri?: string };

const EDITABLE: ItemKind[] = ['login', 'note', 'card', 'identity'];

const MATCH_OPTIONS: { value: string; label: string }[] = [
  { value: '', label: N_('Standard') },
  { value: '0', label: N_('Domain') },
  { value: '1', label: N_('Host') },
  { value: '2', label: N_('Beginnt mit') },
  { value: '3', label: N_('Genau') },
  { value: '4', label: N_('Regulärer Ausdruck') },
  { value: '5', label: N_('Nie') },
];

const FIELD_KINDS: { value: FieldKind; label: string }[] = [
  { value: 'text', label: N_('Text') },
  { value: 'hidden', label: N_('Versteckt') },
  { value: 'boolean', label: N_('Ja/Nein') },
];

const IDENTITY_ORDER = Object.keys(IDENTITY_LABEL);

type FieldState = {
  name: string;
  kind: FieldKind;
  value: string | null;
  from: number | null;
  hasValue: boolean;
};

/**
 * Creating and editing logins, notes, cards and identities. A secret the popup never saw — a
 * password nobody revealed, a card number, a hidden field — stays `null` in the draft, and the
 * background keeps the value the item already has: the field says "Bleibt, wie es ist" instead
 * of showing dots that could be typed over.
 */
export function Editor({
  target,
  onDone,
  onCancel,
}: {
  target: EditorTarget;
  onDone: (id: string | null) => void;
  onCancel: () => void;
}) {
  const settings = useSettings();
  const [kind, setKind] = useState<ItemKind>(target.kind);
  const [loaded, setLoaded] = useState<ItemDetail | null>(null);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [name, setName] = useState(target.name ?? '');
  const [notes, setNotes] = useState('');
  const [folderId, setFolderId] = useState<string>('');
  const [favorite, setFavoriteState] = useState(false);
  const [reprompt, setReprompt] = useState(false);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState<string | null>(target.id ? null : '');
  const [totp, setTotp] = useState<string | null>(target.id ? null : '');
  const [uris, setUris] = useState<{ uri: string; match: string }[]>(
    target.uri ? [{ uri: target.uri, match: '' }] : target.id ? [] : [{ uri: '', match: '' }],
  );
  const [card, setCard] = useState<Record<string, string | null>>({});
  const [identity, setIdentity] = useState<Record<string, string>>({});
  const [identityKnown, setIdentityKnown] = useState<Record<string, string | null>>({});
  const [fields, setFields] = useState<FieldState[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void vaultOverview().then(setOverview, () => undefined);
    if (!target.id) return;
    void vaultItem(target.id).then(
      (item) => {
        setLoaded(item);
        setKind(item.summary.kind);
        setName(item.summary.name);
        setNotes(item.notes ?? '');
        setFolderId(item.summary.folderId ?? '');
        setFavoriteState(item.summary.favorite);
        setReprompt(item.summary.reprompt);
        setUsername(item.login?.username ?? '');
        setUris(
          (item.login?.uris ?? []).map((u) => ({
            uri: u.uri,
            match: u.match === null ? '' : String(u.match),
          })),
        );
        if (item.card) {
          setCard({
            cardholderName: item.card.cardholderName,
            brand: item.card.brand,
            expMonth: item.card.expMonth,
            expYear: item.card.expYear,
            number: null,
            code: null,
          });
        }
        if (item.identity) {
          setIdentityKnown(
            Object.fromEntries(item.identity.map((entry) => [entry.name, entry.value])),
          );
        }
        setFields(
          (item.fields ?? []).map((f) => ({
            name: f.name ?? '',
            kind: f.kind,
            value: f.kind === 'hidden' || f.kind === 'linked' ? null : (f.value ?? ''),
            from: f.index,
            hasValue: f.hasValue,
          })),
        );
      },
      (e) => setError(errorText(e)),
    );
  }, [target.id]);

  const showPassword = async () => {
    if (!target.id) return;
    try {
      setPassword(await revealField(target.id, 'password'));
    } catch (e) {
      toast(errorText(e), 'error');
    }
  };

  const generated = async () => {
    try {
      const result = await generate(settings!.generator);
      setPassword(result.password);
    } catch (e) {
      toast(errorText(e), 'error');
    }
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim()) {
      setError(t('Ein Eintrag braucht einen Namen.'));
      return;
    }
    const draft: Draft = {
      kind,
      name: name.trim(),
      notes: notes,
      favorite,
      reprompt,
      folderId: folderId || null,
      fields: fields.map((f) => ({
        name: f.name.trim() || null,
        kind: f.kind,
        value: f.value,
        from: f.from,
      })),
    };
    if (kind === 'login') {
      draft.login = {
        username,
        password,
        totp,
        uris: uris
          .filter((u) => u.uri.trim())
          .map((u) => ({ uri: u.uri.trim(), match: u.match === '' ? null : Number(u.match) })),
      };
    }
    if (kind === 'card') {
      draft.card = {
        cardholderName: card.cardholderName ?? '',
        brand: card.brand ?? '',
        number: card.number ?? (target.id ? null : ''),
        expMonth: card.expMonth ?? '',
        expYear: card.expYear ?? '',
        code: card.code ?? (target.id ? null : ''),
      };
    }
    if (kind === 'identity') draft.identity = identity;
    setBusy(true);
    setError(null);
    try {
      const id = await saveItem(target.id, draft);
      toast(t('Gespeichert ✧'));
      onDone(id || target.id);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const keep = t('Bleibt, wie es ist');
  const creating = !target.id;

  return (
    <form className="popup-scroll editor form" onSubmit={submit}>
      <BackBar onBack={onCancel}>
        <button className="primary" type="submit" disabled={busy || Boolean(target.id && !loaded)}>
          {busy ? t('Speichert …') : t('Speichern')}
        </button>
      </BackBar>
      <h2 className="card-title">{creating ? t('Neuer Eintrag') : t('Bearbeiten')}</h2>

      {creating && (
        <div className="segmented wide" role="radiogroup" aria-label={t('Art')}>
          {EDITABLE.map((k) => (
            <button
              key={k}
              type="button"
              role="radio"
              aria-checked={kind === k}
              onClick={() => setKind(k)}
            >
              {t(KIND_LABEL[k])}
            </button>
          ))}
        </div>
      )}

      <label className="field">
        <span>{t('Name')}</span>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
          autoFocus={creating}
        />
      </label>

      {kind === 'login' && (
        <>
          <label className="field">
            <span>{t('Benutzername')}</span>
            <input
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              spellCheck={false}
              autoComplete="off"
            />
          </label>
          <div className="field">
            <span>{t('Passwort')}</span>
            <div className="inline-controls">
              <PasswordInput
                value={password ?? ''}
                onChange={setPassword}
                autoComplete="new-password"
                placeholder={password === null && loaded?.login?.hasPassword ? keep : undefined}
              />
              {password === null && loaded?.login?.hasPassword && (
                <button
                  type="button"
                  className="icon-button"
                  onClick={() => void showPassword()}
                  title={t('Zeigen')}
                  aria-label={t('Zeigen')}
                >
                  <Icon name="eye" size={15} />
                </button>
              )}
              <button
                type="button"
                className="icon-button"
                onClick={() => void generated()}
                title={t('Passwort generieren')}
                aria-label={t('Passwort generieren')}
              >
                <Icon name="dice" size={15} />
              </button>
            </div>
          </div>
          <label className="field">
            <span>{t('Authenticator-Schlüssel (TOTP)')}</span>
            <input
              value={totp ?? ''}
              onChange={(e) => setTotp(e.target.value)}
              placeholder={
                totp === null && loaded?.login?.hasTotp ? keep : 'otpauth://… / JBSWY3DP…'
              }
              spellCheck={false}
              autoComplete="off"
            />
          </label>
          <div className="field">
            <span>{t('Websites')}</span>
            {uris.map((uri, index) => (
              <div className="uri-row" key={index}>
                <input
                  value={uri.uri}
                  onChange={(e) =>
                    setUris(uris.map((u, i) => (i === index ? { ...u, uri: e.target.value } : u)))
                  }
                  placeholder="https://example.com"
                  spellCheck={false}
                  aria-label={t('Adresse')}
                />
                <select
                  className="select"
                  value={uri.match}
                  onChange={(e) =>
                    setUris(uris.map((u, i) => (i === index ? { ...u, match: e.target.value } : u)))
                  }
                  aria-label={t('Erkennung')}
                >
                  {MATCH_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {t(option.label)}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  className="icon-button"
                  onClick={() => setUris(uris.filter((_, i) => i !== index))}
                  aria-label={t('Entfernen')}
                  title={t('Entfernen')}
                >
                  <Icon name="close" size={14} />
                </button>
              </div>
            ))}
            <button
              type="button"
              className="quiet add-line"
              onClick={() => setUris([...uris, { uri: '', match: '' }])}
            >
              <Icon name="plus" size={13} /> {t('Website hinzufügen')}
            </button>
          </div>
        </>
      )}

      {kind === 'card' && (
        <>
          <label className="field">
            <span>{t('Karteninhaber')}</span>
            <input
              value={card.cardholderName ?? ''}
              onChange={(e) => setCard({ ...card, cardholderName: e.target.value })}
            />
          </label>
          <label className="field">
            <span>{t('Marke')}</span>
            <input
              value={card.brand ?? ''}
              onChange={(e) => setCard({ ...card, brand: e.target.value })}
              list="card-brands"
            />
            <datalist id="card-brands">
              {[
                'Visa',
                'Mastercard',
                'American Express',
                'Discover',
                'Diners Club',
                'JCB',
                'Maestro',
                'UnionPay',
              ].map((b) => (
                <option key={b} value={b} />
              ))}
            </datalist>
          </label>
          <label className="field">
            <span>{t('Kartennummer')}</span>
            <input
              value={card.number ?? ''}
              onChange={(e) => setCard({ ...card, number: e.target.value })}
              placeholder={card.number === null && loaded?.card?.numberEnding ? keep : undefined}
              inputMode="numeric"
              autoComplete="off"
            />
          </label>
          <div className="form-row">
            <label className="field">
              <span>{t('Monat')}</span>
              <input
                value={card.expMonth ?? ''}
                onChange={(e) => setCard({ ...card, expMonth: e.target.value })}
                inputMode="numeric"
                placeholder="MM"
              />
            </label>
            <label className="field">
              <span>{t('Jahr')}</span>
              <input
                value={card.expYear ?? ''}
                onChange={(e) => setCard({ ...card, expYear: e.target.value })}
                inputMode="numeric"
                placeholder="JJJJ"
              />
            </label>
            <label className="field">
              <span>{t('Prüfnummer')}</span>
              <input
                value={card.code ?? ''}
                onChange={(e) => setCard({ ...card, code: e.target.value })}
                placeholder={card.code === null && loaded?.card?.hasCode ? '•••' : undefined}
                inputMode="numeric"
                autoComplete="off"
              />
            </label>
          </div>
        </>
      )}

      {kind === 'identity' &&
        IDENTITY_ORDER.map((field) => {
          const known = identityKnown[field];
          const sensitive = ['ssn', 'passportNumber', 'licenseNumber'].includes(field);
          return (
            <label className="field" key={field}>
              <span>{t(IDENTITY_LABEL[field]!)}</span>
              <input
                value={identity[field] ?? known ?? ''}
                onChange={(e) => setIdentity({ ...identity, [field]: e.target.value })}
                placeholder={
                  sensitive && target.id && identity[field] === undefined ? keep : undefined
                }
                autoComplete="off"
              />
            </label>
          );
        })}

      <label className="field">
        <span>{t('Notizen')}</span>
        <textarea value={notes} onChange={(e) => setNotes(e.target.value)} rows={3} />
      </label>

      <div className="field">
        <span>{t('Eigene Felder')}</span>
        {fields.map((field, index) => (
          <div className="uri-row" key={index}>
            <input
              value={field.name}
              onChange={(e) =>
                setFields(fields.map((f, i) => (i === index ? { ...f, name: e.target.value } : f)))
              }
              placeholder={t('Name')}
              aria-label={t('Name')}
            />
            {field.kind === 'boolean' ? (
              <select
                className="select"
                value={field.value === 'true' ? 'true' : 'false'}
                onChange={(e) =>
                  setFields(
                    fields.map((f, i) => (i === index ? { ...f, value: e.target.value } : f)),
                  )
                }
                aria-label={t('Wert')}
              >
                <option value="true">{t('Ja')}</option>
                <option value="false">{t('Nein')}</option>
              </select>
            ) : field.kind === 'linked' ? (
              <span className="muted">{t('Verknüpft')}</span>
            ) : (
              <input
                value={field.value ?? ''}
                onChange={(e) =>
                  setFields(
                    fields.map((f, i) => (i === index ? { ...f, value: e.target.value } : f)),
                  )
                }
                placeholder={field.value === null && field.hasValue ? keep : t('Wert')}
                type={field.kind === 'hidden' ? 'password' : 'text'}
                aria-label={t('Wert')}
                autoComplete="off"
              />
            )}
            <button
              type="button"
              className="icon-button"
              onClick={() => setFields(fields.filter((_, i) => i !== index))}
              aria-label={t('Entfernen')}
              title={t('Entfernen')}
            >
              <Icon name="close" size={14} />
            </button>
          </div>
        ))}
        <div className="add-field">
          {FIELD_KINDS.map((option) => (
            <button
              key={option.value}
              type="button"
              className="quiet add-line"
              onClick={() =>
                setFields([
                  ...fields,
                  {
                    name: '',
                    kind: option.value,
                    value: option.value === 'boolean' ? 'false' : '',
                    from: null,
                    hasValue: false,
                  },
                ])
              }
            >
              <Icon name="plus" size={13} /> {t(option.label)}
            </button>
          ))}
        </div>
      </div>

      <label className="field">
        <span>{t('Ordner')}</span>
        <select className="select" value={folderId} onChange={(e) => setFolderId(e.target.value)}>
          <option value="">{t('Kein Ordner')}</option>
          {overview?.folders.map((folder) => (
            <option key={folder.id} value={folder.id}>
              {folder.name}
            </option>
          ))}
        </select>
      </label>
      <label className="check">
        <input
          type="checkbox"
          checked={favorite}
          onChange={(e) => setFavoriteState(e.target.checked)}
        />
        <span>{t('Favorit')}</span>
      </label>
      <label className="check">
        <input type="checkbox" checked={reprompt} onChange={(e) => setReprompt(e.target.checked)} />
        <span>{t('Vor dem Anzeigen nach dem Master-Passwort fragen')}</span>
      </label>

      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
    </form>
  );
}
