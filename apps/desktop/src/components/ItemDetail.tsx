import { Button, Icon, IconButton, ICONS } from '@uwusuite/design';
import { useEffect, useState, type FormEvent, type ReactNode } from 'react';
import {
  copyField,
  deleteItem,
  deletePasskey,
  failure,
  itemPasskeys,
  openItemUri,
  openWebVault,
  restoreItem,
  revealField,
  setFavorite,
  totpCode,
  vaultItem,
  verifyReprompt,
  type ItemDetail as Detail,
  type ItemSummary,
  type Overview,
  type PasskeyInfo,
  type TotpCode,
} from '../lib/api';
import { errorText, toastError } from '../lib/errors';
import { charClasses, copiedText, spacedCode, when } from '../lib/format';
import { t, useLanguage } from '../lib/i18n';
import { IDENTITY_LABEL, KIND_LABEL, SECURITY_LABEL } from '../lib/items';
import { getSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import { setMaskedState, useUwu } from '../lib/uwu';
import { ENTERPRISE_KEYS, isEnterprise, readWifi, type WifiView } from '../lib/wifi';
import { IconMenu, ReminderCard, ShareSendDialog, VersionsCard } from './ItemExtras';
import { ItemTile } from './ItemTile';
import { Modal } from './Modal';
import { PasswordInput } from './PasswordInput';
import { playNyu } from './nyu/stage';
import { WifiShare } from './WifiShare';

async function copy(id: string, field: string) {
  try {
    await copyField(id, field);
    toast(copiedText(field, getSettings().clipboardClear));
    playNyu('copied');
  } catch (e) {
    toastError(e);
  }
}

/** A password with letters, digits and symbols told apart by colour. */
export function Colored({ text }: { text: string }) {
  return (
    <span className="colored">
      {charClasses(text).map((run, index) => (
        <span key={index} data-class={run.kind}>
          {run.text}
        </span>
      ))}
    </span>
  );
}

function CopyButton({ id, field, label }: { id: string; field: string; label: string }) {
  useLanguage();
  return (
    <IconButton
      icon={ICONS.copy}
      size="sm"
      className="phone:size-9"
      onClick={() => void copy(id, field)}
      label={t('{label} kopieren', { label })}
    />
  );
}

function Row({
  label,
  children,
  actions,
  mono,
}: {
  label: string;
  children: ReactNode;
  actions?: ReactNode;
  mono?: boolean;
}) {
  return (
    <div className="detail-row">
      <div className="detail-text">
        <span className="detail-label">{label}</span>
        <span className={mono ? 'detail-value mono' : 'detail-value'}>{children}</span>
      </div>
      {actions && <div className="detail-actions">{actions}</div>}
    </div>
  );
}

/** A value that stays dots until the eye is clicked; then it comes from Rust. */
function SecretRow({
  id,
  field,
  label,
  masked = '••••••••••••',
  multiline,
  colored = true,
}: {
  id: string;
  field: string;
  label: string;
  masked?: string;
  multiline?: boolean;
  colored?: boolean;
}) {
  useLanguage();
  const [value, setValue] = useState<string | null>(null);
  // Out of sight when the window is left alone for a while.
  useEffect(() => {
    if (value === null) return;
    const timer = window.setTimeout(() => setValue(null), 60_000);
    return () => window.clearTimeout(timer);
  }, [value]);
  const toggle = async () => {
    if (value !== null) {
      setValue(null);
      return;
    }
    try {
      setValue(await revealField(id, field));
    } catch (e) {
      toastError(e);
    }
  };
  return (
    <Row
      label={label}
      mono
      actions={
        <>
          <IconButton
            icon={value === null ? ICONS.show : ICONS.hide}
            size="sm"
            className="phone:size-9"
            onClick={() => void toggle()}
            label={
              value === null ? t('{label} zeigen', { label }) : t('{label} verbergen', { label })
            }
            aria-pressed={value !== null}
          />
          <CopyButton id={id} field={field} label={label} />
        </>
      }
    >
      {value === null ? (
        <span className="masked">{masked}</span>
      ) : multiline ? (
        <pre className="secret-block">{value}</pre>
      ) : colored ? (
        <Colored text={value} />
      ) : (
        value
      )}
    </Row>
  );
}

/** The current one-time code, counting down, fetched again when it turns over. */
function TotpRow({ id }: { id: string }) {
  useLanguage();
  const [code, setCode] = useState<TotpCode | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let stopped = false;
    let timer: number | undefined;
    const tick = async () => {
      try {
        const next = await totpCode(id);
        if (stopped) return;
        setCode(next);
        setError(null);
      } catch (e) {
        if (!stopped) setError(errorText(e));
      }
      if (!stopped) timer = window.setTimeout(() => void tick(), 1000);
    };
    void tick();
    return () => {
      stopped = true;
      window.clearTimeout(timer);
    };
  }, [id]);

  const fraction = code ? code.remaining / code.period : 0;
  const circumference = 2 * Math.PI * 9;
  return (
    <Row
      label={t('Einmal-Code (TOTP)')}
      mono
      actions={<CopyButton id={id} field="totp" label={t('Code')} />}
    >
      {error ? (
        <span className="detail-error">{error}</span>
      ) : code ? (
        <span className="totp-block">
          <span className="totp" data-soon={code.remaining <= 5 || undefined}>
            <span className="totp-code">{spacedCode(code.code)}</span>
            <svg className="totp-ring" viewBox="0 0 24 24" width="22" height="22" aria-hidden>
              <circle cx="12" cy="12" r="9" className="totp-track" />
              <circle
                cx="12"
                cy="12"
                r="9"
                className="totp-left"
                strokeDasharray={circumference}
                strokeDashoffset={circumference * (1 - fraction)}
                transform="rotate(-90 12 12)"
              />
            </svg>
            <span className="totp-seconds">{code.remaining}</span>
          </span>
          {code.showNext && (
            <span className="totp-next">
              <span>
                {t('Nächster:')} <b>{spacedCode(code.next)}</b>
              </span>
              <CopyButton id={id} field="totp-next" label={t('Nächsten Code')} />
            </span>
          )}
        </span>
      ) : (
        '…'
      )}
    </Row>
  );
}

function Section({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="detail-card">
      {title && <h3 className="detail-card-title">{title}</h3>}
      {children}
    </section>
  );
}

/**
 * A Wi-Fi network's own values (docs/wifi.md), with copy buttons and the QR code. `actions`
 * is a slot for what a platform adds next to the code (the phone apps' *Connect*).
 */
function WifiSection({
  id,
  wifi,
  actions,
  onShare,
}: {
  id: string;
  wifi: WifiView;
  actions?: ReactNode;
  onShare: () => void;
}) {
  useLanguage();
  const enterprise: [(typeof ENTERPRISE_KEYS)[number], string][] = [
    ['eap', t('EAP-Methode')],
    ['phase2', t('Phase 2')],
    ['identity', t('Identität')],
    ['anonymous', t('Anonyme Identität')],
    ['ca', t('CA-Zertifikat')],
  ];
  return (
    <Section>
      <Row
        label={t('Netzwerkname (SSID)')}
        mono
        actions={
          wifi.ssid && wifi.from.ssid !== undefined ? (
            <CopyButton id={id} field={`field:${wifi.from.ssid}`} label="SSID" />
          ) : undefined
        }
      >
        {wifi.ssid || <span className="muted">—</span>}
      </Row>
      {wifi.password?.hasValue && wifi.security !== 'None' && (
        <SecretRow id={id} field={`field:${wifi.password.index}`} label={t('Passwort')} />
      )}
      <Row label={t('Sicherheit')}>
        {wifi.security ? (
          t(SECURITY_LABEL[wifi.security] ?? wifi.security)
        ) : (
          <span className="muted">—</span>
        )}
      </Row>
      <Row label={t('Verstecktes Netzwerk')}>
        <span className="bool" data-on={wifi.hidden || undefined}>
          {wifi.hidden ? t('Ja') : t('Nein')}
        </span>
      </Row>
      {isEnterprise(wifi.security) &&
        enterprise.map(([key, label]) => {
          const value = wifi[key];
          const from = wifi.from[key];
          return value ? (
            <Row
              key={key}
              label={label}
              actions={
                from !== undefined ? (
                  <CopyButton id={id} field={`field:${from}`} label={label} />
                ) : undefined
              }
            >
              {value === 'none' ? t('Keine') : value}
            </Row>
          ) : null;
        })}
      <div className="wifi-actions">
        {actions}
        <Button
          variant="ghost"
          size="sm"
          icon={ICONS.qrCode}
          onClick={onShare}
          aria-haspopup="dialog"
          data-wifi-share
        >
          {t('Als QR-Code teilen')}
        </Button>
      </div>
    </Section>
  );
}

/** The master password again, before an item with re-prompt shows anything. */
function Reprompt({ id, onPassed }: { id: string; onPassed: () => void }) {
  useLanguage();
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await verifyReprompt(id, password);
      onPassed();
    } catch (e) {
      setError(errorText(e));
      setPassword('');
    } finally {
      setBusy(false);
    }
  };
  return (
    <form className="detail-card reprompt" onSubmit={submit}>
      <p className="detail-card-title">
        <Icon icon={ICONS.masterPassword} />
        {t('Master-Passwort erforderlich')}
      </p>
      <p className="dialog-lead">
        {t(
          'Dieser Eintrag ist besonders geschützt. Gib dein Master-Passwort ein, um ihn zu öffnen.',
        )}
      </p>
      <PasswordInput
        value={password}
        onChange={setPassword}
        autoFocus
        disabled={busy}
        label={t('Master-Passwort')}
      />
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      <div className="form-actions">
        <span className="spacer" />
        <Button variant="primary" type="submit" disabled={busy || !password}>
          {busy ? t('Prüft …') : t('Öffnen')}
        </Button>
      </div>
    </form>
  );
}

/** Asks before something is thrown away for good. */
function ConfirmDelete({
  name,
  permanent,
  masked,
  onCancel,
  onConfirm,
}: {
  name: string;
  permanent: boolean;
  /** The item's masked address, when it has one. */
  masked?: string | null;
  onCancel: () => void;
  /** `disableMasked`: switch the masked address off first. */
  onConfirm: (disableMasked: boolean) => void;
}) {
  useLanguage();
  // Mail to an address whose item is gone should stop, unless someone says otherwise.
  const [disableMasked, setDisableMasked] = useState(true);
  return (
    <Modal
      title={permanent ? t('Endgültig löschen?') : t('In den Papierkorb?')}
      tone={permanent ? 'warning' : 'default'}
      size="small"
      onCancel={onCancel}
      footer={
        <>
          <span className="spacer" />
          <Button
            variant="danger"
            data-secondary
            onClick={() => onConfirm(Boolean(masked && permanent && disableMasked))}
          >
            {permanent ? t('Endgültig löschen') : t('In den Papierkorb')}
          </Button>
          <Button variant="primary" data-autofocus onClick={onCancel}>
            {t('Abbrechen')}
          </Button>
        </>
      }
    >
      <p className="dialog-lead">
        {permanent
          ? t(
              '„{name}“ wird auf dem Server gelöscht. Das lässt sich nicht rückgängig machen – auch nicht im Web-Tresor.',
              { name },
            )
          : t('„{name}“ wandert in den Papierkorb. Der Server hebt ihn dort noch 30 Tage auf.', {
              name,
            })}
      </p>
      {masked && permanent && (
        <label className="check">
          <input
            type="checkbox"
            checked={disableMasked}
            onChange={(e) => setDisableMasked(e.target.checked)}
          />
          <span>{t('Die maskierte Adresse {email} abschalten', { email: masked })}</span>
        </label>
      )}
    </Modal>
  );
}

/**
 * A login's passkeys: for which site, for whom, since when — and deleting
 * one. Nothing secret comes up from Rust for this.
 */
function PasskeysSection({ id, revision }: { id: string; revision: string | null }) {
  useLanguage();
  const [keys, setKeys] = useState<PasskeyInfo[] | null>(null);
  const [asking, setAsking] = useState<PasskeyInfo | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let stopped = false;
    itemPasskeys(id)
      .then((next) => !stopped && setKeys(next))
      .catch((e) => {
        if (!stopped) setKeys([]);
        toastError(e);
      });
    return () => {
      stopped = true;
    };
  }, [id, revision]);
  if (!keys || keys.length === 0) return null;

  const remove = async (key: PasskeyInfo) => {
    setBusy(true);
    try {
      await deletePasskey(id, key.index, key.credentialId || key.fingerprint);
      // The places of the others have moved: read them again before the
      // next delete.
      setKeys(null);
      setKeys(await itemPasskeys(id));
      toast(t('Passkey gelöscht.'));
      playNyu('trashed');
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
      setAsking(null);
    }
  };

  return (
    <Section title={keys.length === 1 ? t('Passkey') : t('Passkeys')}>
      {keys.map((key) => {
        // The rpId is what the passkey is bound to; the name is the site's own claim.
        const site = key.rpId || key.rpName || t('Unbekannte Website');
        const user = key.userName || key.userDisplayName;
        const created = when(key.creationDate);
        return (
          <div className="detail-row passkey-row" key={`${key.index}-${key.credentialId}`}>
            <div className="detail-text">
              <span className="detail-label">
                <Icon icon={ICONS.passkey} size="xs" />
                {site}
              </span>
              <span className="detail-value">
                <span>{user ?? <span className="muted">{t('ohne Benutzernamen')}</span>}</span>
                <span className="passkey-meta">
                  {[
                    key.rpName && key.rpName !== site ? key.rpName : null,
                    created && t('erstellt {when}', { when: created }),
                    !key.readable && t('lässt sich nicht lesen – nur löschen'),
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                </span>
              </span>
            </div>
            <div className="detail-actions">
              <IconButton
                icon={ICONS.delete}
                size="sm"
                className="phone:size-9"
                disabled={busy}
                label={t('Passkey für {site} löschen', { site })}
                onClick={() => setAsking(key)}
              />
            </div>
          </div>
        );
      })}
      {asking && (
        <Modal
          title={t('Passkey löschen?')}
          tone="warning"
          size="small"
          onCancel={() => setAsking(null)}
          footer={
            <>
              <span className="spacer" />
              <Button
                variant="danger"
                data-secondary
                disabled={busy}
                onClick={() => void remove(asking)}
              >
                {t('Löschen')}
              </Button>
              <Button variant="primary" data-autofocus onClick={() => setAsking(null)}>
                {t('Abbrechen')}
              </Button>
            </>
          }
        >
          <p className="dialog-lead">
            {t(
              'Mit diesem Passkey meldest du dich danach nicht mehr bei {site} an. Leg vorher einen anderen Weg zur Anmeldung an, falls du keinen mehr hast.',
              { site: asking.rpId || asking.rpName || t('Unbekannte Website') },
            )}
          </p>
        </Modal>
      )}
    </Section>
  );
}

/** The hosts of all websites but the first, at most three ("a.example, b.example, …"). */
function otherHosts(uris: NonNullable<Detail['login']>['uris']): string {
  const hosts = uris.slice(1).map((uri) => uri.host ?? uri.uri);
  return hosts.slice(0, 3).join(', ') + (hosts.length > 3 ? ', …' : '');
}

/** The first website, the others behind "+2 weitere Websites". */
function WebsitesSection({ id, uris }: { id: string; uris: NonNullable<Detail['login']>['uris'] }) {
  useLanguage();
  const [all, setAll] = useState(false);
  useEffect(() => setAll(false), [id]);
  const shown = all ? uris : uris.slice(0, 1);
  const more = uris.length - 1;
  return (
    <Section title={uris.length === 1 ? t('Website') : t('Websites')}>
      {shown.map((uri, index) => (
        <Row
          key={index}
          label={uri.host ?? t('Adresse')}
          actions={
            <>
              {uri.openable && (
                <IconButton
                  icon={ICONS.openExternal}
                  size="sm"
                  className="phone:size-9"
                  onClick={() =>
                    void openItemUri(id, index).catch((e) => toast(String(e), 'error'))
                  }
                  label={t('{uri} im Browser öffnen', { uri: uri.uri })}
                />
              )}
              <CopyButton id={id} field={`uri:${index}`} label={t('Adresse')} />
            </>
          }
        >
          <span className="uri">{uri.uri}</span>
        </Row>
      ))}
      {more > 0 && (
        <button
          type="button"
          className="more-toggle link-button"
          aria-expanded={all}
          onClick={() => setAll(!all)}
        >
          {all
            ? t('Weniger zeigen')
            : more === 1
              ? t('+1 weitere Website')
              : t('+{n} weitere Websites', { n: more })}
          {!all && (
            // Which sites, even folded: an added one shouldn't hide behind a number.
            <span className="more-hosts">{otherHosts(uris)}</span>
          )}
          <Icon
            icon={ICONS.expand}
            size="xs"
            className={all ? 'transition-transform' : '-rotate-90 transition-transform'}
          />
        </button>
      )}
    </Section>
  );
}

export function ItemDetail({
  summary,
  overview,
  onEdit,
  wifiActions,
}: {
  summary: ItemSummary;
  overview: Overview | null;
  onEdit: () => void;
  /** More buttons next to a Wi-Fi network's QR code, where the platform can do more. */
  wifiActions?: (wifi: WifiView) => ReactNode;
}) {
  useLanguage();
  const [detail, setDetail] = useState<Detail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showHistory, setShowHistory] = useState(false);
  const [asking, setAsking] = useState<null | 'trash' | 'permanent'>(null);
  const [sharing, setSharing] = useState(false);
  const [sharingWifi, setSharingWifi] = useState(false);
  const [busy, setBusy] = useState(false);
  const uwu = useUwu();
  const id = summary.id;
  const masked = uwu.masked[id] ?? null;
  const due = uwu.reminders[id]?.isDue ?? false;

  const load = () => {
    vaultItem(id)
      .then((next) => {
        setDetail(next);
        setError(null);
      })
      .catch((e) => {
        if (failure(e).kind !== 'not-found') setError(errorText(e));
      });
  };
  // Reload when the sync brought a new revision of this item.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(load, [id, summary.revisionDate]);

  const act = async (what: () => Promise<void>, done: string) => {
    setBusy(true);
    try {
      await what();
      toast(done);
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
      setAsking(null);
    }
  };

  const folder = overview?.folders.find((f) => f.id === summary.folderId)?.name;
  const org = overview?.organizations.find((o) => o.id === summary.organizationId)?.name;
  const collections = (overview?.collections ?? [])
    .filter((c) => summary.collectionIds.includes(c.id))
    .map((c) => c.name);

  const d = detail;
  const wifi = d && !d.locked && summary.kind === 'wifi' ? readWifi(d.fields ?? []) : null;
  const fields = wifi ? wifi.others : (d?.fields ?? []);
  return (
    <article className="detail" aria-label={summary.name}>
      <header className="detail-head">
        <span className="tile-wrap">
          <ItemTile item={summary} size="large" />
          <IconMenu summary={summary} detail={d && !d.locked ? d : null} />
        </span>
        <div className="detail-title">
          <h2>
            {summary.name || t('(ohne Namen)')}
            {summary.favorite && (
              <Icon icon={ICONS.favorite} className="badge-star" label={t('Favorit')} />
            )}
          </h2>
          <p className="chips">
            <span className="chip">{t(KIND_LABEL[summary.kind])}</span>
            {folder && (
              <span className="chip">
                <Icon icon={ICONS.folder} size="xs" />
                {folder}
              </span>
            )}
            {org && (
              <span className="chip">
                <Icon icon={ICONS.organization} size="xs" />
                {org}
                {collections.length > 0 && ` · ${collections.join(', ')}`}
              </span>
            )}
            {masked && (
              <span className="chip" title={t('Maskierte Adresse')}>
                <Icon icon={ICONS.maskedAddress} size="xs" />
                {masked.email}
              </span>
            )}
            {due && !summary.deleted && (
              <span className="chip chip-due">
                <Icon icon={ICONS.reminder} size="xs" />
                {t('Neues Passwort fällig')}
              </span>
            )}
            {summary.deleted && <span className="chip chip-muted">{t('Im Papierkorb')}</span>}
          </p>
        </div>
        <div className="detail-tools">
          {summary.deleted ? (
            <>
              <Button
                variant="ghost"
                size="sm"
                icon={ICONS.restore}
                disabled={busy}
                onClick={() => void act(() => restoreItem(id), t('Aus dem Papierkorb geholt ✧'))}
              >
                {t('Wiederherstellen')}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                icon={ICONS.delete}
                className="text-danger-ink!"
                disabled={busy}
                onClick={() => setAsking('permanent')}
              >
                {t('Endgültig löschen')}
              </Button>
            </>
          ) : (
            <>
              <IconButton
                icon={ICONS.favorite}
                size="sm"
                className={
                  summary.favorite ? 'star-toggle text-pink! [&_svg]:fill-current' : 'star-toggle'
                }
                disabled={busy}
                aria-pressed={summary.favorite}
                label={summary.favorite ? t('Favorit entfernen') : t('Zu Favoriten')}
                onClick={() =>
                  void act(
                    () => setFavorite(id, !summary.favorite),
                    summary.favorite ? t('Kein Favorit mehr') : t('Favorit ✧'),
                  )
                }
              />
              <IconButton
                icon={ICONS.send}
                size="sm"
                disabled={busy || !d || d.locked || summary.broken}
                label={t('Als Send teilen …')}
                onClick={() => setSharing(true)}
              />
              <IconButton
                icon={ICONS.delete}
                size="sm"
                disabled={busy}
                label={t('In den Papierkorb')}
                onClick={() => setAsking('trash')}
              />
              <Button
                variant="primary"
                size="sm"
                icon={ICONS.edit}
                disabled={busy || summary.broken}
                onClick={onEdit}
              >
                {t('Bearbeiten')}
              </Button>
            </>
          )}
        </div>
      </header>

      {asking && (
        <ConfirmDelete
          name={summary.name || t('(ohne Namen)')}
          permanent={asking === 'permanent'}
          masked={masked?.email}
          onCancel={() => setAsking(null)}
          onConfirm={(disableMasked) =>
            void act(
              async () => {
                // UwUMail being away doesn't keep the item from going.
                if (disableMasked && masked)
                  await setMaskedState(masked.id, 'disabled').catch((e) => toastError(e));
                await deleteItem(id, asking === 'permanent');
                playNyu('trashed');
              },
              asking === 'permanent' ? t('Gelöscht.') : t('Im Papierkorb.'),
            )
          }
        />
      )}

      {sharing && d && !d.locked && (
        <ShareSendDialog summary={summary} detail={d} onClose={() => setSharing(false)} />
      )}

      {sharingWifi && wifi && (
        <WifiShare itemId={id} wifi={wifi} onClose={() => setSharingWifi(false)} />
      )}

      {error && (
        <p className="notice" data-tone="error">
          {error}
        </p>
      )}
      {summary.broken && (
        <p className="notice" data-tone="error">
          {t(
            'Ein Teil dieses Eintrags ließ sich nicht entschlüsseln. Im Web-Tresor sieht er vielleicht anders aus.',
          )}
        </p>
      )}

      {d?.locked && <Reprompt id={id} onPassed={load} />}

      {d && !d.locked && (
        <>
          {d.login && (
            <Section>
              {d.login.username && (
                <Row
                  label={t('Benutzername')}
                  actions={<CopyButton id={id} field="username" label={t('Benutzername')} />}
                >
                  {d.login.username}
                </Row>
              )}
              {d.login.hasPassword && <SecretRow id={id} field="password" label={t('Passwort')} />}
              {d.login.hasTotp && <TotpRow id={id} />}
              {!d.login.username && !d.login.hasPassword && !d.login.hasTotp && (
                <p className="detail-empty-line">{t('Kein Benutzername, kein Passwort.')}</p>
              )}
            </Section>
          )}

          {d.login && d.login.uris.length > 0 && <WebsitesSection id={id} uris={d.login.uris} />}

          {d.login && d.login.passkeys > 0 && (
            <PasskeysSection id={id} revision={summary.revisionDate} />
          )}

          {d.card && (
            <Section>
              {d.card.cardholderName && (
                <Row
                  label={t('Karteninhaber')}
                  actions={<CopyButton id={id} field="card-name" label={t('Karteninhaber')} />}
                >
                  {d.card.cardholderName}
                </Row>
              )}
              {d.card.brand && <Row label={t('Marke')}>{d.card.brand}</Row>}
              {d.card.numberEnding && (
                <SecretRow
                  id={id}
                  field="card-number"
                  label={t('Kartennummer')}
                  masked={`•••• •••• •••• ${d.card.numberEnding}`}
                  colored={false}
                />
              )}
              {(d.card.expMonth || d.card.expYear) && (
                <Row
                  label={t('Gültig bis')}
                  mono
                  actions={<CopyButton id={id} field="card-expiry" label={t('Gültig bis')} />}
                >
                  {(d.card.expMonth ?? '').padStart(2, '0')}/{d.card.expYear ?? ''}
                </Row>
              )}
              {d.card.hasCode && (
                <SecretRow
                  id={id}
                  field="card-code"
                  label={t('Prüfnummer')}
                  masked="•••"
                  colored={false}
                />
              )}
            </Section>
          )}

          {d.identity && d.identity.length > 0 && (
            <Section>
              {d.identity.map((entry) =>
                entry.sensitive ? (
                  <SecretRow
                    key={entry.name}
                    id={id}
                    field={`identity:${entry.name}`}
                    label={t(IDENTITY_LABEL[entry.name] ?? entry.name)}
                    colored={false}
                  />
                ) : (
                  <Row
                    key={entry.name}
                    label={t(IDENTITY_LABEL[entry.name] ?? entry.name)}
                    actions={
                      <CopyButton
                        id={id}
                        field={`identity:${entry.name}`}
                        label={t(IDENTITY_LABEL[entry.name] ?? entry.name)}
                      />
                    }
                  >
                    {entry.value}
                  </Row>
                ),
              )}
            </Section>
          )}

          {d.sshKey && (
            <Section>
              {d.sshKey.fingerprint && (
                <Row
                  label={t('Fingerprint')}
                  mono
                  actions={<CopyButton id={id} field="ssh-fingerprint" label={t('Fingerprint')} />}
                >
                  {d.sshKey.fingerprint}
                </Row>
              )}
              {d.sshKey.publicKey && (
                <Row
                  label={t('Öffentlicher Schlüssel')}
                  mono
                  actions={
                    <CopyButton id={id} field="ssh-public" label={t('Öffentlicher Schlüssel')} />
                  }
                >
                  <span className="uri">{d.sshKey.publicKey}</span>
                </Row>
              )}
              {d.sshKey.hasPrivateKey && (
                <SecretRow
                  id={id}
                  field="ssh-private"
                  label={t('Privater Schlüssel')}
                  masked="-----BEGIN ••••••••-----"
                  multiline
                />
              )}
            </Section>
          )}

          {wifi && (
            <WifiSection
              id={id}
              wifi={wifi}
              actions={wifiActions?.(wifi)}
              onShare={() => setSharingWifi(true)}
            />
          )}

          {d.notes && (
            <Section title={t('Notizen')}>
              <div className="notes">
                <p>{d.notes}</p>
                <CopyButton id={id} field="notes" label={t('Notizen')} />
              </div>
            </Section>
          )}

          {fields.length > 0 && (
            <Section title={t('Eigene Felder')}>
              {fields.map((field) => {
                const label = field.name || t('Feld {n}', { n: field.index + 1 });
                if (field.kind === 'hidden' && field.hasValue)
                  return (
                    <SecretRow
                      key={field.index}
                      id={id}
                      field={`field:${field.index}`}
                      label={label}
                    />
                  );
                if (field.kind === 'boolean')
                  return (
                    <Row key={field.index} label={label}>
                      <span className="bool" data-on={field.value === 'true' || undefined}>
                        {field.value === 'true' ? t('Ja') : t('Nein')}
                      </span>
                    </Row>
                  );
                if (field.kind === 'linked')
                  return (
                    <Row key={field.index} label={label}>
                      <span className="muted">{t('verknüpft mit einem anderen Feld')}</span>
                    </Row>
                  );
                return (
                  <Row
                    key={field.index}
                    label={label}
                    actions={
                      field.value ? (
                        <CopyButton id={id} field={`field:${field.index}`} label={label} />
                      ) : undefined
                    }
                  >
                    {field.value ?? <span className="muted">—</span>}
                  </Row>
                );
              })}
            </Section>
          )}

          {d.passwordHistory && d.passwordHistory.length > 0 && (
            <Section>
              <Button
                variant="ghost"
                size="sm"
                icon={ICONS.history}
                className="history-toggle text-muted!"
                aria-expanded={showHistory}
                onClick={() => setShowHistory(!showHistory)}
              >
                {d.passwordHistory.length === 1
                  ? t('1 früheres Passwort')
                  : t('{n} frühere Passwörter', { n: d.passwordHistory.length })}
                <Icon
                  icon={ICONS.expand}
                  size="xs"
                  className={
                    showHistory ? 'transition-transform' : '-rotate-90 transition-transform'
                  }
                />
              </Button>
              {showHistory &&
                d.passwordHistory.map((entry) => (
                  <SecretRow
                    key={entry.index}
                    id={id}
                    field={`history:${entry.index}`}
                    label={when(entry.lastUsed) ?? t('Früher')}
                  />
                ))}
            </Section>
          )}

          <ReminderCard summary={summary} />
          <VersionsCard summary={summary} />

          <footer className="detail-foot">
            {(d.attachments ?? 0) > 0 && (
              <p>
                <Icon icon={ICONS.file} size="xs" />
                {t('{n} Anhänge – die öffnest du vorerst im Web-Tresor.', {
                  n: d.attachments ?? 0,
                })}
              </p>
            )}
            <p className="muted">
              {[
                when(summary.revisionDate) &&
                  t('Geändert {when}', { when: when(summary.revisionDate) ?? '' }),
                when(d.creationDate) && t('Erstellt {when}', { when: when(d.creationDate) ?? '' }),
                d.login?.passwordRevisionDate &&
                  t('Passwort geändert {when}', { when: when(d.login.passwordRevisionDate) ?? '' }),
              ]
                .filter(Boolean)
                .join(' · ')}
            </p>
            <p className="detail-beta">
              <button
                type="button"
                className="link-button"
                onClick={() => void openWebVault().catch(() => undefined)}
              >
                {t('Im Web-Tresor öffnen')}
                <Icon icon={ICONS.openExternal} size="xs" />
              </button>
            </p>
          </footer>
        </>
      )}
    </article>
  );
}
