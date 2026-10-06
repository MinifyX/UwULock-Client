/**
 * One item as a grouped list: tapping a value copies it (through Rust, which
 * clears the clipboard later), the eye shows a secret, the one-time code
 * counts down. Below: websites, passkeys, notes, own fields, the password
 * history, the reminder and the versions, sharing as a Send, the trash.
 */

import { haptic, ICONS, ListRow, ListSection, NavButton } from '@uwusuite/design';
import { useEffect, useState } from 'react';
import {
  deleteItem,
  deletePasskey,
  failure,
  itemPasskeys,
  openItemUri,
  openWebVault,
  restoreItem,
  revealField,
  totpCode,
  vaultItem,
  verifyReprompt,
  type ItemDetail as Detail,
  type ItemSummary,
  type PasskeyInfo,
  type TotpCode,
} from '../../lib/api';
import { errorText, toastError } from '../../lib/errors';
import { spacedCode, when } from '../../lib/format';
import { t, useLanguage } from '../../lib/i18n';
import { IDENTITY_LABEL, KIND_LABEL, SECURITY_LABEL } from '../../lib/items';
import { platform } from '../../lib/platform';
import { sendsAvailable } from '../../lib/sends';
import { note, toast } from '../../lib/toast';
import { setMaskedState, useUwu } from '../../lib/uwu';
import { ENTERPRISE_KEYS, isEnterprise, readWifi, type WifiView } from '../../lib/wifi';
import { Colored } from '../../components/ItemDetail';
import { ReminderCard, VersionsCard } from '../../components/ItemExtras';
import { ItemTile } from '../../components/ItemTile';
import { playNyu } from '../../components/nyu/stage';
import { WifiConnect } from '../../components/WifiConnect';
import { WifiShare } from '../../components/WifiShare';
import { copyItemField, toggleFavorite, trashItem, useMobile, useNav } from '../state';
import {
  BackLayer,
  BigButton,
  Chip,
  Empty,
  FieldInput,
  Hero,
  MenuButton,
  Page,
  RowButton,
  useConfirm,
} from '../ui';
import { useItemMenu } from './ItemRow';
import { rememberOpened } from './Search';

/** A value with its name above; a tap copies it. */
function ValueRow({
  id,
  field,
  label,
  value,
  mono,
  wrap,
  trailing,
}: {
  id: string;
  field: string | null;
  label: string;
  value: React.ReactNode;
  mono?: boolean;
  wrap?: boolean;
  trailing?: React.ReactNode;
}) {
  useLanguage();
  return (
    <ListRow
      label={label}
      title={value}
      mono={mono}
      wrap={wrap}
      onCopy={field ? () => void copyItemField(id, field) : undefined}
      copyLabel={t('kopieren')}
      trailing={
        trailing ??
        (field ? (
          <RowButton
            icon={ICONS.copy}
            label={t('{label} kopieren', { label })}
            onClick={() => void copyItemField(id, field)}
          />
        ) : undefined)
      }
    />
  );
}

/** A secret: dots until the eye is tapped, then fetched from Rust; tapping copies. */
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
  // Out of sight again after a minute.
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
      haptic('light');
    } catch (e) {
      toastError(e);
    }
  };
  return (
    <ListRow
      label={label}
      mono
      wrap={multiline && value !== null}
      title={
        value === null ? (
          <span className="m-masked">{masked}</span>
        ) : multiline ? (
          <pre className="m-secret-block">{value}</pre>
        ) : colored ? (
          <Colored text={value} />
        ) : (
          value
        )
      }
      onCopy={() => void copyItemField(id, field)}
      trailing={
        <>
          <RowButton
            icon={value === null ? ICONS.show : ICONS.hide}
            label={
              value === null ? t('{label} zeigen', { label }) : t('{label} verbergen', { label })
            }
            pressed={value !== null}
            onClick={() => void toggle()}
          />
          <RowButton
            icon={ICONS.copy}
            label={t('{label} kopieren', { label })}
            onClick={() => void copyItemField(id, field)}
          />
        </>
      }
    />
  );
}

/** The current one-time code with its countdown ring; a tap copies it. */
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
  return (
    <>
      <ListRow
        label={t('Einmal-Code (TOTP)')}
        title={
          error ? (
            <span className="m-error-inline">{error}</span>
          ) : (
            <span className="m-code">{code ? spacedCode(code.code) : '…'}</span>
          )
        }
        onCopy={() => void copyItemField(id, 'totp')}
        trailing={
          code ? (
            <span
              className="m-ring"
              data-soon={code.remaining <= 5 || undefined}
              style={{ '--p': code.remaining / code.period } as React.CSSProperties}
              aria-label={t('noch {n} s', { n: code.remaining })}
            >
              <span>{code.remaining}</span>
            </span>
          ) : undefined
        }
      />
      {code?.showNext && (
        <ListRow
          label={t('Nächster:')}
          title={<span className="m-code">{spacedCode(code.next)}</span>}
          onCopy={() => void copyItemField(id, 'totp-next')}
        />
      )}
    </>
  );
}

/** The master password again, before an item with re-prompt shows anything. */
function Reprompt({ id, onPassed }: { id: string; onPassed: () => void }) {
  useLanguage();
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await verifyReprompt(id, password);
      onPassed();
    } catch (e) {
      setError(errorText(e));
      setPassword('');
      haptic('error');
    } finally {
      setBusy(false);
    }
  };
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <ListSection
        header={t('Master-Passwort erforderlich')}
        footer={t(
          'Dieser Eintrag ist besonders geschützt. Gib dein Master-Passwort ein, um ihn zu öffnen.',
        )}
      >
        <FieldInput
          label={t('Master-Passwort')}
          type="password"
          value={password}
          onChange={setPassword}
          autoComplete="current-password"
          autoFocus
          disabled={busy}
        />
      </ListSection>
      {error && (
        <p className="m-error" role="alert">
          {error}
        </p>
      )}
      <div className="m-buttons">
        <BigButton icon={ICONS.unlocked} disabled={busy || !password} onClick={() => void submit()}>
          {busy ? t('Prüft …') : t('Öffnen')}
        </BigButton>
      </div>
    </form>
  );
}

/** A login's passkeys, each with delete. */
function PasskeysSection({ id, revision }: { id: string; revision: string | null }) {
  useLanguage();
  const [keys, setKeys] = useState<PasskeyInfo[] | null>(null);
  const confirm = useConfirm();
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
    try {
      await deletePasskey(id, key.index, key.credentialId || key.fingerprint);
      setKeys(null);
      setKeys(await itemPasskeys(id));
      note(t('Passkey gelöscht.'));
      playNyu('trashed');
    } catch (e) {
      toastError(e);
    }
  };
  return (
    <ListSection header={keys.length === 1 ? t('Passkey') : t('Passkeys')}>
      {keys.map((key) => {
        const site = key.rpId || key.rpName || t('Unbekannte Website');
        const user = key.userName || key.userDisplayName || t('ohne Benutzernamen');
        const created = when(key.creationDate);
        return (
          <ListRow
            key={`${key.index}-${key.credentialId}`}
            icon={ICONS.passkey}
            iconTone="success"
            title={user}
            subtitle={[site, created && t('erstellt {when}', { when: created })]
              .filter(Boolean)
              .join(' · ')}
            trailing={
              <RowButton
                icon={ICONS.delete}
                label={t('Passkey für {site} löschen', { site })}
                onClick={() =>
                  confirm.ask({
                    title: t('Passkey löschen?'),
                    text: t(
                      'Mit diesem Passkey meldest du dich danach nicht mehr bei {site} an. Leg vorher einen anderen Weg zur Anmeldung an, falls du keinen mehr hast.',
                      { site },
                    ),
                    confirm: t('Löschen'),
                    run: () => void remove(key),
                  })
                }
              />
            }
          />
        );
      })}
      {confirm.element}
    </ListSection>
  );
}

/** A Wi-Fi network's values, with the QR code (and Android's "Verbinden"). */
function WifiSection({ id, wifi }: { id: string; wifi: WifiView }) {
  useLanguage();
  const [sharing, setSharing] = useState(false);
  const enterprise: [(typeof ENTERPRISE_KEYS)[number], string][] = [
    ['eap', t('EAP-Methode')],
    ['phase2', t('Phase 2')],
    ['identity', t('Identität')],
    ['anonymous', t('Anonyme Identität')],
    ['ca', t('CA-Zertifikat')],
  ];
  return (
    <>
      <ListSection>
        <ValueRow
          id={id}
          field={wifi.ssid && wifi.from.ssid !== undefined ? `field:${wifi.from.ssid}` : null}
          label={t('Netzwerkname (SSID)')}
          value={wifi.ssid || '—'}
          mono
        />
        {wifi.password?.hasValue && wifi.security !== 'None' && (
          <SecretRow id={id} field={`field:${wifi.password.index}`} label={t('Passwort')} />
        )}
        <ListRow
          title={t('Sicherheit')}
          value={wifi.security ? t(SECURITY_LABEL[wifi.security] ?? wifi.security) : '—'}
        />
        <ListRow title={t('Verstecktes Netzwerk')} value={wifi.hidden ? t('Ja') : t('Nein')} />
        {isEnterprise(wifi.security) &&
          enterprise.map(([key, label]) => {
            const value = wifi[key];
            const from = wifi.from[key];
            return value ? (
              <ValueRow
                key={key}
                id={id}
                field={from !== undefined ? `field:${from}` : null}
                label={label}
                value={value === 'none' ? t('Keine') : value}
              />
            ) : null;
          })}
      </ListSection>
      <div className="m-gap" />
      <ListSection>
        <ListRow
          icon={ICONS.qrCode}
          title={t('Als QR-Code teilen')}
          onClick={() => setSharing(true)}
        />
      </ListSection>
      {platform() === 'android' && (
        <div className="m-desktop-block">
          <WifiConnect id={id} wifi={wifi} />
        </div>
      )}
      <BackLayer open={sharing} close={() => setSharing(false)} />
      {sharing && <WifiShare itemId={id} wifi={wifi} onClose={() => setSharing(false)} />}
    </>
  );
}

function ItemBody({ summary }: { summary: ItemSummary }) {
  useLanguage();
  const { android, data, openSheet } = useMobile();
  const nav = useNav();
  const uwu = useUwu();
  const [detail, setDetail] = useState<Detail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState(false);
  const confirm = useConfirm();
  const menu = useItemMenu(summary);
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
  // Again when the sync brought a new revision of this item.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(load, [id, summary.revisionDate]);
  useEffect(() => rememberOpened(id), [id]);

  const overview = data.overview;
  const folder = overview?.folders.find((f) => f.id === summary.folderId)?.name;
  const org = overview?.organizations.find((o) => o.id === summary.organizationId)?.name;
  const collections = (overview?.collections ?? [])
    .filter((c) => summary.collectionIds.includes(c.id))
    .map((c) => c.name);
  const name = summary.name || t('(ohne Namen)');
  const d = detail && !detail.locked ? detail : null;
  const wifi = d && summary.kind === 'wifi' ? readWifi(d.fields ?? []) : null;
  const fields = wifi ? wifi.others : (d?.fields ?? []);
  const edit = () => openSheet({ kind: 'edit', id, type: summary.kind });

  const removeForGood = () =>
    confirm.ask({
      title: t('Endgültig löschen?'),
      text: t(
        '„{name}“ wird auf dem Server gelöscht. Das lässt sich nicht rückgängig machen – auch nicht im Web-Tresor.',
        { name },
      ),
      confirm: t('Endgültig löschen'),
      run: () =>
        void (async () => {
          try {
            // Mail to an address whose item is gone should stop.
            if (masked) await setMaskedState(masked.id, 'disabled').catch((e) => toastError(e));
            await deleteItem(id, true);
            playNyu('trashed');
            note(t('Gelöscht.'));
            nav.back();
          } catch (e) {
            toastError(e);
          }
        })(),
    });

  return (
    <>
      <Page
        hero
        title={name}
        trailing={
          summary.deleted ? undefined : android ? (
            <>
              <NavButton
                label={t('Bearbeiten')}
                icon={ICONS.edit}
                disabled={summary.broken}
                onClick={edit}
              />
              <MenuButton items={menu} />
            </>
          ) : (
            <NavButton label={t('Bearbeiten')} text disabled={summary.broken} onClick={edit} />
          )
        }
      >
        <Hero tile={<ItemTile item={summary} size="large" />} title={name}>
          {folder && <Chip icon={ICONS.folder}>{folder}</Chip>}
          {org && (
            <Chip icon={ICONS.organization}>
              {org}
              {collections.length > 0 && ` · ${collections.join(', ')}`}
            </Chip>
          )}
          {!folder && !org && <Chip>{t(KIND_LABEL[summary.kind])}</Chip>}
          {!summary.deleted && (
            <Chip
              icon={ICONS.favorite}
              tone={summary.favorite ? 'fav' : undefined}
              pressed={summary.favorite}
              onClick={() => void toggleFavorite(summary)}
            >
              {summary.favorite ? t('Favorit') : t('Kein Favorit')}
            </Chip>
          )}
          {masked && <Chip icon={ICONS.maskedAddress}>{masked.email}</Chip>}
          {due && !summary.deleted && (
            <Chip icon={ICONS.reminder} tone="warn">
              {t('Neues Passwort fällig')}
            </Chip>
          )}
          {summary.deleted && <Chip tone="muted">{t('Im Papierkorb')}</Chip>}
        </Hero>

        {error && (
          <p className="m-error" role="alert">
            {error}
          </p>
        )}
        {summary.broken && (
          <p className="m-error">
            {t(
              'Ein Teil dieses Eintrags ließ sich nicht entschlüsseln. Im Web-Tresor sieht er vielleicht anders aus.',
            )}
          </p>
        )}

        {detail?.locked && <Reprompt id={id} onPassed={load} />}

        {d && (
          <>
            {d.login && (
              <ListSection>
                {d.login.username && (
                  <ValueRow
                    id={id}
                    field="username"
                    label={t('Benutzername')}
                    value={d.login.username}
                  />
                )}
                {d.login.hasPassword && (
                  <SecretRow id={id} field="password" label={t('Passwort')} />
                )}
                {d.login.hasTotp && <TotpRow id={id} />}
                {!d.login.username && !d.login.hasPassword && !d.login.hasTotp && (
                  <ListRow title={t('Kein Benutzername, kein Passwort.')} />
                )}
              </ListSection>
            )}

            {d.login && d.login.uris.length > 0 && (
              <ListSection header={d.login.uris.length === 1 ? t('Website') : t('Websites')}>
                {d.login.uris.map((uri, index) => (
                  <ListRow
                    key={index}
                    label={uri.host ?? t('Adresse')}
                    title={uri.uri}
                    onCopy={() => void copyItemField(id, `uri:${index}`)}
                    trailing={
                      uri.openable ? (
                        <RowButton
                          icon={ICONS.openExternal}
                          label={t('{uri} im Browser öffnen', { uri: uri.uri })}
                          onClick={() =>
                            void openItemUri(id, index).catch((e) => toast(String(e), 'error'))
                          }
                        />
                      ) : undefined
                    }
                  />
                ))}
              </ListSection>
            )}

            {d.login && d.login.passkeys > 0 && (
              <PasskeysSection id={id} revision={summary.revisionDate} />
            )}

            {d.card && (
              <ListSection>
                {d.card.cardholderName && (
                  <ValueRow
                    id={id}
                    field="card-name"
                    label={t('Karteninhaber')}
                    value={d.card.cardholderName}
                  />
                )}
                {d.card.brand && <ListRow label={t('Marke')} title={d.card.brand} />}
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
                  <ValueRow
                    id={id}
                    field="card-expiry"
                    label={t('Gültig bis')}
                    mono
                    value={`${(d.card.expMonth ?? '').padStart(2, '0')}/${d.card.expYear ?? ''}`}
                  />
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
              </ListSection>
            )}

            {d.identity && d.identity.length > 0 && (
              <ListSection>
                {d.identity.map((entry) => {
                  const label = t(IDENTITY_LABEL[entry.name] ?? entry.name);
                  return entry.sensitive ? (
                    <SecretRow
                      key={entry.name}
                      id={id}
                      field={`identity:${entry.name}`}
                      label={label}
                      colored={false}
                    />
                  ) : (
                    <ValueRow
                      key={entry.name}
                      id={id}
                      field={`identity:${entry.name}`}
                      label={label}
                      value={entry.value}
                    />
                  );
                })}
              </ListSection>
            )}

            {d.sshKey && (
              <ListSection>
                {d.sshKey.fingerprint && (
                  <ValueRow
                    id={id}
                    field="ssh-fingerprint"
                    label={t('Fingerprint')}
                    value={d.sshKey.fingerprint}
                    mono
                  />
                )}
                {d.sshKey.publicKey && (
                  <ValueRow
                    id={id}
                    field="ssh-public"
                    label={t('Öffentlicher Schlüssel')}
                    value={d.sshKey.publicKey}
                    mono
                  />
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
              </ListSection>
            )}

            {wifi && <WifiSection id={id} wifi={wifi} />}

            {d.notes && (
              <ListSection header={t('Notizen')}>
                <ListRow
                  title={d.notes}
                  wrap
                  className="m-note-row"
                  onCopy={() => void copyItemField(id, 'notes')}
                />
              </ListSection>
            )}

            {fields.length > 0 && (
              <ListSection header={t('Eigene Felder')}>
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
                      <ListRow
                        key={field.index}
                        title={label}
                        value={field.value === 'true' ? t('Ja') : t('Nein')}
                      />
                    );
                  if (field.kind === 'linked')
                    return (
                      <ListRow
                        key={field.index}
                        label={label}
                        title={t('verknüpft mit einem anderen Feld')}
                      />
                    );
                  return (
                    <ValueRow
                      key={field.index}
                      id={id}
                      field={field.value ? `field:${field.index}` : null}
                      label={label}
                      value={field.value ?? '—'}
                    />
                  );
                })}
              </ListSection>
            )}

            {!summary.deleted && (
              <>
                <div className="m-gap" />
                <ListSection>
                  {sendsAvailable(uwu) && (
                    <ListRow
                      icon={ICONS.send}
                      iconTone="success"
                      title={t('Als Send teilen')}
                      disabled={summary.broken}
                      onClick={() => openSheet({ kind: 'share', id })}
                    />
                  )}
                  {d.passwordHistory && d.passwordHistory.length > 0 && (
                    <ListRow
                      icon={ICONS.history}
                      iconTone="neutral"
                      title={t('Passwortverlauf')}
                      value={d.passwordHistory.length}
                      chevron={false}
                      aria-expanded={history}
                      onClick={() => setHistory(!history)}
                    />
                  )}
                  {history &&
                    d.passwordHistory?.map((entry) => (
                      <SecretRow
                        key={entry.index}
                        id={id}
                        field={`history:${entry.index}`}
                        label={when(entry.lastUsed) ?? t('Früher')}
                      />
                    ))}
                </ListSection>
              </>
            )}

            <div className="m-desktop-block">
              <ReminderCard summary={summary} />
              <VersionsCard summary={summary} />
            </div>

            <p className="m-footnote">
              {[
                when(summary.revisionDate) &&
                  t('Geändert {when}', { when: when(summary.revisionDate) ?? '' }),
                when(d.creationDate) && t('Erstellt {when}', { when: when(d.creationDate) ?? '' }),
              ]
                .filter(Boolean)
                .join(' · ')}
              {(d.attachments ?? 0) > 0 && (
                <>
                  <br />
                  {t('{n} Anhänge – die öffnest du vorerst im Web-Tresor.', {
                    n: d.attachments ?? 0,
                  })}
                </>
              )}
            </p>
          </>
        )}

        <div className="m-gap" />
        <ListSection>
          <ListRow
            icon={ICONS.openExternal}
            iconTone="neutral"
            title={t('Im Web-Tresor öffnen')}
            onClick={() => void openWebVault().catch(() => undefined)}
          />
        </ListSection>
        <div className="m-gap" />
        <ListSection>
          {summary.deleted ? (
            <>
              <ListRow
                title={t('Wiederherstellen')}
                tone="accent"
                onClick={() =>
                  void restoreItem(id)
                    .then(() => {
                      haptic('success');
                      note(t('Aus dem Papierkorb geholt ✧'), { tone: 'success' });
                    })
                    .catch((e) => toastError(e))
                }
              />
              <ListRow title={t('Endgültig löschen')} tone="danger" onClick={removeForGood} />
            </>
          ) : (
            <ListRow
              title={t('In den Papierkorb')}
              tone="danger"
              onClick={() => {
                void trashItem(summary);
                nav.back();
              }}
            />
          )}
        </ListSection>
      </Page>
      {confirm.element}
    </>
  );
}

export function ItemPage({ id }: { id: string }) {
  useLanguage();
  const { data } = useMobile();
  const summary = data.byId(id);
  if (!summary)
    return (
      <Page title="">
        <Empty title={data.loaded ? t('Gelöscht') : '…'}>
          {data.loaded && t('Den Eintrag gibt es nicht mehr.')}
        </Empty>
      </Page>
    );
  return <ItemBody summary={summary} />;
}
