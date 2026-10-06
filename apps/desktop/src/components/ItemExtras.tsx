/**
 * An item's part of UwULock Server's extras, shown in its details: its own
 * icon, the password renewal reminder, the entry versions — and sharing it as
 * a Send, which works on Bitwarden and Vaultwarden too.
 */

import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Button, Hint, Icon, IconButton, ICONS, Segmented } from '@uwusuite/design';
import { copyGenerated, type ItemDetail as Detail, type ItemSummary } from '../lib/api';
import { errorText, toastError } from '../lib/errors';
import { when } from '../lib/format';
import { ICON_ACCEPT, iconFromFile } from '../lib/iconImage';
import { N_, locale, t, useLanguage } from '../lib/i18n';
import { IDENTITY_LABEL } from '../lib/items';
import { getSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import {
  deleteOwnIcon,
  deleteReminder,
  deleteVersions,
  fetchDeviceIcon,
  has,
  isLocalHost,
  itemVersions,
  restoreVersion,
  revealVersionField,
  sendOptions,
  setOwnIcon,
  setReminder,
  shareAsSend,
  useUwu,
  type Change,
  type SendOptions,
  type Version,
} from '../lib/uwu';
import { ContextMenu } from './ContextMenu';
import { Modal } from './Modal';
import { LibraryDialog } from './OwnIconPicker';
import { NyuBusy, playNyu } from './nyu/stage';

// ── Names of values ────────────────────────────────────────

const FIXED_LABEL: Record<string, string> = {
  name: N_('Name'),
  username: N_('Benutzername'),
  password: N_('Passwort'),
  totp: N_('Einmal-Code (TOTP)'),
  notes: N_('Notizen'),
  'card-name': N_('Karteninhaber'),
  'card-brand': N_('Marke'),
  'card-number': N_('Kartennummer'),
  'card-expiry': N_('Gültig bis'),
  'card-code': N_('Prüfnummer'),
  'ssh-private': N_('Privater Schlüssel'),
  'ssh-public': N_('Öffentlicher Schlüssel'),
  'ssh-fingerprint': N_('Fingerprint'),
};

/** What a value is called, in the person's language. */
export function valueLabel(field: string, own?: string | null): string {
  if (own) return own;
  const fixed = FIXED_LABEL[field];
  if (fixed) return t(fixed);
  const [prefix, rest = ''] = field.split(':');
  if (prefix === 'uri') return t('Website {n}', { n: Number(rest) + 1 });
  if (prefix === 'field') return t('Feld {n}', { n: Number(rest) + 1 });
  if (prefix === 'identity') return t(IDENTITY_LABEL[rest] ?? rest);
  return field;
}

function Card({ title, children }: { title?: ReactNode; children: ReactNode }) {
  return (
    <section className="detail-card">
      {title && <h3 className="detail-card-title">{title}</h3>}
      {children}
    </section>
  );
}

// ── Own icon ───────────────────────────────────────────────

/**
 * The button on an item's tile: pick a picture, an icon from the server's
 * library, take the icon from a device on the local network, or remove the
 * own icon. The item editor offers the same (`OwnIconEditor`).
 */
export function IconMenu({ summary, detail }: { summary: ItemSummary; detail: Detail | null }) {
  useLanguage();
  const uwu = useUwu();
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [library, setLibrary] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  if (!has(uwu, 'own-icons') || summary.deleted) return null;
  const hasOwn = summary.id in uwu.ownIcons;
  const local = detail?.login?.uris.some((uri) => isLocalHost(uri.host)) ?? false;

  const run = async (what: () => Promise<void>, done: string) => {
    setBusy(true);
    try {
      await what();
      toast(done);
    } catch (e) {
      toast(e instanceof Error ? t('Das Bild ließ sich nicht lesen.') : errorText(e), 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <IconButton
        icon={ICONS.image}
        label={t('Symbol ändern')}
        size="sm"
        className="tile-edit size-6! bg-surface hover:bg-elevated"
        disabled={busy}
        aria-haspopup="menu"
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu({ x: rect.left, y: rect.bottom + 4 });
        }}
      />
      <input
        ref={input}
        type="file"
        accept={ICON_ACCEPT}
        hidden
        onChange={(event) => {
          const file = event.target.files?.[0];
          event.target.value = '';
          if (!file) return;
          void run(
            async () => setOwnIcon(summary.id, await iconFromFile(file)),
            t('Symbol gespeichert ✧'),
          );
        }}
      />
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label={t('Symbol')}
          onClose={() => setMenu(null)}
          items={[
            {
              label: t('Bild wählen …'),
              icon: ICONS.upload,
              onSelect: () => input.current?.click(),
            },
            ...(has(uwu, 'icon-library')
              ? [
                  {
                    label: t('Aus der Bibliothek …'),
                    icon: ICONS.search,
                    onSelect: () => setLibrary(true),
                  },
                ]
              : []),
            ...(local
              ? [
                  {
                    label: t('Symbol vom Gerät holen'),
                    icon: ICONS.network,
                    onSelect: () =>
                      void run(() => fetchDeviceIcon(summary.id), t('Symbol vom Gerät geholt ✧')),
                  },
                ]
              : []),
            ...(hasOwn
              ? [
                  {
                    label: t('Eigenes Symbol entfernen'),
                    icon: ICONS.delete,
                    danger: true,
                    onSelect: () =>
                      void run(() => deleteOwnIcon(summary.id), t('Eigenes Symbol entfernt.')),
                  },
                ]
              : []),
          ]}
        />
      )}
      {library && (
        <LibraryDialog
          initial={summary.name}
          onCancel={() => setLibrary(false)}
          onPick={(png) => {
            setLibrary(false);
            void run(() => setOwnIcon(summary.id, png), t('Symbol gespeichert ✧'));
          }}
        />
      )}
    </>
  );
}

// ── Reminder ───────────────────────────────────────────────

const MONTHS = [1, 3, 6, 12, 24];

function day(iso: string | null): string | null {
  if (!iso) return null;
  const date = new Date(`${iso}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleDateString(locale(), { dateStyle: 'medium', timeZone: 'UTC' });
}

/** The two ways to say when: every so many months, or on a date. */
const whenOptions = (): { value: 'months' | 'date'; label: string }[] => [
  { value: 'months', label: t('Regelmäßig') },
  { value: 'date', label: t('An einem Datum') },
];

/** The reminder as the item editor holds it: switched on, and when. */
export type ReminderDraft = { on: boolean; mode: 'months' | 'date'; months: number; date: string };

type Reminder = { due: string | null; everyMonths: number | null };

export function reminderDraft(reminder: Reminder | undefined): ReminderDraft {
  return {
    on: Boolean(reminder),
    mode: reminder?.everyMonths || !reminder?.due ? 'months' : 'date',
    months: reminder?.everyMonths ?? 6,
    date: reminder?.due ?? '',
  };
}

/**
 * Saves what the editor's switch says for item `id`, if it differs from
 * `before`: on (every so many months, or on a date) or off.
 */
export async function saveReminder(
  id: string,
  draft: ReminderDraft,
  before: Reminder | undefined,
): Promise<void> {
  if (!draft.on) {
    if (before) await deleteReminder(id);
    return;
  }
  const due = draft.mode === 'date' ? draft.date || null : null;
  const every = draft.mode === 'months' ? draft.months : null;
  if (draft.mode === 'date' && !due) return;
  if (before) {
    // Every so many months counts from the password's last change: the server works out the date.
    const same =
      every !== null
        ? before.everyMonths === every
        : before.everyMonths === null && before.due === due;
    if (same) return;
  }
  await setReminder(id, due, every);
}

/** The item editor's "remind me to renew" switch and its settings. */
export function ReminderEditor({
  value,
  onChange,
}: {
  value: ReminderDraft;
  onChange: (next: ReminderDraft) => void;
}) {
  useLanguage();
  const set = (patch: Partial<ReminderDraft>) => onChange({ ...value, ...patch });
  return (
    <div className="editor-reminder">
      <label className="check">
        <input
          type="checkbox"
          role="switch"
          checked={value.on}
          onChange={(e) => set({ on: e.target.checked })}
        />
        <span className="inline-flex items-center gap-2">
          <Icon icon={ICONS.reminder} size="xs" />
          {t('Ans Erneuern des Passworts erinnern')}
        </span>
      </label>
      {value.on && (
        <div className="extras-form">
          <Segmented
            className="justify-self-start"
            label={t('Wann')}
            value={value.mode}
            onChange={(mode) => set({ mode })}
            options={whenOptions()}
          />
          {value.mode === 'months' ? (
            <label className="field">
              <span>{t('Alle')}</span>
              <select
                value={value.months}
                onChange={(e) => set({ months: Number(e.target.value) })}
              >
                {MONTHS.map((n) => (
                  <option key={n} value={n}>
                    {n === 1 ? t('1 Monat') : t('{n} Monate', { n })}
                  </option>
                ))}
              </select>
              <small className="field-hint">
                {t(
                  'Gezählt ab der letzten Passwortänderung; ändert sich das Passwort, beginnt es neu.',
                )}
              </small>
            </label>
          ) : (
            <label className="field">
              <span>{t('Datum')}</span>
              <input
                type="date"
                value={value.date}
                onChange={(e) => set({ date: e.target.value })}
              />
            </label>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * "Remind me to renew this password": every so many months, or on a date.
 * Only once it is switched on in the editor; here it can be changed or removed.
 */
export function ReminderCard({ summary }: { summary: ItemSummary }) {
  useLanguage();
  const uwu = useUwu();
  const reminder = uwu.reminders[summary.id];
  const [editing, setEditing] = useState(false);
  const [mode, setMode] = useState<'months' | 'date'>('months');
  const [months, setMonths] = useState(6);
  const [date, setDate] = useState('');
  const [busy, setBusy] = useState(false);
  if (!has(uwu, 'reminders') || summary.kind !== 'login' || summary.deleted || !reminder)
    return null;

  const run = async (what: () => Promise<void>, done: string) => {
    setBusy(true);
    try {
      await what();
      toast(done);
      setEditing(false);
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  };

  const open = () => {
    setMode(reminder?.everyMonths || !reminder?.due ? 'months' : 'date');
    setMonths(reminder?.everyMonths ?? 6);
    setDate(reminder?.due ?? '');
    setEditing(true);
  };

  const describe = () => {
    const every = reminder.everyMonths
      ? reminder.everyMonths === 1
        ? t('jeden Monat')
        : t('alle {n} Monate', { n: reminder.everyMonths })
      : null;
    const due = day(reminder.due);
    return [every, due && t('fällig am {date}', { date: due })].filter(Boolean).join(' · ');
  };

  return (
    <Card
      title={
        <>
          <Icon icon={ICONS.reminder} size="xs" />
          {t('Passwort erneuern')}
        </>
      }
    >
      {!editing ? (
        <div className="detail-row">
          <div className="detail-text">
            <span className="detail-value">
              {reminder?.isDue && <span className="due-mark">{t('Jetzt fällig')}</span>}
              {describe()}
            </span>
          </div>
          <div className="detail-actions">
            <Button variant="ghost" size="sm" disabled={busy} onClick={open}>
              {t('Ändern')}
            </Button>
            <IconButton
              icon={ICONS.delete}
              label={t('Erinnerung entfernen')}
              size="sm"
              disabled={busy}
              onClick={() => void run(() => deleteReminder(summary.id), t('Erinnerung entfernt.'))}
            />
          </div>
        </div>
      ) : (
        <div className="extras-form">
          <Segmented
            className="justify-self-start"
            label={t('Wann')}
            value={mode}
            onChange={setMode}
            options={whenOptions()}
          />
          {mode === 'months' ? (
            <label className="field">
              <span>{t('Alle')}</span>
              <select value={months} onChange={(e) => setMonths(Number(e.target.value))}>
                {MONTHS.map((n) => (
                  <option key={n} value={n}>
                    {n === 1 ? t('1 Monat') : t('{n} Monate', { n })}
                  </option>
                ))}
              </select>
              <small className="field-hint">
                {t(
                  'Gezählt ab der letzten Passwortänderung; ändert sich das Passwort, beginnt es neu.',
                )}
              </small>
            </label>
          ) : (
            <label className="field">
              <span>{t('Datum')}</span>
              <input type="date" value={date} onChange={(e) => setDate(e.target.value)} />
            </label>
          )}
          <div className="form-actions">
            <Button variant="ghost" size="sm" onClick={() => setEditing(false)}>
              {t('Abbrechen')}
            </Button>
            <span className="spacer" />
            <Button
              variant="primary"
              size="sm"
              disabled={busy || (mode === 'date' && !date)}
              onClick={() =>
                void run(
                  () =>
                    setReminder(
                      summary.id,
                      mode === 'date' ? date : null,
                      mode === 'months' ? months : null,
                    ),
                  t('Erinnerung gespeichert ✧'),
                )
              }
            >
              {t('Speichern')}
            </Button>
          </div>
        </div>
      )}
    </Card>
  );
}

// ── Versions ───────────────────────────────────────────────

/** A value of the comparison that stays dots until the eye is clicked. */
function Hidden({ load }: { load: () => Promise<string> }) {
  useLanguage();
  const [value, setValue] = useState<string | null>(null);
  useEffect(() => {
    if (value === null) return;
    const timer = window.setTimeout(() => setValue(null), 60_000);
    return () => window.clearTimeout(timer);
  }, [value]);
  return (
    <span className="version-secret">
      <span className="mono">{value ?? '••••••••'}</span>
      <IconButton
        icon={value === null ? ICONS.show : ICONS.hide}
        label={value === null ? t('Zeigen') : t('Verbergen')}
        size="sm"
        aria-pressed={value !== null}
        onClick={() =>
          value !== null
            ? setValue(null)
            : void load()
                .then(setValue)
                .catch((e) => toastError(e))
        }
      />
    </span>
  );
}

function ChangeRow({ id, version, change }: { id: string; version: string; change: Change }) {
  useLanguage();
  const empty = <span className="muted">—</span>;
  const before =
    change.kind === 'added' ? (
      empty
    ) : change.secret ? (
      <Hidden load={() => revealVersionField(id, version, change.field)} />
    ) : (
      <span className="version-value">{change.before}</span>
    );
  const after =
    change.kind === 'removed' ? (
      empty
    ) : change.secret ? (
      <Hidden load={() => revealVersionField(id, null, change.field)} />
    ) : (
      <span className="version-value">{change.after}</span>
    );
  return (
    <div className="version-change">
      <span className="detail-label">{valueLabel(change.field, change.label)}</span>
      <span className="version-then">{before}</span>
      <Icon icon={ICONS.next} size="xs" className="text-faint" />
      <span className="version-now">{after}</span>
    </div>
  );
}

/**
 * The item's earlier states: what each changed compared with now, bringing
 * one back, deleting them. Passwords only after a click.
 */
export function VersionsCard({ summary }: { summary: ItemSummary }) {
  useLanguage();
  const uwu = useUwu();
  const [open, setOpen] = useState(false);
  const [versions, setVersions] = useState<Version[] | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [asking, setAsking] = useState<null | { kind: 'restore' | 'delete'; id: string } | 'all'>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const id = summary.id;

  const load = () =>
    itemVersions(id)
      .then(setVersions)
      .catch((e) => {
        setVersions([]);
        toastError(e);
      });

  // Again when the item changed (a restore makes a new version too).
  useEffect(() => {
    if (open) void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, summary.revisionDate]);

  if (!has(uwu, 'versions') || summary.deleted) return null;

  const act = async (what: () => Promise<void>, done: string) => {
    setBusy(true);
    try {
      await what();
      toast(done);
      await load();
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
      setAsking(null);
    }
  };

  return (
    <Card>
      <Button
        variant="ghost"
        size="sm"
        icon={ICONS.history}
        className="history-toggle"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        {versions && open
          ? versions.length === 1
            ? t('1 frühere Version')
            : t('{n} frühere Versionen', { n: versions.length })
          : t('Frühere Versionen')}
        <Icon
          icon={ICONS.expand}
          size="xs"
          className={open ? 'transition-transform' : '-rotate-90 transition-transform'}
        />
      </Button>
      {open && versions === null && <NyuBusy label={t('Einen Moment …')} />}
      {open && versions?.length === 0 && (
        <p className="detail-empty-line">
          {t('Noch keine. Jede Änderung hebt den Stand davor hier auf.')}
        </p>
      )}
      {open &&
        versions?.map((version) => (
          <div className="version" key={version.id}>
            <button
              type="button"
              className="version-head"
              aria-expanded={expanded === version.id}
              onClick={() => setExpanded(expanded === version.id ? null : version.id)}
            >
              <span className="version-when">
                {when(version.revisionDate) ?? t('Früher')}
                <span className="muted">
                  {' · '}
                  {version.changes.length === 0
                    ? t('gleich wie jetzt')
                    : version.changes.length === 1
                      ? t('1 Unterschied')
                      : t('{n} Unterschiede', { n: version.changes.length })}
                </span>
              </span>
              {version.broken && <Icon icon={ICONS.warning} size="xs" className="badge-warning" />}
              <Icon
                icon={ICONS.expand}
                size="xs"
                className={
                  expanded === version.id
                    ? 'text-muted transition-transform'
                    : 'text-muted -rotate-90 transition-transform'
                }
              />
            </button>
            {expanded === version.id && (
              <div className="version-body">
                {version.changes.length > 0 && (
                  <div className="version-legend muted">
                    <span />
                    <span>{t('Damals')}</span>
                    <span />
                    <span>{t('Jetzt')}</span>
                  </div>
                )}
                {version.changes.map((change) => (
                  <ChangeRow key={change.field} id={id} version={version.id} change={change} />
                ))}
                <div className="form-actions">
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={ICONS.delete}
                    className="text-danger-ink! hover:bg-danger-tint!"
                    disabled={busy}
                    onClick={() => setAsking({ kind: 'delete', id: version.id })}
                  >
                    {t('Version löschen')}
                  </Button>
                  <span className="spacer" />
                  <Button
                    variant="primary"
                    size="sm"
                    icon={ICONS.restore}
                    disabled={busy || version.broken || summary.broken}
                    onClick={() => setAsking({ kind: 'restore', id: version.id })}
                  >
                    {t('Wiederherstellen')}
                  </Button>
                </div>
              </div>
            )}
          </div>
        ))}
      {open && versions && versions.length > 1 && (
        <p className="detail-empty-line">
          <button
            type="button"
            className="link-button"
            disabled={busy}
            onClick={() => setAsking('all')}
          >
            {t('Alle Versionen löschen')}
          </button>
        </p>
      )}

      {asking && (
        <Modal
          title={
            asking === 'all'
              ? t('Alle Versionen löschen?')
              : asking.kind === 'restore'
                ? t('Diese Version wiederherstellen?')
                : t('Version löschen?')
          }
          tone={asking !== 'all' && asking.kind === 'restore' ? 'default' : 'warning'}
          size="small"
          onCancel={() => setAsking(null)}
          footer={
            <>
              <span className="spacer" />
              <Button
                variant={asking !== 'all' && asking.kind === 'restore' ? 'primary' : 'danger'}
                data-secondary
                onClick={() =>
                  asking === 'all'
                    ? void act(() => deleteVersions(id), t('Versionen gelöscht.'))
                    : asking.kind === 'restore'
                      ? void act(
                          () => restoreVersion(id, asking.id),
                          t('Version wiederhergestellt ✧'),
                        )
                      : void act(() => deleteVersions(id, asking.id), t('Version gelöscht.'))
                }
              >
                {asking === 'all'
                  ? t('Alle löschen')
                  : asking.kind === 'restore'
                    ? t('Wiederherstellen')
                    : t('Löschen')}
              </Button>
              <Button data-autofocus onClick={() => setAsking(null)}>
                {t('Abbrechen')}
              </Button>
            </>
          }
        >
          <p className="dialog-lead">
            {asking !== 'all' && asking.kind === 'restore'
              ? t(
                  'Der Eintrag bekommt die Werte dieser Version. Der jetzige Stand bleibt als Version erhalten.',
                )
              : t('Gelöschte Versionen lassen sich nicht zurückholen.')}
          </p>
        </Modal>
      )}
    </Card>
  );
}

// ── Share as Send ──────────────────────────────────────────

type Choice = {
  field: string;
  /** What the recipient reads before the value. */
  label: string;
  /** What the list shows, when that is more than the label (a website's address). */
  shown?: string;
  checked: boolean;
  secret: boolean;
};

/**
 * What an item has that can be shared — no secrets of an item whose
 * organisation hides its passwords from this member. The one-time code only
 * in an entry Send (`entry`): its page shows the live codes, the readable
 * text never holds the key — but the key travels in the Send (its last
 * line), so whoever has the link can take it out. Never ticked by default,
 * and ticking it asks once more.
 */
export function choices(detail: Detail, viewPassword: boolean, entry = false): Choice[] {
  const out: Choice[] = [];
  const add = (field: string, label: string, checked: boolean, secret = false, shown?: string) => {
    if (!viewPassword && secret && !WITHHELD_EXCEPT.test(field)) return;
    out.push({ field, label, checked, secret, ...(shown ? { shown } : {}) });
  };
  if (detail.login) {
    if (detail.login.username) add('username', t('Benutzername'), true);
    if (detail.login.hasPassword) add('password', t('Passwort'), true, true);
    if (entry && detail.login.hasTotp) add('totp', t('Einmal-Code'), false, true);
    const many = detail.login.uris.length > 1;
    detail.login.uris.forEach((uri, index) =>
      add(
        `uri:${index}`,
        many ? t('Website {n}', { n: index + 1 }) : t('Website'),
        index === 0,
        false,
        uri.uri,
      ),
    );
  }
  if (detail.card) {
    if (detail.card.cardholderName) add('card-name', t('Karteninhaber'), true);
    if (detail.card.numberEnding) add('card-number', t('Kartennummer'), true, true);
    if (detail.card.expMonth || detail.card.expYear) add('card-expiry', t('Gültig bis'), true);
    if (detail.card.hasCode) add('card-code', t('Prüfnummer'), false, true);
  }
  for (const entry of detail.identity ?? [])
    add(
      `identity:${entry.name}`,
      t(IDENTITY_LABEL[entry.name] ?? entry.name),
      !entry.sensitive,
      entry.sensitive,
    );
  if (detail.sshKey) {
    if (detail.sshKey.publicKey) add('ssh-public', t('Öffentlicher Schlüssel'), true);
    if (detail.sshKey.fingerprint) add('ssh-fingerprint', t('Fingerprint'), false);
    if (detail.sshKey.hasPrivateKey) add('ssh-private', t('Privater Schlüssel'), false, true);
  }
  for (const field of detail.fields ?? [])
    if (field.kind !== 'linked' && field.hasValue)
      add(
        `field:${field.index}`,
        field.name || t('Feld {n}', { n: field.index + 1 }),
        false,
        field.kind === 'hidden',
      );
  if (detail.notes) add('notes', t('Notizen'), !detail.login && !detail.card);
  return out;
}

/** Secret values that `viewPassword: false` doesn't cover (as `send::withheld`). */
const WITHHELD_EXCEPT = /^identity:/;

const DAYS = [1, 2, 3, 7, 14, 30];

export function ShareSendDialog({
  summary,
  detail,
  onClose,
}: {
  summary: ItemSummary;
  detail: Detail;
  onClose: () => void;
}) {
  useLanguage();
  // UwULock Server's Send page shows an entry Send as the entry, with live codes.
  const entry = useUwu().uwu;
  const [fields, setFields] = useState<Choice[]>(() =>
    choices(detail, summary.viewPassword, entry),
  );
  const [options, setOptions] = useState<SendOptions | null>(null);
  const [days, setDays] = useState(1);
  const [maxAccess, setMaxAccess] = useState('1');
  const [password, setPassword] = useState('');
  const [onlyFor, setOnlyFor] = useState(false);
  const [emails, setEmails] = useState('');
  const [domain, setDomain] = useState<string>('');
  const [hideText, setHideText] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [made, setMade] = useState<{ link: string; deletionDate: string } | null>(null);
  // Ticking the one-time code hands out its key for good: asked for once more.
  const [askTotp, setAskTotp] = useState(false);
  const tick = (name: string, checked: boolean) =>
    setFields((current) => current.map((f) => (f.field === name ? { ...f, checked } : f)));

  useEffect(() => {
    void sendOptions()
      .then((next) => {
        setOptions(next);
        setDomain(next.defaultDomainId ?? '');
      })
      .catch(() => setOptions({ emails: false, domains: [], defaultDomainId: null }));
  }, []);

  const addresses = emails
    .split(/[\s,;]+/)
    .map((e) => e.trim())
    .filter(Boolean);
  const chosen = fields.filter((f) => f.checked);

  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await shareAsSend(summary.id, {
        fields: chosen.map((f) => [f.field, f.label]),
        deletionDays: days,
        maxAccess: maxAccess.trim() ? Math.max(1, Number(maxAccess)) : null,
        password: onlyFor ? null : password || null,
        emails: onlyFor ? addresses : [],
        sendDomainId: domain || null,
        hideText,
        entry,
      });
      setMade(result);
      playNyu('shared');
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const copy = () =>
    void copyGenerated(made?.link ?? '')
      .then(() =>
        toast(
          getSettings().clipboardClear > 0
            ? t('Link kopiert ✧ – wird nach {n} s geleert', { n: getSettings().clipboardClear })
            : t('Link kopiert ✧'),
        ),
      )
      .catch((e) => toastError(e));

  return (
    <Modal
      title={t('Als Send teilen')}
      onCancel={onClose}
      footer={
        made ? (
          <>
            <span className="spacer" />
            <Button onClick={onClose}>{t('Fertig')}</Button>
            <Button variant="primary" icon={ICONS.copy} data-autofocus onClick={copy}>
              {t('Link kopieren')}
            </Button>
          </>
        ) : (
          <>
            <Button variant="ghost" data-secondary onClick={onClose}>
              {t('Abbrechen')}
            </Button>
            <span className="spacer" />
            <Button
              variant="primary"
              icon={ICONS.send}
              disabled={
                busy || !options || chosen.length === 0 || (onlyFor && addresses.length === 0)
              }
              onClick={() => void create()}
            >
              {busy ? t('Erstellt …') : t('Send erstellen')}
            </Button>
          </>
        )
      }
    >
      {made ? (
        <div className="extras-form">
          <p className="dialog-lead">
            {t('Wer diesen Link hat, kann die gewählten Werte ansehen – bis {when}.', {
              when: when(made.deletionDate) ?? '',
            })}
          </p>
          <input className="mono" type="text" readOnly value={made.link} aria-label={t('Link')} />
        </div>
      ) : (
        <div className="extras-form">
          <p className="dialog-lead">
            {entry
              ? t(
                  'Die gewählten Werte gehen verschlüsselt an einen Send. Der Schlüssel steckt im Link, nicht auf dem Server. Die Send-Seite zeigt sie als Eintrag mit Kopier-Knöpfen.',
                )
              : t(
                  'Die gewählten Werte gehen verschlüsselt an einen Send. Der Schlüssel steckt im Link, nicht auf dem Server. Der Einmal-Code-Schlüssel wird nie geteilt.',
                )}
          </p>
          <fieldset className="editor-list">
            <legend>{t('Was geteilt wird')}</legend>
            {fields.length === 0 && (
              <p className="muted">{t('Dieser Eintrag hat nichts zu teilen.')}</p>
            )}
            {fields.map((field) => (
              <label className="check" key={field.field}>
                <input
                  type="checkbox"
                  checked={field.checked}
                  onChange={(e) => {
                    if (field.field === 'totp' && e.target.checked) setAskTotp(true);
                    else tick(field.field, e.target.checked);
                  }}
                />
                <span
                  className={
                    field.shown
                      ? 'inline-flex min-w-0 items-center gap-1.5 uri'
                      : 'inline-flex items-center gap-1.5'
                  }
                >
                  {field.shown ?? field.label}
                  {field.secret && (
                    <Icon icon={ICONS.masterPassword} size="xs" className="text-muted" />
                  )}
                </span>
              </label>
            ))}
            {askTotp && (
              <Hint tone="warning" className="grid gap-2" role="alert">
                <p className="m-0">
                  {t(
                    'Der Schlüssel des Einmal-Codes reist verschlüsselt im Send mit. Die Send-Seite zeigt nur die laufenden Codes – wer den Link hat, kann den Schlüssel aber auslesen und damit auch nach dem Löschen des Sends weiter Codes erzeugen.',
                  )}{' '}
                  {t('Teile ihn nur, wenn das okay ist.')}
                </p>
                <div className="flex flex-wrap justify-end gap-2">
                  <Button
                    size="sm"
                    onClick={() => {
                      tick('totp', true);
                      setAskTotp(false);
                    }}
                  >
                    {t('Schlüssel mitgeben')}
                  </Button>
                  <Button
                    variant="primary"
                    size="sm"
                    data-autofocus
                    onClick={() => setAskTotp(false)}
                  >
                    {t('Lieber nicht')}
                  </Button>
                </div>
              </Hint>
            )}
            {!askTotp && fields.some((f) => f.field === 'totp' && f.checked) && (
              <p className="field-hint">
                {t(
                  'Der Schlüssel des Einmal-Codes reist verschlüsselt im Send mit. Die Send-Seite zeigt nur die laufenden Codes – wer den Link hat, kann den Schlüssel aber auslesen und damit auch nach dem Löschen des Sends weiter Codes erzeugen.',
                )}
              </p>
            )}
          </fieldset>
          <div className="editor-row">
            <label className="field">
              <span>{t('Löschen nach')}</span>
              <select value={days} onChange={(e) => setDays(Number(e.target.value))}>
                {DAYS.map((n) => (
                  <option key={n} value={n}>
                    {n === 1 ? t('1 Tag') : t('{n} Tagen', { n })}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>{t('Höchstens so oft öffnen')}</span>
              <input
                type="number"
                min={1}
                max={1000}
                value={maxAccess}
                placeholder={t('unbegrenzt')}
                onChange={(e) => setMaxAccess(e.target.value.replace(/\D/g, ''))}
              />
            </label>
          </div>
          {options?.emails && (
            <label className="check">
              <input
                type="checkbox"
                role="switch"
                checked={onlyFor}
                onChange={(e) => setOnlyFor(e.target.checked)}
              />
              <span>{t('Nur für diese Adressen (mit Code per E-Mail)')}</span>
            </label>
          )}
          {onlyFor ? (
            <label className="field">
              <span>{t('E-Mail-Adressen')}</span>
              <textarea
                rows={2}
                value={emails}
                spellCheck={false}
                placeholder="name@example.com"
                onChange={(e) => setEmails(e.target.value)}
              />
            </label>
          ) : (
            <label className="field">
              <span>{t('Passwort (optional)')}</span>
              <input
                type="password"
                value={password}
                autoComplete="new-password"
                onChange={(e) => setPassword(e.target.value)}
              />
            </label>
          )}
          {options && options.domains.length > 0 && (
            <label className="field">
              <span>{t('Link-Adresse')}</span>
              <select value={domain} onChange={(e) => setDomain(e.target.value)}>
                <option value="">{t('Hauptadresse des Servers')}</option>
                {options.domains.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.url.replace(/^https?:\/\//, '')}
                  </option>
                ))}
              </select>
            </label>
          )}
          <label className="check">
            <input
              type="checkbox"
              role="switch"
              checked={hideText}
              onChange={(e) => setHideText(e.target.checked)}
            />
            <span>{t('Text erst nach einem Klick zeigen')}</span>
          </label>
          {error && (
            <p className="form-error" role="alert">
              {error}
            </p>
          )}
        </div>
      )}
    </Modal>
  );
}
