/**
 * Sends on a phone or iPad: the list (pull to reload, "+" for a new one), a
 * Send's own page (link to copy or share, how long it lives, who may open
 * it) and the editor as the platform's edit surface. The logic is the
 * desktop's `SendsDialog`; Rust seals and opens the Sends.
 *
 * Also the small pieces the other extras pages share: a list loaded once for
 * both columns of an iPad, copying and sharing a link, file sizes.
 */

import { listen } from '@tauri-apps/api/event';
import { Fab, haptic, ICONS, ListRow, ListSection, NavButton, Stepper } from '@uwusuite/design';
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { copyGenerated } from '../../lib/api';
import { errorText, toastError } from '../../lib/errors';
import { when } from '../../lib/format';
import { t, useLanguage } from '../../lib/i18n';
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
  sendFileLimit,
} from '../../lib/sendModel';
import {
  deleteSend,
  removeSendAuth,
  saveSend,
  sends,
  sendsAvailable,
  stageSendFile,
} from '../../lib/sends';
import { getSettings } from '../../lib/settings';
import { note } from '../../lib/toast';
import { has, sendOptions, useUwu, type SendOptions } from '../../lib/uwu';
import { sendStatusText } from '../../components/SendsDialog';
import { useMobile, useNav } from '../state';
import {
  ChoiceSheet,
  Chip,
  Empty,
  EditSurface,
  FieldInput,
  GlyphTile,
  Hero,
  LinkBox,
  Page,
  Segmented,
  StateChip,
  Toggle,
  useConfirm,
} from '../ui';

// ── Shared by the extras pages ───────────────────────────────────────────────

export type Loaded<T> = { value: T | null; error: string | null };

/**
 * A list the list page and the detail page beside it (iPad) both read: loaded
 * once, reloaded after a change, forgotten when the account changes.
 */
export function listStore<T>(fetch: () => Promise<T>) {
  let state: Loaded<T> = { value: null, error: null };
  let owner: string | null | undefined;
  let pending: Promise<void> | null = null;
  let generation = 0;
  const subscribers = new Set<() => void>();
  const set = (next: Loaded<T>) => {
    state = next;
    subscribers.forEach((notify) => notify());
  };
  const load = (): Promise<void> => {
    if (pending) return pending;
    const mine = generation;
    pending = fetch()
      .then(
        (value) => {
          if (mine === generation) set({ value, error: null });
        },
        (e) => {
          if (mine === generation) set({ value: state.value, error: errorText(e) });
        },
      )
      .finally(() => {
        pending = null;
      });
    return pending;
  };
  return {
    load,
    get: () => state,
    subscribe: (notify: () => void) => {
      subscribers.add(notify);
      return () => void subscribers.delete(notify);
    },
    /** Another account's list must not show for a moment. */
    own(account: string | null) {
      if (owner === account) return;
      const first = owner === undefined;
      owner = account;
      if (first) return;
      generation += 1;
      pending = null;
      set({ value: null, error: null });
    },
  };
}

export type ListStore<T> = ReturnType<typeof listStore<T>>;

/** Reads `store`, loading it when a page shows it (and after a sync, with `events`). */
export function useListStore<T>(store: ListStore<T>, events = false): Loaded<T> {
  const { status } = useMobile();
  const state = useSyncExternalStore(store.subscribe, store.get);
  useEffect(() => {
    store.own(status.accountId);
    void store.load();
    if (!events) return;
    // A sync (here or from another device) brings changes.
    const stop = listen('vault-changed', () => void store.load());
    return () => void stop.then((unlisten) => unlisten());
  }, [store, status.accountId, events]);
  return state;
}

/**
 * Copies a link through Rust, which clears the clipboard later — as the
 * desktop does. The link opens the Send (its key is in it), so there is no
 * fallback to the browser's clipboard, which nobody clears.
 */
export async function copyLink(link: string): Promise<void> {
  try {
    await copyGenerated(link);
  } catch (e) {
    toastError(e);
    return;
  }
  haptic('success');
  const seconds = getSettings().clipboardClear;
  note(t('Link kopiert ✧'), {
    tone: 'success',
    detail: seconds > 0 ? t('Wird nach {n} s geleert', { n: seconds }) : undefined,
  });
}

/** The system's share sheet; where there is none, the link is copied. */
export async function shareLink(link: string): Promise<void> {
  if (typeof navigator.share === 'function') {
    try {
      await navigator.share({ url: link });
    } catch (e) {
      // Closing the share sheet is no error.
      if ((e as Error)?.name !== 'AbortError') await copyLink(link);
    }
    return;
  }
  await copyLink(link);
}

export function fileSize(bytes: number | null | undefined): string {
  if (!bytes) return '0 B';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** The editor's state: which one, and whether it is open (it stays mounted while it closes). */
export type Editing<T> = { n: number; target: T; open: boolean } | null;

export function useEditing<T>() {
  const [editing, setEditing] = useState<Editing<T>>(null);
  return {
    editing,
    open: (target: T) =>
      setEditing((current) => ({ n: (current?.n ?? 0) + 1, target, open: true })),
    close: () => setEditing((current) => current && { ...current, open: false }),
  };
}

// ── Sends ────────────────────────────────────────────────────────────────────

export const sendStore = listStore(async () => sortSends(await sends()));

/** The state chip: on, switched off, or no longer opening. */
function sendState(send: Send): 'on' | 'off' | 'gone' {
  const status = sendStatus(send);
  return status.kind === 'active' ? 'on' : status.kind === 'disabled' ? 'off' : 'gone';
}

function openedText(send: Send): string {
  return send.maxAccessCount
    ? t('{n} von {max} Mal geöffnet', { n: send.accessCount, max: send.maxAccessCount })
    : t('{n} Mal geöffnet', { n: send.accessCount });
}

const kindIcon = (kind: SendKind) => (kind === 1 ? ICONS.file : ICONS.note);

const days = (n: number) => (n === 1 ? t('1 Tag') : t('{n} Tage', { n }));

function SendsUnavailable({ title }: { title: string }) {
  useLanguage();
  return (
    <Page title={title}>
      <Empty title={t('Keine Sends')}>{t('Dieser Server bietet keine Sends an.')}</Empty>
    </Page>
  );
}

export function SendsPage() {
  useLanguage();
  const { ios, ipad, android } = useMobile();
  const nav = useNav();
  const uwu = useUwu();
  const { value: list, error } = useListStore(sendStore, true);
  const editor = useEditing<null>();

  if (!sendsAvailable(uwu)) return <SendsUnavailable title={t('Sends')} />;

  const add = () => editor.open(null);
  return (
    <>
      <Page
        title={t('Sends')}
        largeTitle
        subtitle={t('Texte und Dateien teilen, mit Ablaufdatum')}
        onRefresh={() => sendStore.load()}
        trailing={
          (ios || ipad) && <NavButton label={t('Neuer Send')} icon={ICONS.add} onClick={add} />
        }
      >
        {error && <p className="m-error">{error}</p>}
        {list !== null && (
          <ListSection
            footer={t(
              'Wer den Link hat, kann den Send öffnen. Abgelaufene Sends löscht der Server nach der Löschfrist.',
            )}
          >
            {list.length === 0 ? (
              <Empty title={t('Noch keine Sends')}>{t('Tipp auf + für einen neuen.')}</Empty>
            ) : (
              list.map((send) => (
                <ListRow
                  key={send.id}
                  icon={<GlyphTile icon={kindIcon(send.kind)} />}
                  iconTone="none"
                  title={
                    <span className="m-row-title">
                      <span>{send.name || t('(ohne Namen)')}</span>
                      {send.authType === 1 && (
                        <ICONS.locked role="img" aria-label={t('Mit Passwort')} />
                      )}
                      {send.authType === 0 && (
                        <ICONS.account role="img" aria-label={t('Nur bestimmte Adressen')} />
                      )}
                    </span>
                  }
                  subtitle={[sendStatusText(send), openedText(send)].join(' · ')}
                  trailing={<StateChip state={sendState(send)} />}
                  chevron={!android}
                  selected={
                    nav.column !== 'phone' &&
                    nav.selected?.page === 'send' &&
                    nav.selected.id === send.id
                  }
                  onClick={() => nav.open({ page: 'send', id: send.id })}
                />
              ))
            )}
          </ListSection>
        )}
      </Page>
      {android && <Fab label={t('Neuer Send')} icon={ICONS.add} onClick={add} />}
      {editor.editing && (
        <SendEditor
          key={editor.editing.n}
          open={editor.editing.open}
          send={null}
          onClose={editor.close}
          onSaved={(id) => {
            editor.close();
            nav.open({ page: 'send', id });
          }}
        />
      )}
    </>
  );
}

export function SendPage({ id }: { id: string }) {
  useLanguage();
  const { android } = useMobile();
  const nav = useNav();
  const uwu = useUwu();
  const { value: list } = useListStore(sendStore, true);
  const editor = useEditing<Send>();
  const confirm = useConfirm();
  const [busy, setBusy] = useState(false);
  const send = list?.find((s) => s.id === id) ?? null;

  if (!sendsAvailable(uwu)) return <SendsUnavailable title={t('Send')} />;
  if (!send) return <Page title="">{list !== null && <Empty title={t('Gelöscht')} />}</Page>;

  const act = async (what: () => Promise<unknown>, done: string, after?: () => void) => {
    setBusy(true);
    try {
      await what();
      haptic('success');
      note(done, { tone: 'success' });
      await sendStore.load();
      after?.();
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  };

  const name = send.name || t('(ohne Namen)');
  const status = sendStatus(send);
  const access =
    send.authType === 1
      ? t('Mit Passwort')
      : send.authType === 0
        ? t('Nur bestimmte Adressen')
        : t('Jeder mit dem Link');

  return (
    <>
      <Page
        hero
        title={name}
        trailing={
          android ? (
            <NavButton
              label={t('Bearbeiten')}
              icon={ICONS.edit}
              onClick={() => editor.open(send)}
            />
          ) : (
            <NavButton label={t('Bearbeiten')} text onClick={() => editor.open(send)} />
          )
        }
      >
        <Hero tile={<GlyphTile icon={kindIcon(send.kind)} size="large" />} title={name}>
          <StateChip state={sendState(send)} />
          <Chip icon={ICONS.show}>{openedText(send)}</Chip>
        </Hero>

        <ListSection header={t('Link')}>
          <LinkBox
            url={send.link}
            onCopy={() => void copyLink(send.link)}
            onShare={() => void shareLink(send.link)}
          />
        </ListSection>

        {send.kind === 0 ? (
          <ListSection header={send.entry ? t('Text · als Eintrag geteilt') : t('Text')}>
            <div className="m-note">{send.text ?? ''}</div>
          </ListSection>
        ) : (
          <ListSection header={t('Datei')}>
            <ListRow
              icon={ICONS.file}
              iconTone="neutral"
              title={send.fileName ?? ''}
              value={send.size !== null ? fileSize(send.size) : undefined}
            />
          </ListSection>
        )}

        {send.notes && (
          <ListSection header={t('Notizen (nur für dich)')}>
            <div className="m-note">{send.notes}</div>
          </ListSection>
        )}

        <ListSection header={t('Gültigkeit')}>
          <ListRow
            title={t('Läuft ab')}
            value={
              status.kind === 'expired'
                ? t('abgelaufen')
                : (when(send.expirationDate) ?? t('Nie (bis zum Löschen)'))
            }
          />
          <ListRow title={t('Wird gelöscht')} value={when(send.deletionDate) ?? '–'} />
          <ListRow
            title={t('Höchstens so oft öffnen')}
            value={send.maxAccessCount ? String(send.maxAccessCount) : t('unbegrenzt')}
          />
        </ListSection>

        <ListSection
          footer={
            send.authType === 0
              ? t('Nur für: {emails}', { emails: send.emails.join(', ') })
              : undefined
          }
        >
          <ListRow title={t('Wer darf öffnen?')} value={access} />
          {send.authType !== 2 && (
            <ListRow
              tone="accent"
              title={send.authType === 0 ? t('Adressen entfernen') : t('Passwort entfernen')}
              disabled={busy}
              onClick={() =>
                void act(
                  () => removeSendAuth(send.id),
                  send.authType === 0 ? t('Adressen entfernt.') : t('Passwort entfernt.'),
                )
              }
            />
          )}
          <ListRow
            title={t('Meine Adresse nicht zeigen')}
            value={send.hideEmail ? t('Ja') : t('Nein')}
          />
          <ListRow title={t('Deaktiviert')} value={send.disabled ? t('Ja') : t('Nein')} />
        </ListSection>

        <ListSection>
          <ListRow
            tone="danger"
            title={t('Send löschen')}
            disabled={busy}
            onClick={() =>
              confirm.ask({
                title: t('Send löschen?'),
                text: t('Der Link öffnet danach nichts mehr.'),
                confirm: t('Löschen'),
                run: () => void act(() => deleteSend(send.id), t('Gelöscht.'), nav.back),
              })
            }
          />
        </ListSection>
      </Page>
      {confirm.element}
      {editor.editing && (
        <SendEditor
          key={editor.editing.n}
          open={editor.editing.open}
          send={editor.editing.target}
          onClose={editor.close}
          onSaved={() => editor.close()}
        />
      )}
    </>
  );
}

// ── The editor ───────────────────────────────────────────────────────────────

type Choosing = 'deletion' | 'expires' | 'access' | 'domain' | null;

function SendEditor({
  open,
  send,
  onClose,
  onSaved,
}: {
  open: boolean;
  send: Send | null;
  onClose: () => void;
  onSaved: (id: string) => void;
}) {
  useLanguage();
  const uwu = useUwu();
  const [kind, setKind] = useState<SendKind>(send?.kind ?? 0);
  const [form, setForm] = useState<SendForm>(() => sendForm(send));
  const [file, setFile] = useState<File | null>(null);
  const [options, setOptions] = useState<SendOptions | null>(null);
  // Where the link points; `undefined` until the account's default is known.
  const [domain, setDomain] = useState<string | null | undefined>(
    send ? send.sendDomainId : undefined,
  );
  const [choosing, setChoosing] = useState<Choosing>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const set = (patch: Partial<SendForm>) => {
    setDirty(true);
    setForm((current) => ({ ...current, ...patch }));
  };
  const maxBytes = sendFileLimit(uwu.limits?.maxFileBytes);

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
  const addresses = splitAddresses(form.emails);

  // Two taps in one frame would make two Sends: `busy` is only seen after a render.
  const saving = useRef(false);
  const save = async () => {
    if (!ready || saving.current) return;
    saving.current = true;
    setBusy(true);
    setError(null);
    try {
      if (kind === 1 && !send) {
        if (!file) return;
        await stageSendFile(file);
      }
      const draft = sendDraft(form, send, kind);
      const id = await saveSend(send?.id ?? null, draft, domains.length ? domain : undefined);
      await sendStore.load();
      // A new Send's link is what the person wants next.
      const link = send ? null : (sendStore.get().value?.find((s) => s.id === id)?.link ?? null);
      const copied = link
        ? await copyGenerated(link).then(
            () => true,
            () => false,
          )
        : false;
      haptic('success');
      note(
        send
          ? t('Gespeichert ✧')
          : copied
            ? t('Send angelegt – der Link ist kopiert ✧')
            : t('Send angelegt ✧'),
        { tone: 'success' },
      );
      onSaved(id);
    } catch (e) {
      setError(errorText(e));
    } finally {
      saving.current = false;
      setBusy(false);
    }
  };

  const accessOptions: { value: SendAccess; label: string }[] = [
    { value: 2, label: t('Jeder mit dem Link') },
    { value: 1, label: t('Mit Passwort') },
    ...(mailOk ? [{ value: 0 as SendAccess, label: t('Nur bestimmte Adressen') }] : []),
  ];
  const deletionOptions = SEND_DAYS.map((n) => ({ value: n, label: days(n) }));
  const expiresOptions = [{ value: 0, label: t('Nie (bis zum Löschen)') }, ...deletionOptions];
  const domainOptions = [
    { value: '', label: t('Hauptadresse des Servers') },
    ...domains.map((d) => ({ value: d.id, label: d.url.replace(/^https?:\/\//, '') })),
  ];
  const maxAccess = Number(form.maxAccess) || 0;

  return (
    <EditSurface
      open={open}
      onClose={onClose}
      title={send ? t('Send bearbeiten') : t('Neuer Send')}
      dirty={dirty}
      action={{
        label: busy ? t('Speichert …') : send ? t('Sichern') : t('Anlegen'),
        onClick: () => void save(),
        disabled: !ready || busy,
      }}
    >
      {!send && (
        <Segmented
          label={t('Art des Sends')}
          value={kind === 1 ? 'file' : 'text'}
          onChange={(value) => setKind(value === 'file' ? 1 : 0)}
          options={[
            { value: 'text', label: t('Text') },
            { value: 'file', label: t('Datei') },
          ]}
        />
      )}
      {error && (
        <p className="m-error" role="alert">
          {error}
        </p>
      )}

      <ListSection>
        <FieldInput
          label={t('Name des Sends')}
          value={form.name}
          maxLength={200}
          placeholder={t('z. B. WLAN für Gäste')}
          onChange={(name) => set({ name })}
        />
      </ListSection>

      {kind === 0 && send?.entry ? (
        <ListSection
          header={t('Text · als Eintrag geteilt')}
          footer={t(
            'Ein geteilter Eintrag lässt sich nicht ändern: Sein Inhalt steckt noch ein zweites Mal im Send, für die Send-Seite. Um etwas zurückzunehmen, lösch diesen Send und teil den Eintrag neu.',
          )}
        >
          <div className="m-note">{send.text ?? ''}</div>
        </ListSection>
      ) : kind === 0 ? (
        <ListSection>
          <FieldInput
            label={t('Text')}
            value={form.text}
            multiline
            rows={4}
            maxLength={1000}
            placeholder={t('Was geteilt wird')}
            onChange={(text) => set({ text })}
          />
          <ListRow
            title={t('Text erst nach Tipp zeigen')}
            trailing={
              <Toggle
                label={t('Text erst nach Tipp zeigen')}
                checked={form.hidden}
                onChange={(hidden) => set({ hidden })}
              />
            }
          />
        </ListSection>
      ) : send ? (
        <ListSection
          footer={t('Die Datei eines Sends bleibt, wie sie ist: {name}', {
            name: send.fileName ?? '',
          })}
        >
          <ListRow icon={ICONS.file} iconTone="neutral" title={send.fileName ?? ''} />
        </ListSection>
      ) : (
        <ListSection>
          <input
            ref={input}
            type="file"
            hidden
            onChange={(event) => {
              const picked = event.target.files?.[0] ?? null;
              event.target.value = '';
              if (picked && maxBytes && picked.size > maxBytes) {
                setError(
                  t('Die Datei ist zu groß: höchstens {size}.', { size: fileSize(maxBytes) }),
                );
                return;
              }
              setError(null);
              setFile(picked);
              set({ fileName: picked?.name ?? null });
            }}
          />
          <ListRow
            icon={ICONS.file}
            iconTone="neutral"
            title={file ? file.name : t('Datei wählen …')}
            value={file ? fileSize(file.size) : undefined}
            onClick={() => input.current?.click()}
          />
        </ListSection>
      )}

      <ListSection
        header={t('Gültigkeit')}
        footer={send ? t('Beide zählen ab jetzt: Speichern setzt die Daten neu.') : undefined}
      >
        <ListRow
          icon={ICONS.delete}
          iconTone="neutral"
          title={t('Löschen nach')}
          value={days(form.deletionDays)}
          onClick={() => setChoosing('deletion')}
        />
        <ListRow
          icon={ICONS.reminder}
          iconTone="neutral"
          title={t('Läuft ab nach')}
          value={form.expiresDays ? days(form.expiresDays) : t('Nie (bis zum Löschen)')}
          onClick={() => setChoosing('expires')}
        />
        <ListRow
          icon={ICONS.show}
          iconTone="neutral"
          title={t('Höchstens so oft öffnen')}
          value={maxAccess ? String(maxAccess) : t('unbegrenzt')}
          trailing={
            <Stepper
              label={t('Höchstens so oft öffnen')}
              value={maxAccess}
              min={0}
              max={1000}
              onChange={(n) => set({ maxAccess: n ? String(n) : '' })}
            />
          }
        />
      </ListSection>

      <ListSection
        header={t('Wer darf öffnen?')}
        footer={
          !mailOk
            ? t('Nur bestimmte Adressen braucht Mail auf dem Server; hier ist keine eingerichtet.')
            : form.access === 0
              ? `${addresses.length ? `${t('{n} Adressen', { n: addresses.length })} · ` : ''}${t(
                  'Wer den Link öffnet, gibt seine Adresse an und bekommt einen Code per Mail. Der Server kennt dafür die Adressen.',
                )}`
              : undefined
        }
      >
        <ListRow
          icon={form.access === 2 ? ICONS.link : form.access === 1 ? ICONS.locked : ICONS.account}
          iconTone="neutral"
          title={t('Wer darf öffnen?')}
          value={accessOptions.find((o) => o.value === form.access)?.label}
          onClick={() => setChoosing('access')}
        />
        {form.access === 1 && (
          <FieldInput
            label={send?.hasPassword ? t('Neues Passwort (leer lässt das alte)') : t('Passwort')}
            type="password"
            autoComplete="new-password"
            value={form.password}
            onChange={(password) => set({ password })}
          />
        )}
        {form.access === 0 && (
          <FieldInput
            label={t('E-Mail-Adressen')}
            multiline
            rows={2}
            value={form.emails}
            placeholder="friend@example.com, family@example.org"
            onChange={(emails) => set({ emails })}
          />
        )}
        {domains.length > 0 && (
          <ListRow
            icon={ICONS.website}
            iconTone="neutral"
            title={t('Link-Adresse')}
            value={domainOptions.find((o) => o.value === (domain ?? ''))?.label}
            disabled={domain === undefined}
            onClick={() => setChoosing('domain')}
          />
        )}
      </ListSection>

      <ListSection>
        <ListRow
          title={t('Meine Adresse nicht zeigen')}
          trailing={
            <Toggle
              label={t('Meine Adresse nicht zeigen')}
              checked={form.hideEmail}
              onChange={(hideEmail) => set({ hideEmail })}
            />
          }
        />
        {send && (
          <ListRow
            title={t('Deaktiviert')}
            subtitle={t('Der Link öffnet vorerst nichts')}
            trailing={
              <Toggle
                label={t('Deaktiviert')}
                checked={form.disabled}
                onChange={(disabled) => set({ disabled })}
              />
            }
          />
        )}
      </ListSection>

      <ListSection>
        <FieldInput
          label={t('Notizen (nur für dich)')}
          multiline
          rows={2}
          maxLength={1000}
          value={form.notes}
          onChange={(notes) => set({ notes })}
        />
      </ListSection>

      <ChoiceSheet
        open={choosing === 'deletion'}
        onClose={() => setChoosing(null)}
        title={t('Löschen nach')}
        options={deletionOptions}
        value={form.deletionDays}
        onChange={(deletionDays) => set({ deletionDays })}
      />
      <ChoiceSheet
        open={choosing === 'expires'}
        onClose={() => setChoosing(null)}
        title={t('Läuft ab nach')}
        options={expiresOptions}
        value={form.expiresDays}
        onChange={(expiresDays) => set({ expiresDays })}
      />
      <ChoiceSheet
        open={choosing === 'access'}
        onClose={() => setChoosing(null)}
        title={t('Wer darf öffnen?')}
        options={accessOptions}
        value={form.access}
        onChange={(access) => set({ access })}
      />
      <ChoiceSheet
        open={choosing === 'domain'}
        onClose={() => setChoosing(null)}
        title={t('Link-Adresse')}
        options={domainOptions}
        value={domain ?? ''}
        onChange={(value) => {
          setDirty(true);
          setDomain(value || null);
        }}
        footer={t(
          'Der Send öffnet sich unter jeder dieser Adressen; diese steht im Link, den du kopierst.',
        )}
      />
    </EditSurface>
  );
}
