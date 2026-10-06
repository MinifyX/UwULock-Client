/**
 * Masked addresses: e-mail addresses from UwUMail that forward to the real
 * one, one per site. Here they are listed, switched off and on, deleted and
 * made; connecting the account to UwUMail happens in the web vault.
 */

import { Button, Icon, IconButton, ICONS } from '@uwusuite/design';
import { useCallback, useEffect, useState } from 'react';
import { copyGenerated, type ItemSummary } from '../lib/api';
import { errorText, toastError } from '../lib/errors';
import { when } from '../lib/format';
import { t, useLanguage } from '../lib/i18n';
import { getSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import {
  createMaskedAddress,
  deleteMaskedAddress,
  maskedAddresses,
  maskedConnection,
  openWebVaultAt,
  setMaskedState,
  type MaskedAddress,
  type MaskedConnection,
} from '../lib/uwu';
import { Modal } from './Modal';
import { NyuBusy } from './nyu/stage';

export async function copyAddress(email: string) {
  try {
    await copyGenerated(email);
    const seconds = getSettings().clipboardClear;
    toast(
      seconds > 0
        ? t('Adresse kopiert ✧ – wird nach {n} s geleert', { n: seconds })
        : t('Adresse kopiert ✧'),
    );
  } catch (e) {
    toastError(e);
  }
}

/** What to do when the account isn't connected to UwUMail, or no longer is. */
export function MaskedNotConnected({ connection }: { connection: MaskedConnection | null }) {
  useLanguage();
  const revoked = connection?.status === 'revoked';
  return (
    <div className="extras-form">
      <p className="dialog-lead">
        {revoked
          ? t('UwUMail hat die Verbindung zu deinem Konto beendet. Verbinde es im Web-Tresor neu.')
          : t(
              'Maskierte Adressen kommen von UwUMail: für jede Website eine eigene Adresse, die an dein Postfach weiterleitet. Verbinde dein Konto einmal im Web-Tresor mit UwUMail, dann kannst du sie hier anlegen.',
            )}
      </p>
      <p>
        <button
          type="button"
          className="link-button"
          onClick={() => void openWebVaultAt('masked').catch((e) => toastError(e))}
        >
          {t('Im Web-Tresor verbinden')}
          <Icon icon={ICONS.openExternal} size="xs" />
        </button>
      </p>
    </div>
  );
}

export function MaskedDialog({
  items,
  onClose,
  onOpenItem,
}: {
  items: ItemSummary[];
  onClose: () => void;
  onOpenItem: (id: string) => void;
}) {
  useLanguage();
  const [connection, setConnection] = useState<MaskedConnection | null>(null);
  const [addresses, setAddresses] = useState<MaskedAddress[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [site, setSite] = useState('');
  const [description, setDescription] = useState('');
  const [busy, setBusy] = useState(false);
  const [deleting, setDeleting] = useState<MaskedAddress | null>(null);
  const [query, setQuery] = useState('');

  const load = useCallback(async () => {
    try {
      const next = await maskedConnection();
      setConnection(next);
      if (next.connected && next.status !== 'revoked') setAddresses(await maskedAddresses());
      setError(null);
    } catch (e) {
      setError(errorText(e));
      setAddresses([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const act = async (what: () => Promise<unknown>, done?: string) => {
    setBusy(true);
    try {
      await what();
      if (done) toast(done);
      await load();
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  };

  const connected = connection?.connected && connection.status !== 'revoked';
  const words = query.trim().toLowerCase();
  const shown = (addresses ?? [])
    .filter((a) =>
      words
        ? `${a.email} ${a.forDomain ?? ''} ${a.description ?? ''} ${a.itemName ?? ''}`
            .toLowerCase()
            .includes(words)
        : true,
    )
    .sort((a, b) => (b.createdAt ?? '').localeCompare(a.createdAt ?? ''));

  return (
    <Modal
      title={t('Maskierte Adressen')}
      size="wide"
      onCancel={onClose}
      footer={
        <>
          <span className="spacer" />
          <Button onClick={onClose}>{t('Schließen')}</Button>
        </>
      }
    >
      <div className="extras-scroll">
        {error && (
          <p className="notice" data-tone="error">
            {error}
          </p>
        )}
        {connection === null && !error && <NyuBusy label={t('Einen Moment …')} />}
        {connection && !connected && <MaskedNotConnected connection={connection} />}
        {connected && (
          <div className="extras-form">
            <p className="muted small">
              {t('Verbunden mit {server} als {user}.', {
                server: connection?.server?.replace(/^https?:\/\//, '') ?? '',
                user: connection?.username ?? '',
              })}
              {connection?.status === 'unreachable' &&
                ` ${t('UwUMail war zuletzt nicht erreichbar.')}`}
            </p>
            <form
              className="editor-row phone:flex-col phone:items-stretch"
              onSubmit={(event) => {
                event.preventDefault();
                void act(async () => {
                  const made = await createMaskedAddress(site || null, description || null, null);
                  setSite('');
                  setDescription('');
                  await copyAddress(made.email);
                });
              }}
            >
              <label className="field">
                <span>{t('Für Website')}</span>
                <input
                  type="text"
                  value={site}
                  spellCheck={false}
                  placeholder="shop.example.com"
                  onChange={(e) => setSite(e.target.value)}
                />
              </label>
              <label className="field">
                <span>{t('Beschreibung')}</span>
                <input
                  type="text"
                  value={description}
                  maxLength={200}
                  onChange={(e) => setDescription(e.target.value)}
                />
              </label>
              <Button type="submit" variant="primary" icon={ICONS.add} disabled={busy}>
                {t('Neue Adresse')}
              </Button>
            </form>
            {(addresses?.length ?? 0) > 6 && (
              <input
                type="search"
                className="search"
                value={query}
                placeholder={t('Adressen durchsuchen')}
                aria-label={t('Adressen durchsuchen')}
                onChange={(e) => setQuery(e.target.value)}
              />
            )}
            {addresses?.length === 0 && <p className="muted">{t('Noch keine Adressen.')}</p>}
            <ul className="extras-list">
              {shown.map((address) => {
                const item = address.cipherId
                  ? items.find((i) => i.id === address.cipherId)
                  : undefined;
                const on = address.state === 'enabled' || address.state === 'pending';
                return (
                  <li
                    key={address.id}
                    className="extras-row phone:flex-wrap"
                    data-off={!on || undefined}
                  >
                    <Icon icon={ICONS.maskedAddress} size="sm" className="shrink-0 text-muted" />
                    <span className="extras-row-text phone:basis-[calc(100%-2.5rem)]">
                      <span className="item-name mono">{address.email}</span>
                      <span className="item-sub">
                        {[
                          address.description || address.forDomain,
                          address.lastMessageAt
                            ? t('letzte Mail {when}', { when: when(address.lastMessageAt) ?? '' })
                            : null,
                          on ? null : t('abgeschaltet'),
                        ]
                          .filter(Boolean)
                          .join(' · ')}
                      </span>
                      {item && (
                        <button
                          type="button"
                          className="link-button small"
                          onClick={() => onOpenItem(item.id)}
                        >
                          <Icon icon={ICONS.link} size="xs" />
                          {item.name || t('(ohne Namen)')}
                        </button>
                      )}
                    </span>
                    <IconButton
                      icon={ICONS.copy}
                      label={t('{email} kopieren', { email: address.email })}
                      size="sm"
                      className="phone:ml-auto"
                      onClick={() => void copyAddress(address.email)}
                    />
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={busy}
                      onClick={() =>
                        void act(
                          () => setMaskedState(address.id, on ? 'disabled' : 'enabled'),
                          on ? t('Abgeschaltet.') : t('Wieder an ✧'),
                        )
                      }
                    >
                      {on ? t('Abschalten') : t('Einschalten')}
                    </Button>
                    <IconButton
                      icon={ICONS.delete}
                      label={t('{email} löschen', { email: address.email })}
                      size="sm"
                      disabled={busy}
                      onClick={() => setDeleting(address)}
                    />
                  </li>
                );
              })}
            </ul>
          </div>
        )}
      </div>
      {deleting && (
        <Modal
          title={t('Adresse löschen?')}
          tone="warning"
          size="small"
          onCancel={() => setDeleting(null)}
          footer={
            <>
              <span className="spacer" />
              <Button
                variant="danger"
                data-secondary
                onClick={() => {
                  const id = deleting.id;
                  setDeleting(null);
                  void act(() => deleteMaskedAddress(id), t('Gelöscht.'));
                }}
              >
                {t('Löschen')}
              </Button>
              <Button variant="primary" data-autofocus onClick={() => setDeleting(null)}>
                {t('Abbrechen')}
              </Button>
            </>
          }
        >
          <p className="dialog-lead">
            {t(
              '{email} nimmt dann für immer keine Mail mehr an und wird nie wieder vergeben. Zum Pausieren reicht „Abschalten“.',
              { email: deleting.email },
            )}
          </p>
        </Modal>
      )}
    </Modal>
  );
}
