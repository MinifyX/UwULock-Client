/**
 * Masked addresses on a phone or iPad: UwUMail addresses that forward to the
 * real one, one per site. Tapping one copies it, its switch turns it off and
 * on, swiping or a long press deletes it; "+" makes a new one. Connecting the
 * account to UwUMail happens in the web vault. The logic is the desktop's
 * `MaskedDialog`.
 */

import {
  ContextMenu,
  Fab,
  haptic,
  ICONS,
  ListRow,
  ListSection,
  NavButton,
  SwipeRow,
  useLongPress,
  type ContextMenuEntry,
} from '@uwusuite/design';
import { useState } from 'react';
import { useBackLayer } from '../../lib/backStack';
import { errorText, toastError } from '../../lib/errors';
import { when } from '../../lib/format';
import { t, useLanguage } from '../../lib/i18n';
import { note } from '../../lib/toast';
import {
  createMaskedAddress,
  deleteMaskedAddress,
  has,
  maskedAddresses,
  maskedConnection,
  openWebVaultAt,
  setMaskedState,
  useUwu,
  type MaskedAddress,
  type MaskedConnection,
} from '../../lib/uwu';
import { copyAddress } from '../../components/MaskedDialog';
import { useMobile, useNav } from '../state';
import { BigButton, EditSurface, Empty, FieldInput, Page, Toggle, useConfirm } from '../ui';
import { listStore, useEditing, useListStore } from './Sends';

type Masked = { connection: MaskedConnection; addresses: MaskedAddress[] };

export const maskedStore = listStore<Masked>(async () => {
  const connection = await maskedConnection();
  const connected = connection.connected && connection.status !== 'revoked';
  return { connection, addresses: connected ? await maskedAddresses() : [] };
});

const isOn = (address: MaskedAddress) => address.state === 'enabled' || address.state === 'pending';

/** Where the mails go, when UwUMail's user name is an address. */
function forwardedTo(connection: MaskedConnection | undefined): string | undefined {
  if (!connection?.username) return undefined;
  if (connection.username.includes('@'))
    return t('Weitergeleitet an {email}', { email: connection.username });
  return t('Verbunden mit {server} als {user}.', {
    server: connection.server?.replace(/^https?:\/\//, '') ?? '',
    user: connection.username,
  });
}

export function MaskedPage() {
  useLanguage();
  const { ios, ipad, android } = useMobile();
  const uwu = useUwu();
  const { value, error } = useListStore(maskedStore);
  const editor = useEditing<null>();
  const confirm = useConfirm();
  const [busy, setBusy] = useState(false);
  const [query, setQuery] = useState('');

  const connection = value?.connection;
  const connected = Boolean(connection?.connected && connection.status !== 'revoked');
  const available = has(uwu, 'masked-addresses');

  const act = async (what: () => Promise<unknown>, done?: string, detail?: string) => {
    setBusy(true);
    try {
      await what();
      if (done) {
        haptic('success');
        note(done, { tone: 'success', detail });
      }
      await maskedStore.load();
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  };

  const toggle = (address: MaskedAddress) => {
    const on = isOn(address);
    void act(
      () => setMaskedState(address.id, on ? 'disabled' : 'enabled'),
      on ? t('Abgeschaltet.') : t('Wieder an ✧'),
      address.email,
    );
  };

  const remove = (address: MaskedAddress) =>
    confirm.ask({
      title: t('Adresse löschen?'),
      text: t(
        '{email} nimmt dann für immer keine Mail mehr an und wird nie wieder vergeben. Zum Pausieren reicht „Abschalten“.',
        { email: address.email },
      ),
      confirm: t('Löschen'),
      run: () => void act(() => deleteMaskedAddress(address.id), t('Gelöscht.')),
    });

  const words = query.trim().toLowerCase();
  const addresses = value?.addresses ?? [];
  const shown = addresses
    .filter((a) =>
      words
        ? `${a.email} ${a.forDomain ?? ''} ${a.description ?? ''} ${a.itemName ?? ''}`
            .toLowerCase()
            .includes(words)
        : true,
    )
    .sort((a, b) => (b.createdAt ?? '').localeCompare(a.createdAt ?? ''));

  const add = () => editor.open(null);
  const canAdd = available && connected;

  return (
    <>
      <Page
        title={t('Maskierte Adressen')}
        largeTitle
        subtitle={connected ? forwardedTo(connection) : undefined}
        onRefresh={available ? () => maskedStore.load() : undefined}
        trailing={
          canAdd &&
          (ios || ipad) && <NavButton label={t('Neue Adresse')} icon={ICONS.add} onClick={add} />
        }
      >
        {!available ? (
          <Empty title={t('Keine maskierten Adressen')}>
            {t('Dieser Server bietet keine maskierten Adressen an.')}
          </Empty>
        ) : (
          <>
            {error && <p className="m-error">{error}</p>}
            {value && !connected && <NotConnected connection={value.connection} />}
            {connected && (
              <>
                {connection?.status === 'unreachable' && (
                  <p className="m-error">{t('UwUMail war zuletzt nicht erreichbar.')}</p>
                )}
                {addresses.length > 6 && (
                  <label className="m-search-field" data-uwu-field="">
                    <ICONS.search aria-hidden />
                    <input
                      type="search"
                      value={query}
                      placeholder={t('Adressen durchsuchen')}
                      aria-label={t('Adressen durchsuchen')}
                      autoCapitalize="off"
                      autoCorrect="off"
                      spellCheck={false}
                      onChange={(e) => setQuery(e.target.value)}
                    />
                  </label>
                )}
                <ListSection
                  footer={t(
                    'Antippen kopiert die Adresse. Abgeschaltete Adressen nehmen keine Mails mehr an.',
                  )}
                >
                  {addresses.length === 0 ? (
                    <Empty title={t('Noch keine Adressen.')}>
                      {t('Tipp auf + für eine neue.')}
                    </Empty>
                  ) : (
                    shown.map((address) => (
                      <AddressRow
                        key={address.id}
                        address={address}
                        busy={busy}
                        onToggle={() => toggle(address)}
                        onDelete={() => remove(address)}
                      />
                    ))
                  )}
                </ListSection>
              </>
            )}
          </>
        )}
      </Page>
      {android && canAdd && <Fab label={t('Neue Adresse')} icon={ICONS.add} onClick={add} />}
      {confirm.element}
      {editor.editing && (
        <NewAddress
          key={editor.editing.n}
          open={editor.editing.open}
          to={connection?.username?.includes('@') ? connection.username : null}
          onClose={editor.close}
        />
      )}
    </>
  );
}

function AddressRow({
  address,
  busy,
  onToggle,
  onDelete,
}: {
  address: MaskedAddress;
  busy: boolean;
  onToggle: () => void;
  onDelete: () => void;
}) {
  useLanguage();
  const { data } = useMobile();
  const nav = useNav();
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const longPress = useLongPress((point) => setMenu({ x: point.x, y: point.y }));
  useBackLayer(menu !== null, () => setMenu(null));
  const on = isOn(address);
  const item = address.cipherId ? data.byId(address.cipherId) : undefined;

  const entries: ContextMenuEntry[] = [
    {
      label: t('Kopieren'),
      icon: ICONS.copy,
      onSelect: () => void copyAddress(address.email),
    },
    {
      label: on ? t('Abschalten') : t('Einschalten'),
      icon: on ? ICONS.notificationsOff : ICONS.notifications,
      onSelect: onToggle,
    },
    ...(item
      ? [
          {
            label: item.name || t('(ohne Namen)'),
            icon: ICONS.link,
            onSelect: () => nav.open({ page: 'item', id: item.id }),
          },
        ]
      : []),
    'separator',
    { label: t('Löschen'), icon: ICONS.delete, danger: true, onSelect: onDelete },
  ];

  const subtitle = [
    address.description,
    address.forDomain,
    item ? item.name || t('(ohne Namen)') : null,
    address.lastMessageAt
      ? t('letzte Mail {when}', { when: when(address.lastMessageAt) ?? '' })
      : null,
    on ? null : t('abgeschaltet'),
  ]
    .filter(Boolean)
    .join(' · ');

  return (
    <>
      <SwipeRow
        trailing={[
          {
            label: t('Löschen'),
            icon: ICONS.delete,
            tone: 'danger',
            onSelect: onDelete,
          },
        ]}
      >
        <ListRow
          className={on ? undefined : 'm-off'}
          title={address.email}
          mono
          subtitle={subtitle || undefined}
          trailing={
            <Toggle
              label={t('{email} an', { email: address.email })}
              checked={on}
              disabled={busy}
              onChange={onToggle}
            />
          }
          onCopy={() => void copyAddress(address.email)}
          longPress={longPress}
        />
      </SwipeRow>
      <ContextMenu
        open={menu !== null}
        onClose={() => setMenu(null)}
        at={menu ?? undefined}
        label={t('Aktionen für {name}', { name: address.email })}
        items={entries}
      />
    </>
  );
}

/** The account isn't connected to UwUMail, or no longer is (the desktop's `MaskedNotConnected`). */
function NotConnected({ connection }: { connection: MaskedConnection }) {
  useLanguage();
  const revoked = connection.status === 'revoked';
  return (
    <>
      <Empty title={revoked ? t('Verbindung beendet') : t('Nicht verbunden')}>
        {revoked
          ? t('UwUMail hat die Verbindung zu deinem Konto beendet. Verbinde es im Web-Tresor neu.')
          : t(
              'Maskierte Adressen kommen von UwUMail: für jede Website eine eigene Adresse, die an dein Postfach weiterleitet. Verbinde dein Konto einmal im Web-Tresor mit UwUMail, dann kannst du sie hier anlegen.',
            )}
      </Empty>
      <div className="m-buttons">
        <BigButton
          icon={ICONS.openExternal}
          onClick={() => void openWebVaultAt('masked').catch((e) => toastError(e))}
        >
          {t('Im Web-Tresor verbinden')}
        </BigButton>
      </div>
    </>
  );
}

function NewAddress({
  open,
  to,
  onClose,
}: {
  open: boolean;
  /** Where the mails go, when known. */
  to: string | null;
  onClose: () => void;
}) {
  useLanguage();
  const [site, setSite] = useState('');
  const [description, setDescription] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      const made = await createMaskedAddress(site.trim() || null, description.trim() || null, null);
      await copyAddress(made.email);
      haptic('success');
      await maskedStore.load();
      onClose();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <EditSurface
      open={open}
      onClose={onClose}
      title={t('Neue maskierte Adresse')}
      dirty={Boolean(site || description)}
      action={{
        label: busy ? t('Erstellt …') : t('Erstellen'),
        onClick: () => void create(),
        disabled: busy,
      }}
    >
      {error && (
        <p className="m-error" role="alert">
          {error}
        </p>
      )}
      <ListSection
        footer={
          to
            ? t(
                'Mails an die Adresse landen bei {email}. UwUMail erstellt sie, du kannst sie jederzeit abschalten.',
                { email: to },
              )
            : t('UwUMail erstellt die Adresse, du kannst sie jederzeit abschalten.')
        }
      >
        <FieldInput
          label={t('Für Website')}
          value={site}
          inputMode="url"
          placeholder="shop.example.com"
          onChange={setSite}
        />
        <FieldInput
          label={t('Beschreibung')}
          value={description}
          maxLength={200}
          placeholder={t('Wofür ist die Adresse?')}
          onChange={setDescription}
        />
      </ListSection>
    </EditSurface>
  );
}
