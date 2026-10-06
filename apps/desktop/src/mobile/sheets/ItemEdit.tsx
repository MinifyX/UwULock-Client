/**
 * Creating and changing an item on a phone or iPad: the same form as the
 * desktop editor (lib/itemForm.ts), as grouped input rows. The icon head
 * opens the icon picker; the choice is applied once the item is saved.
 * Secrets the editor never saw stay as they are unless revealed or typed.
 */

import { haptic, ICONS, ListRow, ListSection } from '@uwusuite/design';
import { useEffect, useId, useMemo, useState, type ReactNode } from 'react';
import {
  deleteItem,
  generatePassword,
  revealField,
  saveItem,
  vaultItem,
  type FieldKind,
  type ItemKind,
  type ItemSummary,
} from '../../lib/api';
import { errorText, toastError } from '../../lib/errors';
import { t, useLanguage } from '../../lib/i18n';
import {
  draftOf,
  emptyForm,
  formOf,
  keep,
  type Form,
  type Sec,
  type WifiForm,
} from '../../lib/itemForm';
import {
  CARD_BRANDS,
  FIELD_KIND_LABEL,
  IDENTITY_FIELDS,
  IDENTITY_LABEL,
  KIND_LABEL,
  SECURITY_LABEL,
} from '../../lib/items';
import { note } from '../../lib/toast';
import {
  applyIconChoice,
  createMaskedAddress,
  has,
  linkMaskedAddress,
  useUwu,
  type IconChoice,
} from '../../lib/uwu';
import { EAP_METHODS, isEnterprise, PHASE2_METHODS, SECURITIES } from '../../lib/wifi';
import { loadOptions } from '../../components/GeneratorDialog';
import {
  reminderDraft,
  ReminderEditor,
  saveReminder,
  type ReminderDraft,
} from '../../components/ItemExtras';
import { ItemTile } from '../../components/ItemTile';
import { playNyu } from '../../components/nyu/stage';
import { useMobile } from '../state';
import { EditSurface, FieldInput, GlyphTile, RowButton, Toggle, useConfirm } from '../ui';
import { IconPicker } from './IconPicker';
import { KIND_ICON } from './NewItem';

/** A native select inside a grouped list. */
function SelectRow({
  label,
  value,
  onChange,
  children,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <div className="m-field">
      <label htmlFor={id}>{label}</label>
      <div className="m-field-row">
        <select id={id} value={value} onChange={(event) => onChange(event.target.value)}>
          {children}
        </select>
        <ICONS.expand className="m-select-chevron" aria-hidden />
      </div>
    </div>
  );
}

/** A secret the editor may not have seen: the eye fetches it, typing replaces it. */
function SecretInput({
  label,
  secret,
  onChange,
  itemId,
  field,
  multiline,
  extra,
}: {
  label: string;
  secret: Sec;
  onChange: (next: Sec) => void;
  itemId: string | null;
  field?: string;
  multiline?: boolean;
  extra?: ReactNode;
}) {
  useLanguage();
  const kept = secret.mode === 'keep';
  const canReveal = kept && Boolean(itemId && field);
  const reveal = async () => {
    if (!itemId || !field) return;
    try {
      onChange({ mode: 'value', value: await revealField(itemId, field) });
    } catch (e) {
      toastError(e);
    }
  };
  return (
    <FieldInput
      label={label}
      value={secret.value}
      placeholder={kept ? '••••••••••••' : undefined}
      mono
      multiline={multiline}
      rows={4}
      onChange={(value) => {
        // Deleting what was typed goes back to leaving the value alone.
        if (value === '' && kept) return;
        onChange({ mode: 'value', value });
      }}
      trailing={
        <>
          {canReveal && (
            <RowButton
              icon={ICONS.show}
              label={t('{label} zeigen', { label })}
              onClick={() => void reveal()}
            />
          )}
          {extra}
        </>
      }
    />
  );
}

export function ItemEditSheet({
  open,
  onClose,
  id,
  kind,
  folderId,
}: {
  open: boolean;
  onClose: () => void;
  id: string | null;
  kind: ItemKind;
  folderId?: string | null;
}) {
  useLanguage();
  const { data, openInTab, android } = useMobile();
  const uwu = useUwu();
  const summary: ItemSummary | null = id ? (data.byId(id) ?? null) : null;
  const [form, setForm] = useState<Form>(() => ({ ...emptyForm(kind), folderId: folderId ?? '' }));
  const [initial, setInitial] = useState(() => JSON.stringify(form));
  const [loading, setLoading] = useState(Boolean(id));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [picking, setPicking] = useState(false);
  const [icon, setIcon] = useState<IconChoice>(null);
  const [newMasked, setNewMasked] = useState<string | null>(null);
  const [nextKey, setNextKey] = useState(1000);
  const confirm = useConfirm();
  const remindable = has(uwu, 'reminders') && kind === 'login';
  const reminderBefore = id ? uwu.reminders[id] : undefined;
  const [reminder, setReminder] = useState<ReminderDraft>(() => reminderDraft(reminderBefore));
  const [reminderInitial] = useState(() => JSON.stringify(reminderDraft(reminderBefore)));

  useEffect(() => {
    if (!id || !summary) return;
    let stopped = false;
    vaultItem(id)
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
    // Loaded once per item; a sync while editing doesn't overwrite the form.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  const set = (patch: Partial<Form>) => setForm((current) => ({ ...current, ...patch }));
  const setWifi = (patch: Partial<WifiForm>) =>
    setForm((current) => ({ ...current, wifi: { ...current.wifi, ...patch } }));
  const setSsid = (ssid: string) =>
    setForm((current) => ({
      ...current,
      name: !current.name.trim() || current.name === current.wifi.ssid ? ssid : current.name,
      wifi: { ...current.wifi, ssid },
    }));
  const key = () => {
    setNextKey((n) => n + 1);
    return nextKey;
  };
  const dirty = useMemo(
    () =>
      JSON.stringify(form) !== initial ||
      JSON.stringify(reminder) !== reminderInitial ||
      icon !== null,
    [form, initial, reminder, reminderInitial, icon],
  );

  const close = () => {
    if (dirty && !busy)
      confirm.ask({
        title: t('Änderungen verwerfen?'),
        confirm: t('Verwerfen'),
        run: onClose,
      });
    else onClose();
  };

  const generate = async (into: 'password' | 'wifi') => {
    try {
      const made = await generatePassword(loadOptions());
      haptic('light');
      const value: Sec = { mode: 'value', value: made.password };
      if (into === 'wifi') setWifi({ password: value });
      else set({ password: value });
    } catch (e) {
      toastError(e);
    }
  };

  const mask = async () => {
    try {
      const site = form.uris.find((uri) => uri.uri.trim())?.uri.trim() ?? null;
      const address = await createMaskedAddress(site, form.name.trim() || null, id);
      if (!id) setNewMasked(address.id);
      set({ username: address.email });
      note(t('Maskierte Adresse erstellt ✧'), { tone: 'success' });
    } catch (e) {
      toastError(e);
    }
  };

  const save = async () => {
    if (!form.name.trim()) {
      setError(t('Ohne Namen findest du den Eintrag später nicht wieder.'));
      haptic('error');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const saved = await saveItem(id, draftOf(form, kind));
      if (newMasked && !id) await linkMaskedAddress(newMasked, saved).catch((e) => toastError(e));
      if (remindable)
        await saveReminder(saved, reminder, reminderBefore).catch((e) => toastError(e));
      if (icon) await applyIconChoice(saved, icon).catch((e) => toastError(e));
      haptic('success');
      note(id ? t('Gespeichert ✧') : t('Angelegt ✧'), { tone: 'success' });
      playNyu('saved');
      onClose();
      if (!id) {
        await data.reload();
        openInTab({ page: 'item', id: saved });
      }
    } catch (e) {
      setError(errorText(e));
      haptic('error');
    } finally {
      setBusy(false);
    }
  };

  const ownIcons = has(uwu, 'own-icons');
  const hasOwn = Boolean(summary && summary.id in uwu.ownIcons);
  const tile =
    icon && icon !== 'remove' ? (
      <span className="item-tile" data-image aria-hidden>
        <img src={icon.png} alt="" draggable={false} />
      </span>
    ) : summary && icon !== 'remove' ? (
      <ItemTile item={summary} />
    ) : (
      <GlyphTile icon={KIND_ICON[kind]} />
    );
  const iconState =
    icon === 'remove'
      ? t('Wird beim Sichern entfernt')
      : icon
        ? t('Neues Icon, gilt beim Sichern')
        : hasOwn
          ? t('Eigenes Icon')
          : kind === 'login'
            ? t('Icon der Website')
            : t('Icon des Typs');
  const folders = [...(data.overview?.folders ?? [])].sort((a, b) => a.name.localeCompare(b.name));
  const title = id ? t('Bearbeiten') : t('Neu: {kind}', { kind: t(KIND_LABEL[kind]) });

  return (
    <>
      <EditSurface
        open={open}
        onClose={close}
        title={title}
        dirty={dirty}
        action={{
          label: android ? t('Speichern') : t('Sichern'),
          onClick: () => void save(),
          disabled: busy || loading || !form.name.trim(),
        }}
      >
        {error && (
          <p className="m-error" role="alert">
            {error}
          </p>
        )}
        {loading ? (
          <p className="m-footnote">{t('Einen Moment …')}</p>
        ) : (
          <form
            className="m-edit-form"
            onSubmit={(event) => {
              event.preventDefault();
              void save();
            }}
          >
            {ownIcons && (
              <ListSection>
                <div className="m-icon-head">
                  {tile}
                  <span className="m-icon-head-text">
                    <b>{t('Icon')}</b>
                    <span>{iconState}</span>
                  </span>
                  <button type="button" className="m-pill" onClick={() => setPicking(true)}>
                    {t('Ändern')}
                  </button>
                </div>
              </ListSection>
            )}

            <ListSection>
              <FieldInput
                label={t('Name')}
                value={form.name}
                placeholder={t('z. B. GitHub')}
                autoFocus={!id}
                onChange={(name) => set({ name })}
              />
            </ListSection>

            {kind === 'login' && (
              <>
                <ListSection>
                  <FieldInput
                    label={t('Benutzername')}
                    value={form.username}
                    placeholder={t('E-Mail oder Name')}
                    inputMode="email"
                    onChange={(username) => set({ username })}
                    trailing={
                      has(uwu, 'masked-addresses') ? (
                        <RowButton
                          icon={ICONS.maskedAddress}
                          label={t('Neue maskierte Adresse')}
                          onClick={() => void mask()}
                        />
                      ) : undefined
                    }
                  />
                  <SecretInput
                    label={t('Passwort')}
                    secret={form.password}
                    onChange={(password) => set({ password })}
                    itemId={id}
                    field="password"
                    extra={
                      <RowButton
                        icon={ICONS.generate}
                        label={t('Passwort generieren')}
                        onClick={() => void generate('password')}
                      />
                    }
                  />
                  <SecretInput
                    label={t('Einmal-Code (TOTP)')}
                    secret={form.totp}
                    onChange={(totp) => set({ totp })}
                    itemId={id}
                    field="totp"
                  />
                </ListSection>
                <ListSection header={t('Websites')}>
                  {form.uris.map((uri, index) => (
                    <FieldInput
                      key={uri.key}
                      label={t('Website {n}', { n: index + 1 })}
                      value={uri.uri}
                      placeholder="example.com"
                      inputMode="url"
                      onChange={(value) =>
                        set({
                          uris: form.uris.map((row) =>
                            row.key === uri.key ? { ...row, uri: value } : row,
                          ),
                        })
                      }
                      trailing={
                        <RowButton
                          icon={ICONS.delete}
                          label={t('Adresse {n} entfernen', { n: index + 1 })}
                          onClick={() =>
                            set({ uris: form.uris.filter((row) => row.key !== uri.key) })
                          }
                        />
                      }
                    />
                  ))}
                  <ListRow
                    icon={ICONS.add}
                    iconTone="none"
                    title={t('Website hinzufügen')}
                    tone="accent"
                    onClick={() =>
                      set({ uris: [...form.uris, { key: key(), uri: '', match: null }] })
                    }
                  />
                </ListSection>
              </>
            )}

            {kind === 'card' && (
              <ListSection>
                <FieldInput
                  label={t('Karteninhaber')}
                  value={form.cardholderName}
                  onChange={(cardholderName) => set({ cardholderName })}
                />
                <SelectRow
                  label={t('Marke')}
                  value={form.brand}
                  onChange={(brand) => set({ brand })}
                >
                  <option value="">{t('Keine Angabe')}</option>
                  {CARD_BRANDS.map((brand) => (
                    <option key={brand} value={brand}>
                      {brand}
                    </option>
                  ))}
                  {form.brand && !CARD_BRANDS.includes(form.brand) && (
                    <option value={form.brand}>{form.brand}</option>
                  )}
                </SelectRow>
                <SecretInput
                  label={t('Kartennummer')}
                  secret={form.number}
                  onChange={(number) => set({ number })}
                  itemId={id}
                  field="card-number"
                />
                <SelectRow
                  label={t('Gültig bis (Monat)')}
                  value={form.expMonth}
                  onChange={(expMonth) => set({ expMonth })}
                >
                  <option value="">—</option>
                  {Array.from({ length: 12 }, (_, i) => String(i + 1)).map((month) => (
                    <option key={month} value={month}>
                      {month.padStart(2, '0')}
                    </option>
                  ))}
                </SelectRow>
                <FieldInput
                  label={t('Gültig bis (Jahr)')}
                  value={form.expYear}
                  inputMode="numeric"
                  maxLength={4}
                  onChange={(value) => set({ expYear: value.replace(/\D/g, '') })}
                />
                <SecretInput
                  label={t('Prüfnummer')}
                  secret={form.code}
                  onChange={(code) => set({ code })}
                  itemId={id}
                  field="card-code"
                />
              </ListSection>
            )}

            {kind === 'identity' && (
              <ListSection>
                {IDENTITY_FIELDS.map((field) =>
                  field.sensitive ? (
                    <SecretInput
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
                    <FieldInput
                      key={field.name}
                      label={t(IDENTITY_LABEL[field.name] ?? field.name)}
                      value={form.identity[field.name] ?? ''}
                      onChange={(value) =>
                        set({ identity: { ...form.identity, [field.name]: value } })
                      }
                    />
                  ),
                )}
              </ListSection>
            )}

            {kind === 'ssh-key' && (
              <ListSection>
                <SecretInput
                  label={t('Privater Schlüssel')}
                  secret={form.privateKey}
                  onChange={(privateKey) => set({ privateKey })}
                  itemId={id}
                  field="ssh-private"
                  multiline
                />
                <FieldInput
                  label={t('Öffentlicher Schlüssel')}
                  value={form.publicKey}
                  mono
                  multiline
                  onChange={(publicKey) => set({ publicKey })}
                />
                <FieldInput
                  label={t('Fingerprint')}
                  value={form.fingerprint}
                  mono
                  onChange={(fingerprint) => set({ fingerprint })}
                />
              </ListSection>
            )}

            {kind === 'wifi' && (
              <ListSection>
                <FieldInput
                  label={t('Netzwerkname (SSID)')}
                  value={form.wifi.ssid}
                  maxLength={64}
                  onChange={setSsid}
                />
                <SelectRow
                  label={t('Sicherheit')}
                  value={form.wifi.security}
                  onChange={(security) => setWifi({ security })}
                >
                  {SECURITIES.map((security) => (
                    <option key={security} value={security}>
                      {t(SECURITY_LABEL[security] ?? security)}
                    </option>
                  ))}
                  {!(SECURITIES as readonly string[]).includes(form.wifi.security) && (
                    <option value={form.wifi.security}>{form.wifi.security}</option>
                  )}
                </SelectRow>
                {form.wifi.security !== 'None' && (
                  <SecretInput
                    label={t('WLAN-Passwort')}
                    secret={form.wifi.password}
                    onChange={(password) => setWifi({ password })}
                    itemId={form.wifi.from.password === undefined ? null : id}
                    field={
                      form.wifi.from.password === undefined
                        ? undefined
                        : `field:${form.wifi.from.password}`
                    }
                    extra={
                      <RowButton
                        icon={ICONS.generate}
                        label={t('Passwort generieren')}
                        onClick={() => void generate('wifi')}
                      />
                    }
                  />
                )}
                <ListRow
                  title={t('Verstecktes Netzwerk (sendet seinen Namen nicht)')}
                  wrap
                  trailing={
                    <Toggle
                      label={t('Verstecktes Netzwerk')}
                      checked={form.wifi.hidden}
                      onChange={(hidden) => setWifi({ hidden })}
                    />
                  }
                />
                {isEnterprise(form.wifi.security) && (
                  <>
                    <SelectRow
                      label={t('EAP-Methode')}
                      value={form.wifi.eap}
                      onChange={(eap) => setWifi({ eap })}
                    >
                      <option value="">{t('Keine Angabe')}</option>
                      {EAP_METHODS.map((method) => (
                        <option key={method} value={method}>
                          {method}
                        </option>
                      ))}
                    </SelectRow>
                    <SelectRow
                      label={t('Phase 2')}
                      value={form.wifi.phase2}
                      onChange={(phase2) => setWifi({ phase2 })}
                    >
                      <option value="">{t('Keine Angabe')}</option>
                      {PHASE2_METHODS.map((method) => (
                        <option key={method} value={method}>
                          {method === 'none' ? t('Keine') : method}
                        </option>
                      ))}
                    </SelectRow>
                    <FieldInput
                      label={t('Identität')}
                      value={form.wifi.identity}
                      onChange={(identity) => setWifi({ identity })}
                    />
                    <FieldInput
                      label={t('Anonyme Identität')}
                      value={form.wifi.anonymous}
                      onChange={(anonymous) => setWifi({ anonymous })}
                    />
                    <FieldInput
                      label={t('CA-Zertifikat')}
                      value={form.wifi.ca}
                      onChange={(ca) => setWifi({ ca })}
                    />
                  </>
                )}
              </ListSection>
            )}

            <div className="m-gap" />
            <ListSection>
              <SelectRow
                label={t('Ordner')}
                value={form.folderId}
                onChange={(next) => set({ folderId: next })}
              >
                <option value="">{t('Ohne Ordner')}</option>
                {folders.map((folder) => (
                  <option key={folder.id} value={folder.id}>
                    {folder.name}
                  </option>
                ))}
              </SelectRow>
              <ListRow
                icon={ICONS.favorite}
                iconTone="warning"
                title={t('Favorit')}
                trailing={
                  <Toggle
                    label={t('Favorit')}
                    checked={form.favorite}
                    onChange={(favorite) => set({ favorite })}
                  />
                }
              />
              <ListRow
                icon={ICONS.masterPassword}
                iconTone="neutral"
                title={t('Vor dem Anzeigen fragen')}
                subtitle={t('nach dem Master-Passwort')}
                trailing={
                  <Toggle
                    label={t('Vor dem Anzeigen nach dem Master-Passwort fragen')}
                    checked={form.reprompt}
                    onChange={(reprompt) => set({ reprompt })}
                  />
                }
              />
            </ListSection>

            {remindable && (
              <div className="m-desktop-block">
                <ReminderEditor value={reminder} onChange={setReminder} />
              </div>
            )}

            <ListSection header={t('Notizen')}>
              <FieldInput
                label={t('Notizen')}
                value={form.notes}
                multiline
                rows={kind === 'note' ? 8 : 3}
                placeholder={t('Notiz')}
                onChange={(notes) => set({ notes })}
              />
            </ListSection>

            <ListSection header={t('Eigene Felder')}>
              {form.fields.map((field, index) => {
                const rename = (name: string) =>
                  set({
                    fields: form.fields.map((row) =>
                      row.key === field.key ? { ...row, name } : row,
                    ),
                  });
                const revalue = (value: Sec) =>
                  set({
                    fields: form.fields.map((row) =>
                      row.key === field.key ? { ...row, value } : row,
                    ),
                  });
                const remove = (
                  <RowButton
                    icon={ICONS.delete}
                    label={t('Feld {n} entfernen', { n: index + 1 })}
                    onClick={() =>
                      set({ fields: form.fields.filter((row) => row.key !== field.key) })
                    }
                  />
                );
                return (
                  <div key={field.key} className="m-custom-field">
                    <FieldInput
                      label={t('Feldname')}
                      value={field.name}
                      onChange={rename}
                      trailing={remove}
                    />
                    {field.kind === 'linked' ? (
                      <ListRow title={t('verknüpft mit einem anderen Feld')} />
                    ) : field.kind === 'boolean' ? (
                      <ListRow
                        title={field.value.value === 'true' ? t('Ja') : t('Nein')}
                        trailing={
                          <Toggle
                            label={t('Wert von Feld {n}', { n: index + 1 })}
                            checked={field.value.value === 'true'}
                            onChange={(on) =>
                              revalue({ mode: 'value', value: on ? 'true' : 'false' })
                            }
                          />
                        }
                      />
                    ) : field.kind === 'hidden' ? (
                      <SecretInput
                        label={t('Wert')}
                        secret={field.value}
                        onChange={revalue}
                        itemId={id}
                        field={field.from === null ? undefined : `field:${field.from}`}
                      />
                    ) : (
                      <FieldInput
                        label={t('Wert')}
                        value={field.value.value}
                        onChange={(value) => revalue({ mode: 'value', value })}
                      />
                    )}
                  </div>
                );
              })}
              {(['text', 'hidden', 'boolean'] as FieldKind[]).map((fieldKind) => (
                <ListRow
                  key={fieldKind}
                  icon={ICONS.add}
                  iconTone="none"
                  tone="accent"
                  title={t('Eigenes Feld: {kind}', { kind: t(FIELD_KIND_LABEL[fieldKind]) })}
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
                />
              ))}
            </ListSection>

            {id && summary && !summary.deleted && (
              <>
                <div className="m-gap" />
                <ListSection>
                  <ListRow
                    title={t('In den Papierkorb')}
                    tone="danger"
                    onClick={() =>
                      confirm.ask({
                        title: t('In den Papierkorb?'),
                        text: t(
                          '„{name}“ wandert in den Papierkorb. Der Server hebt ihn dort noch 30 Tage auf.',
                          { name: summary.name || t('(ohne Namen)') },
                        ),
                        confirm: t('In den Papierkorb'),
                        run: () =>
                          void deleteItem(summary.id, false)
                            .then(() => {
                              playNyu('trashed');
                              note(t('Im Papierkorb.'));
                              onClose();
                            })
                            .catch((e) => toastError(e)),
                      })
                    }
                  />
                </ListSection>
              </>
            )}
            <div className="m-sheet-end" />
          </form>
        )}
      </EditSurface>
      {ownIcons && (
        <IconPicker
          open={open && picking}
          onClose={() => setPicking(false)}
          summary={summary}
          name={form.name}
          uris={form.uris.map((row) => row.uri)}
          value={icon}
          onChange={setIcon}
        />
      )}
      {confirm.element}
    </>
  );
}
