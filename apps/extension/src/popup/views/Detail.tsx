import { Button, Icon, IconButton, ICONS } from '@uwusuite/design';
import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { playNyu } from '@desktop/components/nyu/stage';
import { QrCode } from '@desktop/components/QrCode';
import { ENTERPRISE_KEYS, isEnterprise, readWifi, wifiQr, type WifiView } from '@desktop/lib/wifi';
import { N_, t } from '../../shared/i18n';
import { ItemIcon } from '../icons';
import { FillReprompt } from './FillReprompt';
import type { ItemDetail, ItemKind, PasskeyInfo, TotpCode } from '../../shared/protocol';
import {
  copyField,
  deleteItem,
  deletePasskey,
  itemPasskeys,
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
  toastError,
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
  wifi: N_('WLAN'),
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

/** A Wi-Fi network's securities, as the app names them (`@desktop/lib/items`). */
const SECURITY_LABEL: Record<string, string> = {
  'WPA2/WPA3': N_('WPA2/WPA3 (gemischt)'),
  WPA: N_('WPA (veraltet)'),
  WEP: N_('WEP (unsicher)'),
  None: N_('Keine (offenes Netz)'),
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
      toastError(e);
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
      playNyu('trashed');
      onBack();
    } catch (e) {
      toastError(e);
    }
  };

  return (
    <div className="popup-scroll detail-view">
      <BackBar onBack={onBack}>
        {!summary.deleted && !item.locked && (
          <>
            <IconButton
              icon={ICONS.favorite}
              className={summary.favorite ? 'star-toggle [&>svg]:fill-pink' : 'star-toggle'}
              onClick={() => void toggleFavorite()}
              active={summary.favorite}
              label={summary.favorite ? t('Aus Favoriten entfernen') : t('Zu Favoriten')}
            />
            {!summary.broken && (
              <IconButton
                icon={ICONS.send}
                label={t('Als Send teilen')}
                onClick={() => onShare(id)}
              />
            )}
            {/* SSH keys and Wi-Fi networks are edited in the app or the web vault. */}
            {!summary.broken && summary.kind !== 'ssh-key' && summary.kind !== 'wifi' && (
              <IconButton
                icon={ICONS.edit}
                label={t('Bearbeiten')}
                onClick={() => onEdit(id, summary.kind)}
              />
            )}
          </>
        )}
        {summary.deleted && (
          <Button
            variant="ghost"
            size="sm"
            icon={ICONS.restore}
            onClick={() =>
              void restoreItem(id)
                .then(() => {
                  toast(t('Wiederhergestellt ✧'));
                  return load();
                })
                .catch((e) => toastError(e))
            }
          >
            {t('Wiederherstellen')}
          </Button>
        )}
        <IconButton
          icon={ICONS.delete}
          label={summary.deleted ? t('Endgültig löschen') : t('Löschen')}
          onClick={() => void remove()}
        />
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
      <Button variant="ghost" size="sm" icon={ICONS.back} className="back" onClick={onBack}>
        {t('Zurück')}
      </Button>
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
        <Icon icon={ICONS.masterPassword} size="xs" /> {t('Master-Passwort bestätigen')}
      </h3>
      <p className="dialog-lead">
        {t('Dieser Eintrag ist geschützt. Gib dein Master-Passwort ein, um ihn zu sehen.')}
      </p>
      <PasswordInput value={password} onChange={setPassword} autoFocus disabled={busy} />
      {error && <p className="form-error">{error}</p>}
      <div className="form-actions">
        <span className="spacer" />
        <Button variant="primary" size="sm" type="submit" busy={busy} disabled={!password}>
          {t('Bestätigen')}
        </Button>
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
  const wifi = item.summary.kind === 'wifi' ? readWifi(item.fields ?? []) : null;
  const fields = wifi ? wifi.others : (item.fields ?? []);
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
              <Button
                variant="primary"
                className="w-full"
                onClick={() =>
                  item.summary.reprompt
                    ? setAsking(true)
                    : void fillTab(id)
                        .then(() => window.close())
                        .catch((e) => toastError(e))
                }
              >
                {t('Auf dieser Seite ausfüllen')}
              </Button>
            </div>
          )}
          {login.username && (
            <Row id={id} label={t('Benutzername')} field="username" value={login.username} />
          )}
          {login.hasPassword && (
            <SecretRow id={id} label={t('Passwort')} field="password" colored />
          )}
          {login.hasTotp && <TotpRow id={id} />}
        </section>
      )}

      {login && login.uris.length > 0 && <Websites id={id} uris={login.uris} />}

      {login && login.passkeys > 0 && (
        <Passkeys id={id} revision={item.summary.revisionDate} deleted={item.summary.deleted} />
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

      {wifi && <WifiCard id={id} wifi={wifi} />}

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

      {fields.length > 0 && (
        <section className="detail-card">
          <h3 className="detail-card-title">{t('Eigene Felder')}</h3>
          {fields.map((field) =>
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

/**
 * A Wi-Fi network (docs/wifi.md): its values to copy and the QR code to join it with. Never
 * offered for filling. The code is drawn here; the password comes from the vault for it and is
 * dropped when the code is hidden.
 */
function WifiCard({ id, wifi }: { id: string; wifi: WifiView }) {
  const [code, setCode] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const open = wifi.security === 'None';
  const labels: Record<(typeof ENTERPRISE_KEYS)[number], string> = {
    eap: t('EAP-Methode'),
    phase2: t('Phase 2'),
    identity: t('Identität'),
    anonymous: t('Anonyme Identität'),
    ca: t('CA-Zertifikat'),
  };
  const toggle = async () => {
    if (code !== null) {
      setCode(null);
      return;
    }
    setBusy(true);
    try {
      const password =
        !open && wifi.password?.hasValue
          ? await revealField(id, `field:${wifi.password.index}`)
          : '';
      setCode(wifiQr({ ...wifi, password }));
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="detail-card" data-wifi>
      <Row
        id={id}
        label={t('Netzwerkname (SSID)')}
        value={wifi.ssid || '—'}
        field={wifi.ssid && wifi.from.ssid !== undefined ? `field:${wifi.from.ssid}` : undefined}
        mono
      />
      {wifi.password?.hasValue && !open && (
        <SecretRow id={id} label={t('Passwort')} field={`field:${wifi.password.index}`} colored />
      )}
      <Row
        id={id}
        label={t('Sicherheit')}
        value={wifi.security ? t(SECURITY_LABEL[wifi.security] ?? wifi.security) : '—'}
      />
      <Row id={id} label={t('Verstecktes Netzwerk')} value={wifi.hidden ? t('Ja') : t('Nein')} />
      {isEnterprise(wifi.security) &&
        ENTERPRISE_KEYS.map((key) =>
          wifi[key] ? (
            <Row
              key={key}
              id={id}
              label={labels[key]}
              value={wifi[key] === 'none' ? t('Keine') : wifi[key]}
              field={wifi.from[key] !== undefined ? `field:${wifi.from[key]}` : undefined}
            />
          ) : null,
        )}
      {wifi.ssid.trim() ? (
        <div className="detail-row wifi-qr-row">
          <Button
            variant="ghost"
            size="sm"
            icon={ICONS.qrCode}
            className="w-full"
            onClick={() => void toggle()}
            aria-expanded={code !== null}
            busy={busy}
            data-wifi-share
          >
            {code === null ? t('QR-Code zeigen') : t('QR-Code verbergen')}
          </Button>
          {code !== null && (
            <>
              <QrCode text={code} label={t('QR-Code für das WLAN {ssid}', { ssid: wifi.ssid })} />
              <p className="muted wifi-qr-hint">
                {open
                  ? t('Mit der Kamera scannen, um „{ssid}“ beizutreten.', { ssid: wifi.ssid })
                  : t(
                      'Mit der Kamera scannen, um „{ssid}“ beizutreten. Wer den Code sieht, kennt das Passwort.',
                      { ssid: wifi.ssid },
                    )}
              </p>
            </>
          )}
        </div>
      ) : null}
    </section>
  );
}

function CopyButton({ id, field }: { id: string; field: string }) {
  const settings = useSettings();
  return (
    <IconButton
      icon={ICONS.copy}
      size="sm"
      label={t('Kopieren')}
      onClick={() =>
        void copyField(id, field)
          .then(() => {
            toast(copiedText(field, settings?.clipboardClear ?? 30));
            playNyu('copied');
          })
          .catch((e) => toastError(e))
      }
    />
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
      toastError(e);
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
        <IconButton
          icon={value === null ? ICONS.show : ICONS.hide}
          size="sm"
          label={value === null ? t('Zeigen') : t('Verbergen')}
          onClick={() => void reveal()}
          aria-pressed={value !== null}
        />
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
          <span className="totp-block">
            <span className="totp" data-soon={code.remaining <= 5 || undefined}>
              <span className="totp-code mono">{spacedCode(code.code)}</span>
              <TotpRing remaining={code.remaining} period={code.period} />
              <span className="totp-seconds">{code.remaining}</span>
            </span>
            {code.showNext && (
              <span className="totp-next">
                <span>
                  {t('Nächster:')} <b>{spacedCode(code.next)}</b>
                </span>
                <CopyButton id={id} field="totp-next" />
              </span>
            )}
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
      <Button
        variant="ghost"
        size="sm"
        icon={ICONS.history}
        className="history-toggle text-muted!"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
      >
        {t('Frühere Passwörter ({n})', { n: entries.length })}
        <Icon
          icon={ICONS.expand}
          size="xs"
          className={open ? 'transition-transform' : '-rotate-90 transition-transform'}
        />
      </Button>
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

/** The first website; the others behind "+2 weitere Websites". */
function Websites({ id, uris }: { id: string; uris: NonNullable<ItemDetail['login']>['uris'] }) {
  const [all, setAll] = useState(false);
  const more = uris.length - 1;
  return (
    <section className="detail-card">
      <h3 className="detail-card-title">{uris.length === 1 ? t('Website') : t('Websites')}</h3>
      {(all ? uris : uris.slice(0, 1)).map((uri, index) => (
        <div className="detail-row" key={index}>
          <span className="detail-text">
            <span className="detail-value uri">{uri.uri}</span>
          </span>
          <span className="detail-actions">
            {uri.openable && (
              <IconButton
                icon={ICONS.openExternal}
                size="sm"
                label={t('Öffnen')}
                onClick={() => void openItemUri(id, index)}
              />
            )}
            <CopyButton id={id} field={`uri:${index}`} />
          </span>
        </div>
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
            <span className="more-hosts">
              {uris
                .slice(1, 4)
                .map((uri) => uri.host ?? uri.uri)
                .join(', ') + (more > 3 ? ', …' : '')}
            </span>
          )}
          <Icon
            icon={ICONS.expand}
            size="xs"
            className={all ? 'rotate-180 transition-transform' : 'transition-transform'}
          />
        </button>
      )}
    </section>
  );
}

/**
 * A login's passkeys: site, user, since when; deleting one asks first. The site is the RP id —
 * what the browser checked — with the name the site gave itself only next to it (R4-6).
 */
function Passkeys({
  id,
  revision,
  deleted,
}: {
  id: string;
  revision: string | null;
  deleted: boolean;
}) {
  const [keys, setKeys] = useState<PasskeyInfo[] | null>(null);
  const [busy, setBusy] = useState(false);
  // Fetched again after every delete: the places of the others shift.
  const [loads, setLoads] = useState(0);
  useEffect(() => {
    let alive = true;
    itemPasskeys(id)
      .then((next) => alive && setKeys(next))
      .catch(() => alive && setKeys([]));
    return () => {
      alive = false;
    };
  }, [id, revision, loads]);
  if (!keys || keys.length === 0) return null;

  const remove = async (key: PasskeyInfo) => {
    const site = siteOf(key);
    if (
      !window.confirm(
        t('Den Passkey für {site} löschen? Damit meldest du dich dort danach nicht mehr an.', {
          site,
        }),
      )
    )
      return;
    setBusy(true);
    try {
      await deletePasskey(id, key.index, key.credentialId || key.fingerprint);
      setKeys(null);
      setLoads((n) => n + 1);
      toast(t('Passkey gelöscht.'));
      playNyu('trashed');
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="detail-card">
      <h3 className="detail-card-title">{keys.length === 1 ? t('Passkey') : t('Passkeys')}</h3>
      {keys.map((key) => {
        const site = key.rpId || t('Unbekannte Website');
        const named = key.rpName?.trim();
        const created = when(key.creationDate);
        return (
          <div className="detail-row passkey-row" key={`${key.index}-${key.credentialId}`}>
            <span className="detail-text">
              <span className="detail-label">{site}</span>
              <span className="detail-value">
                <span>{key.userName || key.userDisplayName || t('ohne Benutzernamen')}</span>
                <span className="passkey-meta">
                  {[
                    named && named.toLowerCase() !== key.rpId.toLowerCase()
                      ? t('nennt sich „{name}“', { name: named })
                      : null,
                    created && t('erstellt {when}', { when: created }),
                    !key.readable && t('lässt sich nicht lesen – nur löschen'),
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                </span>
              </span>
            </span>
            {!deleted && (
              <span className="detail-actions">
                <IconButton
                  icon={ICONS.delete}
                  size="sm"
                  label={t('Passkey für {site} löschen', { site })}
                  disabled={busy || (!key.credentialId && !key.fingerprint)}
                  onClick={() => void remove(key)}
                />
              </span>
            )}
          </div>
        );
      })}
    </section>
  );
}

/** A passkey's site for the delete question: the RP id, and the name the site gave itself. */
function siteOf(key: PasskeyInfo): string {
  const named = key.rpName?.trim();
  if (!key.rpId) return named || t('Unbekannte Website');
  return named && named.toLowerCase() !== key.rpId.toLowerCase()
    ? `${key.rpId} („${named}“)`
    : key.rpId;
}
