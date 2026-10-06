/**
 * Sends: a text or a file behind a link, for somebody without an account.
 * The link carries the key; the server keeps only what it can't read, and
 * forgets it on the deletion date. Here the owner sees their Sends, makes
 * new ones, changes them, copies a link again, opens one to anybody again,
 * and deletes them — as in the web vault. The page that opens a Send is the
 * web vault's (or Bitwarden's).
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { Button, Icon, IconButton, ICONS, Tag } from '@uwusuite/design';
import { copyGenerated } from '../lib/api';
import { errorText, toastError } from '../lib/errors';
import { when } from '../lib/format';
import { t, useLanguage } from '../lib/i18n';
import {
  SEND_DAYS,
  sendDraft,
  sendForm,
  sendFormReady,
  sendStatus,
  sortSends,
  splitAddresses,
  type Send,
  type SendAccess,
  type SendForm,
  type SendKind,
} from '../lib/sendModel';
import { deleteSend, removeSendAuth, saveSend, sends, stageSendFile } from '../lib/sends';
import { getSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import { has, sendOptions, useUwu, type SendOptions } from '../lib/uwu';
import { Modal } from './Modal';
import { NyuBusy } from './nyu/stage';
import { PasswordInput } from './PasswordInput';

type View =
  | { kind: 'list' }
  | { kind: 'send'; id: string }
  | { kind: 'form'; id: string | null; sendKind: SendKind };

function size(bytes: number | null | undefined): string {
  if (!bytes) return '0 B';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

async function copyLink(link: string) {
  try {
    await copyGenerated(link);
    const seconds = getSettings().clipboardClear;
    toast(
      seconds > 0
        ? t('Link kopiert ✧ – wird nach {n} s geleert', { n: seconds })
        : t('Link kopiert ✧'),
    );
  } catch (e) {
    toastError(e);
  }
}

/** Whether it opens right now, in a few words. */
export function sendStatusText(send: Send): string {
  const status = sendStatus(send);
  switch (status.kind) {
    case 'disabled':
      return t('Deaktiviert');
    case 'expired':
      return t('Abgelaufen');
    case 'used-up':
      return t('So oft geöffnet, wie erlaubt');
    default:
      return t('Bis {when}', { when: when(status.until) ?? '' });
  }
}

function openedText(send: Send): string {
  return send.maxAccessCount
    ? t('{n} von {max} Mal geöffnet', { n: send.accessCount, max: send.maxAccessCount })
    : t('{n} Mal geöffnet', { n: send.accessCount });
}

const kindIcon = (kind: SendKind) => (kind === 1 ? ICONS.file : ICONS.note);

export function SendsDialog({ onClose }: { onClose: () => void }) {
  useLanguage();
  const [view, setView] = useState<View>({ kind: 'list' });
  const [list, setList] = useState<Send[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setList(sortSends(await sends()));
      setError(null);
    } catch (e) {
      setError(errorText(e));
      setList((current) => current ?? []);
    }
  }, []);

  useEffect(() => {
    void load();
    // A sync (here or from another device) brings changed Sends.
    const stop = listen('vault-changed', () => void load());
    return () => void stop.then((unlisten) => unlisten());
  }, [load]);

  const current =
    view.kind !== 'list' && view.id ? (list?.find((send) => send.id === view.id) ?? null) : null;
  // Deleted elsewhere while open: back to the list.
  useEffect(() => {
    if (view.kind === 'send' && list && !current) setView({ kind: 'list' });
  }, [view.kind, list, current]);

  return (
    <Modal
      title={
        view.kind === 'form'
          ? view.id
            ? t('Send bearbeiten')
            : view.sendKind === 1
              ? t('Neuer Datei-Send')
              : t('Neuer Text-Send')
          : current
            ? current.name || t('(ohne Namen)')
            : t('Sends')
      }
      size="wide"
      onCancel={
        view.kind === 'list'
          ? onClose
          : view.kind === 'form' && current
            ? () => setView({ kind: 'send', id: current.id })
            : () => setView({ kind: 'list' })
      }
      footer={
        view.kind === 'list' ? (
          <>
            <Button variant="ghost" onClick={onClose}>
              {t('Schließen')}
            </Button>
            <span className="spacer" />
            <Button
              icon={ICONS.add}
              onClick={() => setView({ kind: 'form', id: null, sendKind: 1 })}
            >
              {t('Datei')}
            </Button>
            <Button
              variant="primary"
              icon={ICONS.add}
              onClick={() => setView({ kind: 'form', id: null, sendKind: 0 })}
            >
              {t('Text')}
            </Button>
          </>
        ) : view.kind === 'send' && current ? (
          <>
            <Button variant="ghost" onClick={() => setView({ kind: 'list' })}>
              {t('Zurück')}
            </Button>
            <span className="spacer" />
            <Button
              icon={ICONS.edit}
              onClick={() => setView({ kind: 'form', id: current.id, sendKind: current.kind })}
            >
              {t('Bearbeiten')}
            </Button>
            <Button variant="primary" icon={ICONS.copy} onClick={() => void copyLink(current.link)}>
              {t('Link kopieren')}
            </Button>
          </>
        ) : undefined
      }
    >
      <div className="extras-scroll">
        {error && (
          <p className="notice" data-tone="error">
            {error}
          </p>
        )}
        {view.kind === 'list' && (
          <SendList sends={list} onOpen={(id) => setView({ kind: 'send', id })} />
        )}
        {view.kind === 'send' && current && (
          <SendDetail
            send={current}
            onChanged={() => void load()}
            onDeleted={() => {
              setView({ kind: 'list' });
              void load();
            }}
          />
        )}
        {view.kind === 'form' && (
          <SendEditor
            send={current}
            kind={view.sendKind}
            onCancel={() => setView(current ? { kind: 'send', id: current.id } : { kind: 'list' })}
            onSaved={async (id) => {
              await load();
              setView({ kind: 'send', id });
            }}
          />
        )}
      </div>
    </Modal>
  );
}

function SendList({ sends, onOpen }: { sends: Send[] | null; onOpen: (id: string) => void }) {
  useLanguage();
  if (sends === null) return <NyuBusy label={t('Einen Moment …')} />;
  if (sends.length === 0)
    return (
      <p className="dialog-lead">
        {t(
          'Noch keine Sends. Ein Send ist ein Text oder eine Datei hinter einem Link – für jemanden ohne Konto, verschlüsselt, und nach einer Frist wieder weg.',
        )}
      </p>
    );
  return (
    <ul className="extras-list">
      {sends.map((send) => (
        <li key={send.id}>
          <button type="button" className="extras-row" onClick={() => onOpen(send.id)}>
            <Icon icon={kindIcon(send.kind)} size="sm" className="text-muted" />
            <span className="extras-row-text">
              <span className="item-name">{send.name || t('(ohne Namen)')}</span>
              <span className="item-sub">
                {[sendStatusText(send), openedText(send)].join(' · ')}
              </span>
            </span>
            {send.authType === 1 && (
              <Icon
                icon={ICONS.locked}
                size="xs"
                className="text-muted"
                aria-label={t('Mit Passwort')}
              />
            )}
            {send.authType === 0 && (
              <Icon
                icon={ICONS.account}
                size="xs"
                className="text-muted"
                aria-label={t('Nur bestimmte Adressen')}
              />
            )}
            <Icon icon={ICONS.next} size="xs" className="text-faint" />
          </button>
        </li>
      ))}
    </ul>
  );
}

function SendDetail({
  send,
  onChanged,
  onDeleted,
}: {
  send: Send;
  onChanged: () => void;
  onDeleted: () => void;
}) {
  useLanguage();
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const status = sendStatus(send);

  const act = async (what: () => Promise<unknown>, done: string, after: () => void) => {
    setBusy(true);
    try {
      await what();
      toast(done);
      after();
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
      setAsking(false);
    }
  };

  return (
    <div className="extras-form">
      <p className="flex flex-wrap gap-1">
        <Tag>{send.kind === 1 ? t('Datei') : t('Text')}</Tag>
        {status.kind === 'active' ? (
          <Tag tone="success">{t('Aktiv')}</Tag>
        ) : (
          <Tag tone="warning">{sendStatusText(send)}</Tag>
        )}
        {send.authType === 1 && <Tag>{t('Mit Passwort')}</Tag>}
        {send.authType === 0 && <Tag>{t('Nur bestimmte Adressen')}</Tag>}
        {send.hideEmail && <Tag>{t('Ohne deine Adresse')}</Tag>}
      </p>
      <section className="detail-card">
        <div className="detail-row">
          <div className="detail-text">
            <span className="detail-label">{t('Link')}</span>
            <span className="detail-value mono uri">{send.link}</span>
          </div>
          <div className="detail-actions">
            <IconButton
              icon={ICONS.copy}
              label={t('Link kopieren')}
              size="sm"
              onClick={() => void copyLink(send.link)}
            />
          </div>
        </div>
        {send.kind === 0 ? (
          <div className="detail-row">
            <div className="detail-text">
              <span className="detail-label">
                {send.entry ? t('Text · als Eintrag geteilt') : t('Text')}
              </span>
              <span className="detail-value multiline">{send.text ?? ''}</span>
            </div>
          </div>
        ) : (
          <div className="detail-row">
            <div className="detail-text">
              <span className="detail-label">{t('Datei')}</span>
              <span className="detail-value">
                {send.fileName ?? ''} {send.size !== null && `(${size(send.size)})`}
              </span>
            </div>
          </div>
        )}
        {send.notes && (
          <div className="detail-row">
            <div className="detail-text">
              <span className="detail-label">{t('Notizen (nur für dich)')}</span>
              <span className="detail-value multiline">{send.notes}</span>
            </div>
          </div>
        )}
      </section>
      <section className="detail-card">
        <div className="detail-row">
          <div className="detail-text">
            <span className="detail-value">{openedText(send)}</span>
            {send.expirationDate && (
              <span className="item-sub">
                {t('Läuft ab {when}', { when: when(send.expirationDate) ?? '' })}
              </span>
            )}
            <span className="item-sub">
              {t('Wird gelöscht {when}', { when: when(send.deletionDate) ?? '' })}
            </span>
            {send.authType === 0 && (
              <span className="item-sub">
                {t('Nur für: {emails}', { emails: send.emails.join(', ') })}
              </span>
            )}
          </div>
        </div>
      </section>
      <div className="form-actions">
        <Button variant="ghost" icon={ICONS.delete} disabled={busy} onClick={() => setAsking(true)}>
          {t('Löschen')}
        </Button>
        <span className="spacer" />
        {send.authType !== 2 && (
          <Button
            variant="ghost"
            icon={ICONS.unlocked}
            disabled={busy}
            onClick={() =>
              void act(
                () => removeSendAuth(send.id),
                send.authType === 0 ? t('Adressen entfernt.') : t('Passwort entfernt.'),
                onChanged,
              )
            }
          >
            {send.authType === 0 ? t('Adressen entfernen') : t('Passwort entfernen')}
          </Button>
        )}
      </div>

      {asking && (
        <Modal
          title={t('Send löschen?')}
          tone="warning"
          size="small"
          onCancel={() => setAsking(false)}
          footer={
            <>
              <span className="spacer" />
              <Button
                variant="danger"
                data-secondary
                disabled={busy}
                onClick={() => void act(() => deleteSend(send.id), t('Gelöscht.'), onDeleted)}
              >
                {t('Löschen')}
              </Button>
              <Button variant="primary" data-autofocus onClick={() => setAsking(false)}>
                {t('Abbrechen')}
              </Button>
            </>
          }
        >
          <p className="dialog-lead">{t('Der Link öffnet danach nichts mehr.')}</p>
        </Modal>
      )}
    </div>
  );
}

const days = (n: number) => (n === 1 ? t('1 Tag') : t('{n} Tage', { n }));

function SendEditor({
  send,
  kind,
  onCancel,
  onSaved,
}: {
  send: Send | null;
  kind: SendKind;
  onCancel: () => void;
  onSaved: (id: string) => Promise<void>;
}) {
  useLanguage();
  const uwu = useUwu();
  const [form, setForm] = useState<SendForm>(() => sendForm(send));
  const [file, setFile] = useState<File | null>(null);
  const [options, setOptions] = useState<SendOptions | null>(null);
  // Where the link points; `undefined` until the account's default is known.
  const [domain, setDomain] = useState<string | null | undefined>(
    send ? send.sendDomainId : undefined,
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const set = (patch: Partial<SendForm>) => setForm((current) => ({ ...current, ...patch }));
  const maxBytes = uwu.limits?.maxFileBytes ?? null;

  useEffect(() => {
    let gone = false;
    sendOptions().then(
      (found) => {
        if (gone) return;
        setOptions(found);
        // A new Send starts on the account's default domain, which the server gives it anyway.
        setDomain((current) => (current === undefined ? found.defaultDomainId : current));
      },
      () => {
        if (gone) return;
        setOptions({ emails: false, domains: [], defaultDomainId: null });
        setDomain((current) => (current === undefined ? null : current));
      },
    );
    return () => {
      gone = true;
    };
  }, []);

  const domains = has(uwu, 'send-domains') ? (options?.domains ?? []) : [];
  const mailOk = Boolean(options?.emails) || form.access === 0;
  const ready = options !== null && sendFormReady(form, send, kind);

  const save = async () => {
    if (!ready || busy) return;
    setBusy(true);
    setError(null);
    try {
      if (kind === 1 && !send) {
        if (!file) return;
        await stageSendFile(file);
      }
      const draft = sendDraft(form, send, kind);
      const id = await saveSend(send?.id ?? null, draft, domains.length ? domain : undefined);
      // A new Send's link is what the person wants next.
      const link = send ? null : ((await sends()).find((s) => s.id === id)?.link ?? null);
      if (link) await copyGenerated(link).catch(() => undefined);
      toast(
        send
          ? t('Gespeichert ✧')
          : link
            ? t('Send angelegt – der Link ist kopiert ✧')
            : t('Send angelegt ✧'),
      );
      await onSaved(id);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const accessOptions: { value: SendAccess; label: string }[] = [
    { value: 2, label: t('Jeder mit dem Link') },
    { value: 1, label: t('Mit Passwort') },
    { value: 0, label: t('Nur bestimmte Adressen') },
  ];

  return (
    <form
      className="extras-form"
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      <label className="field">
        <span>{t('Name')}</span>
        <input
          type="text"
          value={form.name}
          maxLength={200}
          autoFocus
          onChange={(e) => set({ name: e.target.value })}
        />
      </label>
      {kind === 0 && send?.entry ? (
        <div className="field">
          <span>{t('Text · als Eintrag geteilt')}</span>
          <p className="detail-value multiline">{send.text ?? ''}</p>
          <small className="field-hint">
            {t(
              'Ein geteilter Eintrag lässt sich nicht ändern: Sein Inhalt steckt noch ein zweites Mal im Send, für die Send-Seite. Um etwas zurückzunehmen, lösch diesen Send und teil den Eintrag neu.',
            )}
          </small>
        </div>
      ) : kind === 0 ? (
        <label className="field">
          <span>{t('Text')}</span>
          <textarea
            rows={5}
            value={form.text}
            maxLength={1000}
            onChange={(e) => set({ text: e.target.value })}
          />
        </label>
      ) : send ? (
        <p className="field-hint">
          {t('Die Datei eines Sends bleibt, wie sie ist: {name}', { name: send.fileName ?? '' })}
        </p>
      ) : (
        <div className="field">
          <span>{t('Datei')}</span>
          <input
            ref={input}
            type="file"
            hidden
            onChange={(event) => {
              const picked = event.target.files?.[0] ?? null;
              event.target.value = '';
              if (picked && maxBytes && picked.size > maxBytes) {
                setError(t('Die Datei ist zu groß: höchstens {size}.', { size: size(maxBytes) }));
                return;
              }
              setError(null);
              setFile(picked);
              set({ fileName: picked?.name ?? null });
            }}
          />
          <Button
            className="justify-self-start"
            icon={ICONS.upload}
            onClick={() => input.current?.click()}
          >
            {file ? `${file.name} (${size(file.size)})` : t('Datei wählen …')}
          </Button>
        </div>
      )}
      {kind === 0 && (
        <label className="check">
          <input
            type="checkbox"
            checked={form.hidden}
            onChange={(e) => set({ hidden: e.target.checked })}
          />
          <span>{t('Text erst nach einem Klick zeigen')}</span>
        </label>
      )}
      <div className="editor-row">
        <label className="field">
          <span>{t('Löschen nach')}</span>
          <select
            value={form.deletionDays}
            onChange={(e) => set({ deletionDays: Number(e.target.value) })}
          >
            {SEND_DAYS.map((n) => (
              <option key={n} value={n}>
                {days(n)}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t('Läuft ab nach')}</span>
          <select
            value={form.expiresDays}
            onChange={(e) => set({ expiresDays: Number(e.target.value) })}
          >
            <option value={0}>{t('Nie (bis zum Löschen)')}</option>
            {SEND_DAYS.map((n) => (
              <option key={n} value={n}>
                {days(n)}
              </option>
            ))}
          </select>
        </label>
      </div>
      {send && (
        <small className="field-hint">
          {t('Beide zählen ab jetzt: Speichern setzt die Daten neu.')}
        </small>
      )}
      <label className="field">
        <span>{t('Höchstens so oft öffnen')}</span>
        <input
          type="number"
          min={1}
          value={form.maxAccess}
          placeholder={t('unbegrenzt')}
          onChange={(e) => set({ maxAccess: e.target.value.replace(/\D/g, '') })}
        />
      </label>
      {domains.length > 0 && (
        <label className="field">
          <span>{t('Link-Adresse')}</span>
          <select
            value={domain ?? ''}
            disabled={domain === undefined}
            onChange={(e) => setDomain(e.target.value || null)}
          >
            <option value="">{t('Hauptadresse des Servers')}</option>
            {domains.map((d) => (
              <option key={d.id} value={d.id}>
                {d.url.replace(/^https?:\/\//, '')}
              </option>
            ))}
          </select>
          <small className="field-hint">
            {t(
              'Der Send öffnet sich unter jeder dieser Adressen; diese steht im Link, den du kopierst.',
            )}
          </small>
        </label>
      )}
      <fieldset className="field">
        <legend>{t('Wer darf öffnen?')}</legend>
        <div className="flex flex-wrap gap-x-4 gap-y-1">
          {accessOptions.map((option) => (
            <label key={option.value} className="check">
              <input
                type="radio"
                name="send-access"
                checked={form.access === option.value}
                disabled={option.value === 0 && !mailOk}
                onChange={() => set({ access: option.value })}
              />
              <span>{option.label}</span>
            </label>
          ))}
        </div>
        {!mailOk && (
          <small className="field-hint">
            {t('Nur bestimmte Adressen braucht Mail auf dem Server; hier ist keine eingerichtet.')}
          </small>
        )}
      </fieldset>
      {form.access === 1 && (
        <label className="field">
          <span>
            {send?.hasPassword ? t('Neues Passwort (leer lässt das alte)') : t('Passwort')}
          </span>
          <PasswordInput
            value={form.password}
            onChange={(password) => set({ password })}
            autoComplete="new-password"
          />
        </label>
      )}
      {form.access === 0 && (
        <label className="field">
          <span>{t('E-Mail-Adressen')}</span>
          <textarea
            rows={2}
            value={form.emails}
            placeholder="friend@example.com, family@example.org"
            onChange={(e) => set({ emails: e.target.value })}
          />
          <small className="field-hint">
            {splitAddresses(form.emails).length > 0 &&
              `${t('{n} Adressen', { n: splitAddresses(form.emails).length })} · `}
            {t(
              'Wer den Link öffnet, gibt seine Adresse an und bekommt einen Code per Mail. Der Server kennt dafür die Adressen.',
            )}
          </small>
        </label>
      )}
      <label className="field">
        <span>{t('Notizen (nur für dich)')}</span>
        <textarea
          rows={2}
          value={form.notes}
          maxLength={1000}
          onChange={(e) => set({ notes: e.target.value })}
        />
      </label>
      <label className="check">
        <input
          type="checkbox"
          checked={form.hideEmail}
          onChange={(e) => set({ hideEmail: e.target.checked })}
        />
        <span>{t('Meine Adresse nicht zeigen')}</span>
      </label>
      <label className="check">
        <input
          type="checkbox"
          role="switch"
          checked={form.disabled}
          onChange={(e) => set({ disabled: e.target.checked })}
        />
        <span>{t('Deaktiviert: der Link öffnet vorerst nichts')}</span>
      </label>
      <div className="form-actions">
        <Button variant="ghost" onClick={onCancel}>
          {t('Abbrechen')}
        </Button>
        <span className="spacer" />
        <Button type="submit" variant="primary" disabled={!ready || busy}>
          {busy ? t('Speichert …') : send ? t('Speichern') : t('Anlegen')}
        </Button>
      </div>
    </form>
  );
}
