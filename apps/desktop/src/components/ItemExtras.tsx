/**
 * An item's part of UwULock Server's extras, shown in its details: its own
 * icon, the password renewal reminder, the entry versions — and sharing it as
 * a Send, which works on Bitwarden and Vaultwarden too.
 */

import { useEffect, useRef, useState, type ReactNode } from 'react';
import { copyGenerated, type ItemDetail as Detail, type ItemSummary } from '../lib/api';
import { errorText } from '../lib/errors';
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
import { Icon } from './Icon';
import { Modal } from './Modal';

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
 * The button on an item's tile: pick a picture, take the icon from a device
 * on the local network, or remove the own icon.
 */
export function IconMenu({ summary, detail }: { summary: ItemSummary; detail: Detail | null }) {
  useLanguage();
  const uwu = useUwu();
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [busy, setBusy] = useState(false);
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
      <button
        className="icon-button tile-edit"
        disabled={busy}
        title={t('Symbol ändern')}
        aria-label={t('Symbol ändern')}
        aria-haspopup="menu"
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu({ x: rect.left, y: rect.bottom + 4 });
        }}
      >
        <Icon name="image" size={13} />
      </button>
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
              icon: 'upload',
              onSelect: () => input.current?.click(),
            },
            ...(local
              ? [
                  {
                    label: t('Symbol vom Gerät holen'),
                    icon: 'network' as const,
                    onSelect: () =>
                      void run(() => fetchDeviceIcon(summary.id), t('Symbol vom Gerät geholt ✧')),
                  },
                ]
              : []),
            ...(hasOwn
              ? [
                  {
                    label: t('Eigenes Symbol entfernen'),
                    icon: 'trash' as const,
                    danger: true,
                    onSelect: () =>
                      void run(() => deleteOwnIcon(summary.id), t('Eigenes Symbol entfernt.')),
                  },
                ]
              : []),
          ]}
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

/** "Remind me to renew this password": every so many months, or on a date. */
export function ReminderCard({ summary }: { summary: ItemSummary }) {
  useLanguage();
  const uwu = useUwu();
  const reminder = uwu.reminders[summary.id];
  const [editing, setEditing] = useState(false);
  const [mode, setMode] = useState<'months' | 'date'>('months');
  const [months, setMonths] = useState(6);
  const [date, setDate] = useState('');
  const [busy, setBusy] = useState(false);
  if (!has(uwu, 'reminders') || summary.kind !== 'login' || summary.deleted) return null;

  const run = async (what: () => Promise<void>, done: string) => {
    setBusy(true);
    try {
      await what();
      toast(done);
      setEditing(false);
    } catch (e) {
      toast(errorText(e), 'error');
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
    if (!reminder) return t('Keine Erinnerung.');
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
          <Icon name="bell" size={13} />
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
            <button className="quiet" disabled={busy} onClick={open}>
              {reminder ? t('Ändern') : t('Erinnern …')}
            </button>
            {reminder && (
              <button
                className="icon-button"
                disabled={busy}
                title={t('Erinnerung entfernen')}
                aria-label={t('Erinnerung entfernen')}
                onClick={() =>
                  void run(() => deleteReminder(summary.id), t('Erinnerung entfernt.'))
                }
              >
                <Icon name="trash" size={15} />
              </button>
            )}
          </div>
        </div>
      ) : (
        <div className="extras-form">
          <div className="segmented" role="radiogroup" aria-label={t('Wann')}>
            <button role="radio" aria-checked={mode === 'months'} onClick={() => setMode('months')}>
              {t('Regelmäßig')}
            </button>
            <button role="radio" aria-checked={mode === 'date'} onClick={() => setMode('date')}>
              {t('An einem Datum')}
            </button>
          </div>
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
            <button className="quiet" onClick={() => setEditing(false)}>
              {t('Abbrechen')}
            </button>
            <span className="spacer" />
            <button
              className="primary"
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
            </button>
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
      <button
        className="icon-button"
        title={value === null ? t('Zeigen') : t('Verbergen')}
        aria-label={value === null ? t('Zeigen') : t('Verbergen')}
        aria-pressed={value !== null}
        onClick={() =>
          value !== null
            ? setValue(null)
            : void load()
                .then(setValue)
                .catch((e) => toast(errorText(e), 'error'))
        }
      >
        <Icon name={value === null ? 'eye' : 'eyeOff'} size={14} />
      </button>
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
      <Icon name="chevron" size={12} />
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
        toast(errorText(e), 'error');
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
      toast(errorText(e), 'error');
    } finally {
      setBusy(false);
      setAsking(null);
    }
  };

  return (
    <Card>
      <button className="history-toggle quiet" aria-expanded={open} onClick={() => setOpen(!open)}>
        <Icon name="layers" size={15} />
        {versions && open
          ? versions.length === 1
            ? t('1 frühere Version')
            : t('{n} frühere Versionen', { n: versions.length })
          : t('Frühere Versionen')}
        <Icon name="chevron" size={14} className={open ? 'turned' : undefined} />
      </button>
      {open && versions === null && <p className="detail-empty-line">{t('Einen Moment …')}</p>}
      {open && versions?.length === 0 && (
        <p className="detail-empty-line">
          {t('Noch keine. Jede Änderung hebt den Stand davor hier auf.')}
        </p>
      )}
      {open &&
        versions?.map((version) => (
          <div className="version" key={version.id}>
            <button
              className="version-head quiet"
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
              {version.broken && <Icon name="warning" size={13} className="badge-warning" />}
              <Icon
                name="chevron"
                size={13}
                className={expanded === version.id ? 'turned' : undefined}
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
                  <button
                    className="quiet danger-text"
                    disabled={busy}
                    onClick={() => setAsking({ kind: 'delete', id: version.id })}
                  >
                    <Icon name="trash" size={14} />
                    {t('Version löschen')}
                  </button>
                  <span className="spacer" />
                  <button
                    className="primary"
                    disabled={busy || version.broken || summary.broken}
                    onClick={() => setAsking({ kind: 'restore', id: version.id })}
                  >
                    <Icon name="history" size={14} />
                    {t('Wiederherstellen')}
                  </button>
                </div>
              </div>
            )}
          </div>
        ))}
      {open && versions && versions.length > 1 && (
        <p className="detail-empty-line">
          <button className="link-button" disabled={busy} onClick={() => setAsking('all')}>
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
          onCancel={() => setAsking(null)}
          footer={
            <>
              <span className="spacer" />
              <button
                className={asking !== 'all' && asking.kind === 'restore' ? 'primary' : 'danger'}
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
              </button>
              <button data-autofocus onClick={() => setAsking(null)}>
                {t('Abbrechen')}
              </button>
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

type Choice = { field: string; label: string; checked: boolean; secret: boolean };

/**
 * What an item has that can be shared — never the authenticator key, and no
 * secrets of an item whose organisation hides its passwords from this member.
 */
function choices(detail: Detail, viewPassword: boolean): Choice[] {
  const out: Choice[] = [];
  const add = (field: string, label: string, checked: boolean, secret = false) => {
    if (!viewPassword && secret && !WITHHELD_EXCEPT.test(field)) return;
    out.push({ field, label, checked, secret });
  };
  if (detail.login) {
    if (detail.login.username) add('username', t('Benutzername'), true);
    if (detail.login.hasPassword) add('password', t('Passwort'), true, true);
    detail.login.uris.forEach((uri, index) =>
      add(`uri:${index}`, uri.host ?? t('Website {n}', { n: index + 1 }), index === 0),
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
  const [fields, setFields] = useState<Choice[]>(() => choices(detail, summary.viewPassword));
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
      });
      setMade(result);
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
      .catch((e) => toast(errorText(e), 'error'));

  return (
    <Modal
      title={t('Als Send teilen')}
      onCancel={onClose}
      footer={
        made ? (
          <>
            <span className="spacer" />
            <button onClick={onClose}>{t('Fertig')}</button>
            <button className="primary" data-autofocus onClick={copy}>
              <Icon name="copy" size={15} />
              {t('Link kopieren')}
            </button>
          </>
        ) : (
          <>
            <button className="quiet" data-secondary onClick={onClose}>
              {t('Abbrechen')}
            </button>
            <span className="spacer" />
            <button
              className="primary"
              disabled={
                busy || !options || chosen.length === 0 || (onlyFor && addresses.length === 0)
              }
              onClick={() => void create()}
            >
              <Icon name="send" size={15} />
              {busy ? t('Erstellt …') : t('Send erstellen')}
            </button>
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
            {t(
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
                  onChange={(e) =>
                    setFields(
                      fields.map((f) =>
                        f.field === field.field ? { ...f, checked: e.target.checked } : f,
                      ),
                    )
                  }
                />
                <span>
                  {field.label}
                  {field.secret && <Icon name="lock" size={12} className="icon-gap" />}
                </span>
              </label>
            ))}
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
