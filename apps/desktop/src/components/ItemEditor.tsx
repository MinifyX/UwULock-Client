import { Button, IconButton, ICONS, Switch } from '@uwusuite/design';
import { useEffect, useId, useMemo, useState, type FormEvent, type ReactNode } from 'react';
import {
  revealField,
  saveItem,
  vaultItem,
  type Draft,
  type FieldKind,
  type ItemDetail as Detail,
  type ItemKind,
  type ItemSummary,
  type Overview,
} from '../lib/api';
import { errorText, toastError } from '../lib/errors';
import { t, useLanguage } from '../lib/i18n';
import {
  CARD_BRANDS,
  FIELD_KIND_LABEL,
  IDENTITY_FIELDS,
  IDENTITY_LABEL,
  KIND_LABEL,
  MATCH_LABEL,
  SECURITY_LABEL,
} from '../lib/items';
import { toast } from '../lib/toast';
import { createMaskedAddress, has, linkMaskedAddress, useUwu } from '../lib/uwu';
import {
  EAP_METHODS,
  isEnterprise,
  PHASE2_METHODS,
  readWifi,
  SECURITIES,
  wifiFields,
  type WifiView,
} from '../lib/wifi';
import { useCloseGuard } from './CloseGuard';
import { GeneratorDialog } from './GeneratorDialog';
import { reminderDraft, ReminderEditor, saveReminder, type ReminderDraft } from './ItemExtras';
import { Modal } from './Modal';
import { NyuBusy, playNyu } from './nyu/stage';

/**
 * A value the editor may not have: a password, a card number, a hidden field.
 * `keep` means the item's own value stays — the editor never saw it, and it
 * never goes through the window unless someone asks to see it.
 */
type Sec = { mode: 'keep' | 'value'; value: string };

const keep = (has: boolean): Sec =>
  has ? { mode: 'keep', value: '' } : { mode: 'value', value: '' };
const sent = (secret: Sec): string | null => (secret.mode === 'keep' ? null : secret.value);

type UriRow = { key: number; uri: string; match: number | null };
type FieldRow = {
  key: number;
  name: string;
  kind: FieldKind;
  value: Sec;
  /** Which field of the item this row was, for the value and the link. */
  from: number | null;
};

/** A Wi-Fi network's own fields; the editor's other fields are the item's other ones. */
type WifiForm = {
  ssid: string;
  password: Sec;
  security: string;
  hidden: boolean;
  eap: string;
  phase2: string;
  identity: string;
  anonymous: string;
  ca: string;
  from: WifiView['from'];
};

type Form = {
  name: string;
  folderId: string;
  favorite: boolean;
  reprompt: boolean;
  notes: string;
  username: string;
  password: Sec;
  totp: Sec;
  uris: UriRow[];
  cardholderName: string;
  brand: string;
  number: Sec;
  expMonth: string;
  expYear: string;
  code: Sec;
  identity: Record<string, string>;
  identitySecrets: Record<string, Sec>;
  privateKey: Sec;
  publicKey: string;
  fingerprint: string;
  fields: FieldRow[];
  wifi: WifiForm;
};

function emptyForm(kind: ItemKind): Form {
  return {
    name: '',
    folderId: '',
    favorite: false,
    reprompt: false,
    notes: '',
    username: '',
    password: keep(false),
    totp: keep(false),
    uris: kind === 'login' ? [{ key: 1, uri: '', match: null }] : [],
    cardholderName: '',
    brand: '',
    number: keep(false),
    expMonth: '',
    expYear: '',
    code: keep(false),
    identity: {},
    identitySecrets: {},
    privateKey: keep(false),
    publicKey: '',
    fingerprint: '',
    fields: [],
    wifi: {
      ssid: '',
      password: keep(false),
      security: 'WPA2',
      hidden: false,
      eap: 'PEAP',
      phase2: 'MSCHAPV2',
      identity: '',
      anonymous: '',
      ca: '',
      from: {},
    },
  };
}

function fieldRow(field: NonNullable<Detail['fields']>[number]): FieldRow {
  return {
    key: field.index + 1,
    name: field.name ?? '',
    kind: field.kind,
    value:
      field.kind === 'hidden' ? keep(field.hasValue) : { mode: 'value', value: field.value ?? '' },
    from: field.index,
  };
}

/** The item as it is now, as far as the page is allowed to know it. */
function formOf(summary: ItemSummary, detail: Detail): Form {
  const form = emptyForm(summary.kind);
  form.name = summary.name;
  form.folderId = summary.folderId ?? '';
  form.favorite = summary.favorite;
  form.reprompt = summary.reprompt;
  form.notes = detail.notes ?? '';
  if (detail.login) {
    form.username = detail.login.username ?? '';
    form.password = keep(detail.login.hasPassword);
    form.totp = keep(detail.login.hasTotp);
    form.uris = detail.login.uris.map((uri, index) => ({
      key: index + 1,
      uri: uri.uri,
      match: uri.match,
    }));
  }
  if (detail.card) {
    form.cardholderName = detail.card.cardholderName ?? '';
    form.brand = detail.card.brand ?? '';
    form.number = keep(Boolean(detail.card.numberEnding));
    form.expMonth = detail.card.expMonth ?? '';
    form.expYear = detail.card.expYear ?? '';
    form.code = keep(detail.card.hasCode);
  }
  for (const field of IDENTITY_FIELDS) {
    const entry = detail.identity?.find((e) => e.name === field.name);
    if (field.sensitive) form.identitySecrets[field.name] = keep(Boolean(entry));
    else form.identity[field.name] = entry?.value ?? '';
  }
  if (detail.sshKey) {
    form.privateKey = keep(detail.sshKey.hasPrivateKey);
    form.publicKey = detail.sshKey.publicKey ?? '';
    form.fingerprint = detail.sshKey.fingerprint ?? '';
  }
  if (summary.kind === 'wifi') {
    const wifi = readWifi(detail.fields ?? []);
    const password = wifi.password;
    form.wifi = {
      ssid: wifi.ssid,
      password: !password
        ? keep(false)
        : password.kind === 'hidden'
          ? keep(password.hasValue)
          : { mode: 'value', value: password.value ?? '' },
      security: wifi.security || 'WPA2',
      hidden: wifi.hidden,
      // An Enterprise network without a method gets the usual one shown, and saved.
      eap: wifi.eap || (isEnterprise(wifi.security) ? '' : 'PEAP'),
      phase2: wifi.phase2 || (isEnterprise(wifi.security) ? '' : 'MSCHAPV2'),
      identity: wifi.identity,
      anonymous: wifi.anonymous,
      ca: wifi.ca,
      from: wifi.from,
    };
    form.fields = wifi.others.map(fieldRow);
    return form;
  }
  form.fields = (detail.fields ?? []).map(fieldRow);
  return form;
}

function draftOf(form: Form, kind: ItemKind): Draft {
  const draft: Draft = {
    // A Wi-Fi network is a secure note to the vault.
    kind: kind === 'wifi' ? 'note' : kind,
    name: form.name.trim(),
    notes: form.notes,
    favorite: form.favorite,
    reprompt: form.reprompt,
    folderId: form.folderId || null,
    fields: form.fields.map((field) => ({
      name: field.name.trim() || null,
      kind: field.kind,
      value: field.kind === 'linked' ? null : sent(field.value),
      from: field.from,
    })),
  };
  if (kind === 'login') {
    draft.login = {
      username: form.username,
      password: sent(form.password),
      totp: sent(form.totp),
      uris: form.uris
        .filter((uri) => uri.uri.trim())
        .map((uri) => ({ uri: uri.uri.trim(), match: uri.match })),
    };
  }
  if (kind === 'card') {
    draft.card = {
      cardholderName: form.cardholderName,
      brand: form.brand,
      number: sent(form.number),
      expMonth: form.expMonth,
      expYear: form.expYear,
      code: sent(form.code),
    };
  }
  if (kind === 'identity') {
    const values: Record<string, string> = {};
    for (const field of IDENTITY_FIELDS) {
      if (field.sensitive) {
        const secret = form.identitySecrets[field.name];
        if (secret && secret.mode === 'value') values[field.name] = secret.value;
      } else {
        values[field.name] = form.identity[field.name] ?? '';
      }
    }
    draft.identity = values;
  }
  if (kind === 'wifi') {
    const wifi = form.wifi;
    draft.fields = wifiFields(
      {
        ssid: wifi.ssid.trim(),
        password: sent(wifi.password),
        security: wifi.security,
        hidden: wifi.hidden,
        eap: wifi.eap,
        phase2: wifi.phase2,
        identity: wifi.identity,
        anonymous: wifi.anonymous,
        ca: wifi.ca,
        from: wifi.from,
      },
      draft.fields,
    );
  }
  if (kind === 'ssh-key') {
    draft.sshKey = {
      privateKey: sent(form.privateKey),
      publicKey: form.publicKey,
      fingerprint: form.fingerprint,
    };
  }
  return draft;
}

function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: string }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
      {hint && <small className="field-hint">{hint}</small>}
    </label>
  );
}

/** An on/off choice: its words, then the switch (clicking the words flips it too). */
function SwitchRow({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  const id = useId();
  return (
    <div className="flex items-center gap-2 text-meta">
      <label htmlFor={id} className="flex-1 cursor-pointer">
        {label}
      </label>
      <Switch id={id} checked={checked} onChange={onChange} />
    </div>
  );
}

/**
 * A field for a value the editor may not have. It shows dots until someone
 * asks to see it; typing replaces it, the cross empties it, and an empty field
 * that was never touched leaves the item's value alone.
 */
function SecretField({
  label,
  secret,
  onChange,
  itemId,
  field,
  multiline,
  hint,
  children,
}: {
  label: string;
  secret: Sec;
  onChange: (next: Sec) => void;
  /** Where to fetch the value from, for the eye. */
  itemId?: string | null;
  field?: string;
  multiline?: boolean;
  hint?: string;
  children?: ReactNode;
}) {
  useLanguage();
  const kept = secret.mode === 'keep';
  const cleared = secret.mode === 'value' && secret.value === '' && Boolean(itemId && field);

  const reveal = async () => {
    if (!itemId || !field) return;
    try {
      onChange({ mode: 'value', value: await revealField(itemId, field) });
    } catch (e) {
      toastError(e);
    }
  };

  const type = (value: string) => {
    // Deleting what was typed goes back to leaving the value alone.
    if (value === '' && kept) return;
    onChange({ mode: 'value', value });
  };

  const input = multiline ? (
    <textarea
      className="mono"
      rows={4}
      value={secret.value}
      placeholder={kept ? '••••••••••••' : undefined}
      spellCheck={false}
      onChange={(e) => type(e.target.value)}
    />
  ) : (
    <input
      type="text"
      className="mono"
      value={secret.value}
      placeholder={kept ? '••••••••••••' : undefined}
      spellCheck={false}
      autoComplete="off"
      onChange={(e) => type(e.target.value)}
    />
  );

  return (
    <div className="field">
      <span className="field-label-row">
        <span>{label}</span>
        <span className="field-actions">
          {kept && itemId && field && (
            <IconButton
              icon={ICONS.show}
              size="sm"
              onClick={() => void reveal()}
              label={t('{label} zeigen', { label })}
            />
          )}
          {children}
          {!kept && itemId && field && (
            <IconButton
              icon={ICONS.undo}
              size="sm"
              onClick={() => onChange(keep(true))}
              label={t('{label} unverändert lassen', { label })}
            />
          )}
        </span>
      </span>
      {input}
      {kept ? (
        <small className="field-hint">{t('Bleibt, wie es ist.')}</small>
      ) : cleared ? (
        <small className="field-hint" data-tone="warn">
          {t('Wird beim Speichern geleert.')}
        </small>
      ) : (
        hint && <small className="field-hint">{hint}</small>
      )}
    </div>
  );
}

type Props = {
  /** The item to change, or `null` with a kind for a new one. */
  summary: ItemSummary | null;
  kind: ItemKind;
  overview: Overview | null;
  onClose: () => void;
  onSaved: (id: string) => void;
};

export function ItemEditor({ summary, kind, overview, onClose, onSaved }: Props) {
  useLanguage();
  const [form, setForm] = useState<Form>(() => emptyForm(kind));
  const [initial, setInitial] = useState<string>(() => JSON.stringify(emptyForm(kind)));
  const [loading, setLoading] = useState(Boolean(summary));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [generator, setGenerator] = useState<null | 'password' | 'wifi'>(null);
  const [nextKey, setNextKey] = useState(1000);
  /** A masked address made here for a new item, linked to it once it is saved. */
  const [newMasked, setNewMasked] = useState<string | null>(null);
  const [masking, setMasking] = useState(false);
  const uwu = useUwu();
  const id = summary?.id ?? null;
  const remindable = has(uwu, 'reminders') && kind === 'login';
  const reminderBefore = id ? uwu.reminders[id] : undefined;
  const [reminder, setReminder] = useState<ReminderDraft>(() => reminderDraft(reminderBefore));
  const [reminderInitial, setReminderInitial] = useState(() =>
    JSON.stringify(reminderDraft(reminderBefore)),
  );

  useEffect(() => {
    if (!summary) return;
    let stopped = false;
    vaultItem(summary.id)
      .then((detail) => {
        if (stopped) return;
        if (detail.locked) {
          setError(t('Dieser Eintrag fragt zuerst nach deinem Master-Passwort.'));
          return;
        }
        const next = formOf(summary, detail);
        setForm(next);
        setInitial(JSON.stringify(next));
      })
      .catch((e) => !stopped && setError(errorText(e)))
      .finally(() => !stopped && setLoading(false));
    return () => {
      stopped = true;
    };
  }, [summary]);

  const set = (patch: Partial<Form>) => setForm((current) => ({ ...current, ...patch }));
  const setWifi = (patch: Partial<WifiForm>) =>
    setForm((current) => ({ ...current, wifi: { ...current.wifi, ...patch } }));
  /** The SSID, and the name with it as long as the name was the SSID (or empty). */
  const setSsid = (ssid: string) =>
    setForm((current) => ({
      ...current,
      name: !current.name.trim() || current.name === current.wifi.ssid ? ssid : current.name,
      wifi: { ...current.wifi, ssid },
    }));
  const dirty = useMemo(
    () => JSON.stringify(form) !== initial || JSON.stringify(reminder) !== reminderInitial,
    [form, initial, reminder, reminderInitial],
  );
  const guard = useCloseGuard(dirty && !busy, onClose);

  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (!form.name.trim()) {
      setError(t('Ohne Namen findest du den Eintrag später nicht wieder.'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const saved = await saveItem(id, draftOf(form, kind));
      if (newMasked && !id) await linkMaskedAddress(newMasked, saved).catch((e) => toastError(e));
      // The item is saved either way; a reminder that didn't take says so.
      if (remindable)
        await saveReminder(saved, reminder, reminderBefore).catch((e) => toastError(e));
      setInitial(JSON.stringify(form));
      setReminderInitial(JSON.stringify(reminder));
      toast(id ? t('Gespeichert ✧') : t('Angelegt ✧'));
      playNyu('saved');
      onSaved(saved);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const key = () => {
    setNextKey((n) => n + 1);
    return nextKey;
  };

  /** A new masked address for the site of the first address, as the username. */
  const mask = async () => {
    setMasking(true);
    try {
      const site = form.uris.find((uri) => uri.uri.trim())?.uri.trim() ?? null;
      const address = await createMaskedAddress(site, form.name.trim() || null, id);
      if (!id) setNewMasked(address.id);
      set({ username: address.email });
      toast(t('Maskierte Adresse erstellt ✧'));
    } catch (e) {
      toastError(e);
    } finally {
      setMasking(false);
    }
  };

  const folders = [...(overview?.folders ?? [])].sort((a, b) => a.name.localeCompare(b.name));

  return (
    <>
      <Modal
        title={
          id
            ? t('{kind} bearbeiten', { kind: t(KIND_LABEL[kind]) })
            : t('Neu: {kind}', { kind: t(KIND_LABEL[kind]) })
        }
        size="wide"
        onCancel={guard.request}
        footer={
          <>
            <Button variant="ghost" data-secondary onClick={guard.request}>
              {t('Abbrechen')}
            </Button>
            <span className="spacer" />
            <Button
              variant="primary"
              type="submit"
              form="item-editor"
              disabled={busy || loading || !form.name.trim()}
            >
              {busy ? t('Speichert …') : t('Speichern')}
            </Button>
          </>
        }
      >
        {loading ? (
          <NyuBusy label={t('Einen Moment …')} />
        ) : (
          <form id="item-editor" className="editor" onSubmit={save}>
            {error && (
              <p className="form-error" role="alert">
                {error}
              </p>
            )}

            <div className="editor-row">
              <div className="field">
                <span className="field-label-row">
                  <label htmlFor="editor-name">{t('Name')}</label>
                  <span className="field-actions">
                    <IconButton
                      icon={ICONS.favorite}
                      size="sm"
                      className={
                        form.favorite
                          ? 'star-toggle text-pink! [&_svg]:fill-current'
                          : 'star-toggle'
                      }
                      aria-pressed={form.favorite}
                      label={form.favorite ? t('Favorit entfernen') : t('Zu Favoriten')}
                      onClick={() => set({ favorite: !form.favorite })}
                    />
                  </span>
                </span>
                <input
                  id="editor-name"
                  type="text"
                  value={form.name}
                  autoFocus
                  maxLength={200}
                  onChange={(e) => set({ name: e.target.value })}
                />
              </div>
              <Field label={t('Ordner')}>
                <select
                  value={form.folderId}
                  onChange={(e) => set({ folderId: e.target.value })}
                  disabled={Boolean(summary?.organizationId)}
                >
                  <option value="">{t('Ohne Ordner')}</option>
                  {folders.map((folder) => (
                    <option key={folder.id} value={folder.id}>
                      {folder.name}
                    </option>
                  ))}
                </select>
              </Field>
            </div>

            {kind === 'login' && (
              <>
                <div className="field">
                  <span className="field-label-row">
                    <label htmlFor="editor-username">{t('Benutzername')}</label>
                    {has(uwu, 'masked-addresses') && (
                      <span className="field-actions">
                        <IconButton
                          icon={ICONS.maskedAddress}
                          size="sm"
                          disabled={masking}
                          onClick={() => void mask()}
                          label={t('Neue maskierte Adresse')}
                        />
                      </span>
                    )}
                  </span>
                  <input
                    id="editor-username"
                    type="text"
                    value={form.username}
                    autoComplete="off"
                    onChange={(e) => set({ username: e.target.value })}
                  />
                </div>
                <SecretField
                  label={t('Passwort')}
                  secret={form.password}
                  onChange={(password) => set({ password })}
                  itemId={id}
                  field="password"
                >
                  <IconButton
                    icon={ICONS.generate}
                    size="sm"
                    onClick={() => setGenerator('password')}
                    label={t('Passwort-Generator')}
                  />
                </SecretField>
                <SecretField
                  label={t('Einmal-Code (TOTP)')}
                  secret={form.totp}
                  onChange={(totp) => set({ totp })}
                  itemId={id}
                  field="totp"
                  hint={t('Der Schlüssel aus der App: Base32 oder eine otpauth://-Adresse.')}
                />

                <fieldset className="editor-list">
                  <legend>{t('Websites')}</legend>
                  {form.uris.map((uri, index) => (
                    <div className="editor-row" key={uri.key}>
                      <input
                        type="text"
                        value={uri.uri}
                        spellCheck={false}
                        placeholder="https://…"
                        aria-label={t('Adresse {n}', { n: index + 1 })}
                        onChange={(e) =>
                          set({
                            uris: form.uris.map((row) =>
                              row.key === uri.key ? { ...row, uri: e.target.value } : row,
                            ),
                          })
                        }
                      />
                      <select
                        value={uri.match ?? ''}
                        aria-label={t('Wann diese Adresse passt')}
                        onChange={(e) =>
                          set({
                            uris: form.uris.map((row) =>
                              row.key === uri.key
                                ? {
                                    ...row,
                                    match: e.target.value === '' ? null : Number(e.target.value),
                                  }
                                : row,
                            ),
                          })
                        }
                      >
                        <option value="">{t('Standard')}</option>
                        {Object.entries(MATCH_LABEL).map(([value, label]) => (
                          <option key={value} value={value}>
                            {t(label)}
                          </option>
                        ))}
                      </select>
                      <IconButton
                        icon={ICONS.delete}
                        size="sm"
                        label={t('Adresse {n} entfernen', { n: index + 1 })}
                        onClick={() =>
                          set({ uris: form.uris.filter((row) => row.key !== uri.key) })
                        }
                      />
                    </div>
                  ))}
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={ICONS.add}
                    className="justify-self-start"
                    onClick={() =>
                      set({ uris: [...form.uris, { key: key(), uri: '', match: null }] })
                    }
                  >
                    {t('Website hinzufügen')}
                  </Button>
                </fieldset>
              </>
            )}

            {kind === 'card' && (
              <>
                <div className="editor-row">
                  <Field label={t('Karteninhaber')}>
                    <input
                      type="text"
                      value={form.cardholderName}
                      onChange={(e) => set({ cardholderName: e.target.value })}
                    />
                  </Field>
                  <Field label={t('Marke')}>
                    <select value={form.brand} onChange={(e) => set({ brand: e.target.value })}>
                      <option value="">{t('Keine Angabe')}</option>
                      {CARD_BRANDS.map((brand) => (
                        <option key={brand} value={brand}>
                          {brand}
                        </option>
                      ))}
                      {form.brand && !CARD_BRANDS.includes(form.brand) && (
                        <option value={form.brand}>{form.brand}</option>
                      )}
                    </select>
                  </Field>
                </div>
                <SecretField
                  label={t('Kartennummer')}
                  secret={form.number}
                  onChange={(number) => set({ number })}
                  itemId={id}
                  field="card-number"
                />
                <div className="editor-row">
                  <Field label={t('Gültig bis (Monat)')}>
                    <select
                      value={form.expMonth}
                      onChange={(e) => set({ expMonth: e.target.value })}
                    >
                      <option value="">—</option>
                      {Array.from({ length: 12 }, (_, i) => String(i + 1)).map((month) => (
                        <option key={month} value={month}>
                          {month.padStart(2, '0')}
                        </option>
                      ))}
                    </select>
                  </Field>
                  <Field label={t('Gültig bis (Jahr)')}>
                    <input
                      type="text"
                      inputMode="numeric"
                      maxLength={4}
                      value={form.expYear}
                      onChange={(e) => set({ expYear: e.target.value.replace(/\D/g, '') })}
                    />
                  </Field>
                  <SecretField
                    label={t('Prüfnummer')}
                    secret={form.code}
                    onChange={(code) => set({ code })}
                    itemId={id}
                    field="card-code"
                  />
                </div>
              </>
            )}

            {kind === 'identity' && (
              <div className="editor-grid">
                {IDENTITY_FIELDS.map((field) =>
                  field.sensitive ? (
                    <SecretField
                      key={field.name}
                      label={t(IDENTITY_LABEL[field.name] ?? field.name)}
                      secret={form.identitySecrets[field.name] ?? keep(false)}
                      onChange={(value) =>
                        set({ identitySecrets: { ...form.identitySecrets, [field.name]: value } })
                      }
                      itemId={id}
                      field={`identity:${field.name}`}
                    />
                  ) : (
                    <Field key={field.name} label={t(IDENTITY_LABEL[field.name] ?? field.name)}>
                      <input
                        type="text"
                        value={form.identity[field.name] ?? ''}
                        onChange={(e) =>
                          set({ identity: { ...form.identity, [field.name]: e.target.value } })
                        }
                      />
                    </Field>
                  ),
                )}
              </div>
            )}

            {kind === 'ssh-key' && (
              <>
                <p className="dialog-lead">
                  {t(
                    'Ein SSH-Schlüssel braucht alle drei Teile – privater Schlüssel, öffentlicher Schlüssel und Fingerprint. Der Server wirft den Eintrag sonst weg.',
                  )}
                </p>
                <SecretField
                  label={t('Privater Schlüssel')}
                  secret={form.privateKey}
                  onChange={(privateKey) => set({ privateKey })}
                  itemId={id}
                  field="ssh-private"
                  multiline
                />
                <Field label={t('Öffentlicher Schlüssel')}>
                  <input
                    type="text"
                    className="mono"
                    value={form.publicKey}
                    spellCheck={false}
                    onChange={(e) => set({ publicKey: e.target.value })}
                  />
                </Field>
                <Field
                  label={t('Fingerprint')}
                  hint={t('Zum Beispiel aus „ssh-keygen -lf schluessel.pub“.')}
                >
                  <input
                    type="text"
                    className="mono"
                    value={form.fingerprint}
                    spellCheck={false}
                    onChange={(e) => set({ fingerprint: e.target.value })}
                  />
                </Field>
              </>
            )}

            {kind === 'wifi' && (
              <>
                <div className="editor-row">
                  <Field label={t('Netzwerkname (SSID)')}>
                    <input
                      type="text"
                      value={form.wifi.ssid}
                      spellCheck={false}
                      autoComplete="off"
                      maxLength={64}
                      onChange={(e) => setSsid(e.target.value)}
                    />
                  </Field>
                  <Field label={t('Sicherheit')}>
                    <select
                      value={form.wifi.security}
                      onChange={(e) => setWifi({ security: e.target.value })}
                    >
                      {SECURITIES.map((security) => (
                        <option key={security} value={security}>
                          {t(SECURITY_LABEL[security] ?? security)}
                        </option>
                      ))}
                      {!(SECURITIES as readonly string[]).includes(form.wifi.security) && (
                        <option value={form.wifi.security}>{form.wifi.security}</option>
                      )}
                    </select>
                  </Field>
                </div>
                {form.wifi.security !== 'None' && (
                  <SecretField
                    label={t('WLAN-Passwort')}
                    secret={form.wifi.password}
                    onChange={(password) => setWifi({ password })}
                    itemId={form.wifi.from.password === undefined ? null : id}
                    field={
                      form.wifi.from.password === undefined
                        ? undefined
                        : `field:${form.wifi.from.password}`
                    }
                  >
                    <IconButton
                      icon={ICONS.generate}
                      size="sm"
                      onClick={() => setGenerator('wifi')}
                      label={t('Passwort-Generator')}
                    />
                  </SecretField>
                )}
                <SwitchRow
                  label={t('Verstecktes Netzwerk (sendet seinen Namen nicht)')}
                  checked={form.wifi.hidden}
                  onChange={(hidden) => setWifi({ hidden })}
                />
                {isEnterprise(form.wifi.security) && (
                  <fieldset className="editor-list">
                    <legend>{t('Enterprise (802.1X)')}</legend>
                    <div className="editor-row">
                      <Field label={t('EAP-Methode')}>
                        <select
                          value={form.wifi.eap}
                          onChange={(e) => setWifi({ eap: e.target.value })}
                        >
                          <option value="">{t('Keine Angabe')}</option>
                          {EAP_METHODS.map((method) => (
                            <option key={method} value={method}>
                              {method}
                            </option>
                          ))}
                          {form.wifi.eap &&
                            !(EAP_METHODS as readonly string[]).includes(form.wifi.eap) && (
                              <option value={form.wifi.eap}>{form.wifi.eap}</option>
                            )}
                        </select>
                      </Field>
                      <Field label={t('Phase 2')}>
                        <select
                          value={form.wifi.phase2}
                          onChange={(e) => setWifi({ phase2: e.target.value })}
                        >
                          <option value="">{t('Keine Angabe')}</option>
                          {PHASE2_METHODS.map((method) => (
                            <option key={method} value={method}>
                              {method === 'none' ? t('Keine') : method}
                            </option>
                          ))}
                          {form.wifi.phase2 &&
                            !(PHASE2_METHODS as readonly string[]).includes(form.wifi.phase2) && (
                              <option value={form.wifi.phase2}>{form.wifi.phase2}</option>
                            )}
                        </select>
                      </Field>
                    </div>
                    <div className="editor-row">
                      <Field label={t('Identität')}>
                        <input
                          type="text"
                          value={form.wifi.identity}
                          spellCheck={false}
                          autoComplete="off"
                          onChange={(e) => setWifi({ identity: e.target.value })}
                        />
                      </Field>
                      <Field label={t('Anonyme Identität')}>
                        <input
                          type="text"
                          value={form.wifi.anonymous}
                          spellCheck={false}
                          autoComplete="off"
                          onChange={(e) => setWifi({ anonymous: e.target.value })}
                        />
                      </Field>
                    </div>
                    <Field
                      label={t('CA-Zertifikat')}
                      hint={t('Die Domain des Servers oder ein Hinweis, welches Zertifikat gilt.')}
                    >
                      <input
                        type="text"
                        value={form.wifi.ca}
                        spellCheck={false}
                        autoComplete="off"
                        onChange={(e) => setWifi({ ca: e.target.value })}
                      />
                    </Field>
                  </fieldset>
                )}
              </>
            )}

            <Field label={t('Notizen')}>
              <textarea
                rows={kind === 'note' ? 8 : 3}
                value={form.notes}
                onChange={(e) => set({ notes: e.target.value })}
              />
            </Field>

            <fieldset className="editor-list">
              <legend>{t('Eigene Felder')}</legend>
              {form.fields.map((field, index) => (
                <div className="editor-row" key={field.key}>
                  <input
                    type="text"
                    value={field.name}
                    placeholder={t('Feldname')}
                    aria-label={t('Name von Feld {n}', { n: index + 1 })}
                    onChange={(e) =>
                      set({
                        fields: form.fields.map((row) =>
                          row.key === field.key ? { ...row, name: e.target.value } : row,
                        ),
                      })
                    }
                  />
                  {field.kind === 'linked' ? (
                    <span className="muted flex-1 text-caption">
                      {t('verknüpft mit einem anderen Feld')}
                    </span>
                  ) : field.kind === 'boolean' ? (
                    <label className="check">
                      <input
                        type="checkbox"
                        checked={field.value.value === 'true'}
                        onChange={(e) =>
                          set({
                            fields: form.fields.map((row) =>
                              row.key === field.key
                                ? {
                                    ...row,
                                    value: {
                                      mode: 'value',
                                      value: e.target.checked ? 'true' : 'false',
                                    },
                                  }
                                : row,
                            ),
                          })
                        }
                      />
                      <span>{field.value.value === 'true' ? t('Ja') : t('Nein')}</span>
                    </label>
                  ) : field.kind === 'hidden' ? (
                    <SecretField
                      label={t('Wert')}
                      secret={field.value}
                      itemId={id}
                      field={field.from === null ? undefined : `field:${field.from}`}
                      onChange={(value) =>
                        set({
                          fields: form.fields.map((row) =>
                            row.key === field.key ? { ...row, value } : row,
                          ),
                        })
                      }
                    />
                  ) : (
                    <input
                      type="text"
                      value={field.value.value}
                      aria-label={t('Wert von Feld {n}', { n: index + 1 })}
                      onChange={(e) =>
                        set({
                          fields: form.fields.map((row) =>
                            row.key === field.key
                              ? { ...row, value: { mode: 'value', value: e.target.value } }
                              : row,
                          ),
                        })
                      }
                    />
                  )}
                  <IconButton
                    icon={ICONS.delete}
                    size="sm"
                    label={t('Feld {n} entfernen', { n: index + 1 })}
                    onClick={() =>
                      set({ fields: form.fields.filter((row) => row.key !== field.key) })
                    }
                  />
                </div>
              ))}
              <div className="add-kinds flex flex-wrap gap-1">
                {(['text', 'hidden', 'boolean'] as FieldKind[]).map((fieldKind) => (
                  <Button
                    key={fieldKind}
                    variant="ghost"
                    size="sm"
                    icon={ICONS.add}
                    onClick={() =>
                      set({
                        fields: [
                          ...form.fields,
                          {
                            key: key(),
                            name: '',
                            kind: fieldKind,
                            value: { mode: 'value', value: fieldKind === 'boolean' ? 'false' : '' },
                            from: null,
                          },
                        ],
                      })
                    }
                  >
                    {t(FIELD_KIND_LABEL[fieldKind])}
                  </Button>
                ))}
              </div>
            </fieldset>

            <div className="grid gap-2">
              {remindable && <ReminderEditor value={reminder} onChange={setReminder} />}
              <SwitchRow
                label={t('Vor dem Anzeigen nach dem Master-Passwort fragen')}
                checked={form.reprompt}
                onChange={(reprompt) => set({ reprompt })}
              />
            </div>
          </form>
        )}
      </Modal>
      {guard.dialog}
      {generator && (
        <GeneratorDialog
          onClose={() => setGenerator(null)}
          onUse={(password) => {
            if (generator === 'wifi') setWifi({ password: { mode: 'value', value: password } });
            else set({ password: { mode: 'value', value: password } });
            setGenerator(null);
          }}
        />
      )}
    </>
  );
}
