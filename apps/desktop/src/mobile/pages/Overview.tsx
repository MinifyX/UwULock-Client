/**
 * The Tresor tab's first page, instead of the desktop's sidebar: all and due
 * items, favourites, types, folders, organisations, the UwU-Apps spaces and
 * the extras — each a row that opens its list. On an iPad the same choices
 * are the split view's sidebar ({@link VaultSidebar}).
 */

import {
  ContextMenu,
  Fab,
  haptic,
  ICONS,
  ListRow,
  ListSection,
  NavButton,
  SearchBar,
  SidebarHeading,
  SidebarRow,
  useLongPress,
} from '@uwusuite/design';
import type { LucideIcon } from 'lucide-react';
import { useState } from 'react';
import { deleteFolder, type Folder, type ItemKind } from '../../lib/api';
import { useBackLayer } from '../../lib/backStack';
import { toastError } from '../../lib/errors';
import { sameFilter, type Filter } from '../../lib/filters';
import { ago } from '../../lib/format';
import { N_, t, useLanguage } from '../../lib/i18n';
import { sendsAvailable } from '../../lib/sends';
import { useSettings } from '../../lib/settings';
import { note } from '../../lib/toast';
import { has, useUwu } from '../../lib/uwu';
import { initialOf } from '../../components/AccountCard';
import { SPACE_TITLE } from '../../components/SuitePane';
import { TravelDialog, travelLabel, useTravel } from '../../components/TravelBadge';
import type { Route } from '../nav';
import { useMobile, useNav } from '../state';
import { Page, useConfirm } from '../ui';
import { ItemRow } from './ItemRow';

export const TYPES: { type: ItemKind; label: string; icon: LucideIcon }[] = [
  { type: 'login', label: N_('Logins'), icon: ICONS.website },
  { type: 'card', label: N_('Karten'), icon: ICONS.card },
  { type: 'identity', label: N_('Identitäten'), icon: ICONS.identity },
  { type: 'note', label: N_('Notizen'), icon: ICONS.note },
  { type: 'ssh-key', label: N_('SSH-Schlüssel'), icon: ICONS.sshKey },
  { type: 'wifi', label: N_('WLAN'), icon: ICONS.wifi },
];

/** The account's round initial; it opens the account switcher. */
export function AvatarButton() {
  useLanguage();
  const { status, openSheet } = useMobile();
  const initial = initialOf({
    name: status.name,
    label: status.label ?? undefined,
    email: status.email ?? '?',
  });
  return (
    <button
      type="button"
      className="m-avatar-button"
      aria-label={t('Konto wechseln')}
      onClick={() => openSheet({ kind: 'accounts' })}
    >
      {initial}
    </button>
  );
}

/** "Synchronisiert vor 2 Min.", or what the sync is doing. */
export function SyncLine() {
  useLanguage();
  const { status } = useMobile();
  if (status.syncing)
    return (
      <>
        <ICONS.refresh aria-hidden width={15} height={15} />
        {t('Synchronisiert gerade …')}
      </>
    );
  if (status.syncError)
    return (
      <>
        <ICONS.error aria-hidden width={15} height={15} />
        {t('Sync fehlgeschlagen')}
      </>
    );
  return (
    <>
      <ICONS.success aria-hidden width={15} height={15} />
      {t('Synchronisiert {when}', { when: ago(status.lastSync) })}
    </>
  );
}

/** Which rows the overview and the sidebar have, in their order. */
function useVaultChoices() {
  const { data } = useMobile();
  const uwu = useUwu();
  const settings = useSettings();
  const { counts, overview } = data;
  const folders = [...(overview?.folders ?? [])].sort((a, b) => a.name.localeCompare(b.name));
  return {
    counts,
    folders,
    organizations: overview?.organizations ?? [],
    collections: overview?.collections ?? [],
    reminders: has(uwu, 'reminders'),
    suite: has(uwu, 'suite'),
    sends: sendsAvailable(uwu),
    requests: has(uwu, 'file-requests'),
    masked: has(uwu, 'masked-addresses'),
    unseenUploads: uwu.unseen.fileRequestSubmissions,
    trash: settings.showTrash && counts.trash > 0,
  };
}

/** A folder's row, with rename and delete on a long press. */
function FolderRow({ folder, count }: { folder: Folder; count: number }) {
  useLanguage();
  const nav = useNav();
  const { openSheet } = useMobile();
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const longPress = useLongPress((point) => setMenu({ x: point.x, y: point.y }));
  useBackLayer(menu !== null, () => setMenu(null));
  const confirm = useConfirm();
  return (
    <>
      <ListRow
        icon={ICONS.folder}
        iconTone="neutral"
        title={folder.name}
        value={count}
        onClick={() => nav.open({ page: 'list', filter: { kind: 'folder', id: folder.id } })}
        longPress={longPress}
      />
      <ContextMenu
        open={menu !== null}
        onClose={() => setMenu(null)}
        at={menu ?? undefined}
        label={folder.name}
        items={[
          {
            label: t('Umbenennen'),
            icon: ICONS.edit,
            onSelect: () => openSheet({ kind: 'folder', id: folder.id, name: folder.name }),
          },
          'separator',
          {
            label: t('Löschen'),
            icon: ICONS.delete,
            danger: true,
            onSelect: () =>
              confirm.ask({
                title: t('Ordner löschen?'),
                text: t('Die Einträge darin bleiben, nur ohne Ordner.'),
                confirm: t('Löschen'),
                run: () =>
                  void deleteFolder(folder.id)
                    .then(() => {
                      haptic('warning');
                      note(t('Ordner gelöscht.'));
                    })
                    .catch((e) => toastError(e)),
              }),
          },
        ]}
      />
      {confirm.element}
    </>
  );
}

export function OverviewPage() {
  useLanguage();
  const mobile = useMobile();
  const { android, data, openSheet, sync, setSearchOpen } = mobile;
  const nav = useNav();
  const choices = useVaultChoices();
  const travel = useTravel();
  const [travelOpen, setTravelOpen] = useState(false);
  useBackLayer(travelOpen, () => setTravelOpen(false));
  const { counts } = choices;
  const open = (filter: Filter) => nav.open({ page: 'list', filter });
  const favorites = data.items
    .filter((item) => item.favorite && !item.deleted)
    .sort((a, b) => a.name.localeCompare(b.name));
  const add = () => openSheet({ kind: 'new' });

  return (
    <>
      <Page
        title={t('Tresor')}
        largeTitle
        subtitle={<SyncLine />}
        leading={android ? undefined : <AvatarButton />}
        trailing={
          android ? undefined : (
            <NavButton label={t('Neuer Eintrag')} icon={ICONS.add} onClick={add} />
          )
        }
        onRefresh={sync}
        searchBar={
          android ? (
            <SearchBar
              placeholder={t('Im Tresor suchen')}
              onActivate={() => setSearchOpen(true)}
              trailing={<AvatarButton />}
            />
          ) : undefined
        }
      >
        <ListSection>
          <ListRow
            icon={ICONS.vault}
            title={t('Alle Einträge')}
            value={counts.all}
            onClick={() => open({ kind: 'all' })}
          />
          {choices.reminders && (
            <ListRow
              icon={ICONS.reminder}
              iconTone="warning"
              title={t('Neues Passwort fällig')}
              value={counts.due}
              onClick={() => open({ kind: 'due' })}
            />
          )}
          {travel.enabled && (
            <ListRow
              icon={ICONS.travelMode}
              iconTone="warning"
              title={travelLabel(travel.hidden)}
              onClick={() => setTravelOpen(true)}
            />
          )}
        </ListSection>

        <ListSection header={t('Favoriten')}>
          {favorites.length ? (
            favorites.map((item) => <ItemRow key={item.id} item={item} />)
          ) : (
            <ListRow title={t('Noch keine Favoriten')} className="m-muted-row" />
          )}
        </ListSection>

        <ListSection header={t('Typen')}>
          {TYPES.map((type) => (
            <ListRow
              key={type.type}
              icon={type.icon}
              title={t(type.label)}
              value={counts.type(type.type)}
              onClick={() => open({ kind: 'type', type: type.type })}
            />
          ))}
        </ListSection>

        <ListSection
          header={t('Ordner')}
          headerAction={
            <button
              type="button"
              className="m-list-action"
              aria-label={t('Neuer Ordner')}
              onClick={() => openSheet({ kind: 'folder', id: null, name: '' })}
            >
              <ICONS.newFolder aria-hidden />
            </button>
          }
        >
          {choices.folders.map((folder) => (
            <FolderRow key={folder.id} folder={folder} count={counts.folder(folder.id)} />
          ))}
          <ListRow
            icon={ICONS.folder}
            iconTone="neutral"
            title={t('Ohne Ordner')}
            value={counts.folder(null)}
            onClick={() => open({ kind: 'folder', id: null })}
          />
        </ListSection>

        {choices.organizations.map((org) => (
          <ListSection
            key={org.id}
            header={
              <span className="m-row-title">
                <ICONS.organization aria-hidden />
                <span>{org.name}</span>
              </span>
            }
          >
            <ListRow
              icon={ICONS.vault}
              iconTone="neutral"
              title={t('Alle Einträge')}
              value={counts.organization(org.id)}
              onClick={() => open({ kind: 'organization', id: org.id })}
            />
            {choices.collections
              .filter((collection) => collection.organizationId === org.id)
              .map((collection) => (
                <ListRow
                  key={collection.id}
                  icon={ICONS.collection}
                  iconTone="neutral"
                  title={collection.name}
                  value={counts.collection(collection.id)}
                  onClick={() => open({ kind: 'collection', id: collection.id })}
                />
              ))}
          </ListSection>
        ))}

        {choices.suite && (
          <ListSection header={t('UwU-Apps')}>
            {(['ssh', 'rdp'] as const).map((space) => (
              <ListRow
                key={space}
                icon={space === 'ssh' ? ICONS.terminal : ICONS.computer}
                iconTone="solid"
                title={t(SPACE_TITLE[space])}
                onClick={() => nav.open({ page: 'suite', space })}
              />
            ))}
          </ListSection>
        )}

        {(choices.sends || choices.requests || choices.masked) && (
          <ListSection header={t('Extras')}>
            {choices.sends && (
              <ListRow
                icon={ICONS.send}
                iconTone="success"
                title={t('Sends')}
                onClick={() => nav.open({ page: 'sends' })}
              />
            )}
            {choices.requests && (
              <ListRow
                icon={ICONS.inbox}
                iconTone="success"
                title={t('Dateianfragen')}
                trailing={
                  choices.unseenUploads > 0 ? (
                    <span
                      className="m-count-badge"
                      aria-label={t('{n} neue Uploads', { n: choices.unseenUploads })}
                    >
                      {choices.unseenUploads}
                    </span>
                  ) : undefined
                }
                onClick={() => nav.open({ page: 'requests' })}
              />
            )}
            {choices.masked && (
              <ListRow
                icon={ICONS.maskedAddress}
                iconTone="success"
                title={t('Maskierte Adressen')}
                onClick={() => nav.open({ page: 'masked' })}
              />
            )}
          </ListSection>
        )}

        {choices.trash && (
          <>
            <div className="m-gap" />
            <ListSection>
              <ListRow
                icon={ICONS.delete}
                iconTone="neutral"
                title={t('Papierkorb')}
                value={counts.trash}
                onClick={() => open({ kind: 'trash' })}
              />
            </ListSection>
          </>
        )}

        <p className="m-footnote">{t('Zum Synchronisieren nach unten ziehen')}</p>
      </Page>
      {android && <Fab label={t('Neuer Eintrag')} icon={ICONS.add} onClick={add} />}
      {travelOpen && travel.enabled && (
        <TravelDialog hidden={travel.hidden} onClose={() => setTravelOpen(false)} />
      )}
    </>
  );
}

/** The iPad's sidebar: the same choices as the overview, as a column. */
export function VaultSidebar({ current, onClose }: { current: Route; onClose: () => void }) {
  useLanguage();
  const nav = useNav();
  const choices = useVaultChoices();
  const { counts } = choices;
  const isList = (filter: Filter) => current.page === 'list' && sameFilter(current.filter, filter);
  const row = (filter: Filter, icon: LucideIcon, label: string, count: number) => (
    <SidebarRow
      key={JSON.stringify(filter)}
      label={label}
      icon={icon}
      count={count}
      current={isList(filter)}
      onClick={() => nav.open({ page: 'list', filter })}
    />
  );
  return (
    <>
      <div className="m-sidebar-head">
        <AvatarButton />
        <b>{t('Tresor')}</b>
        <NavButton
          label={t('Seitenleiste ausblenden')}
          icon={ICONS.sidebarHide}
          onClick={onClose}
        />
      </div>
      <div className="uwu-split-sidebar-scroll">
        {row({ kind: 'all' }, ICONS.vault, t('Alle Einträge'), counts.all)}
        {row({ kind: 'favorites' }, ICONS.favorite, t('Favoriten'), counts.favorites)}
        {choices.reminders &&
          row({ kind: 'due' }, ICONS.reminder, t('Neues Passwort fällig'), counts.due)}
        <SidebarHeading>{t('Typen')}</SidebarHeading>
        {TYPES.map((type) =>
          row({ kind: 'type', type: type.type }, type.icon, t(type.label), counts.type(type.type)),
        )}
        <SidebarHeading>{t('Ordner')}</SidebarHeading>
        {choices.folders.map((folder) =>
          row(
            { kind: 'folder', id: folder.id },
            ICONS.folder,
            folder.name,
            counts.folder(folder.id),
          ),
        )}
        {row({ kind: 'folder', id: null }, ICONS.folder, t('Ohne Ordner'), counts.folder(null))}
        {choices.organizations.map((org) => (
          <div key={org.id}>
            <SidebarHeading>{org.name}</SidebarHeading>
            {row(
              { kind: 'organization', id: org.id },
              ICONS.vault,
              t('Alle Einträge'),
              counts.organization(org.id),
            )}
            {choices.collections
              .filter((collection) => collection.organizationId === org.id)
              .map((collection) =>
                row(
                  { kind: 'collection', id: collection.id },
                  ICONS.collection,
                  collection.name,
                  counts.collection(collection.id),
                ),
              )}
          </div>
        ))}
        {(choices.sends || choices.requests || choices.masked) && (
          <SidebarHeading>{t('Extras')}</SidebarHeading>
        )}
        {choices.sends && (
          <SidebarRow
            label={t('Sends')}
            icon={ICONS.send}
            current={current.page === 'sends'}
            onClick={() => nav.open({ page: 'sends' })}
          />
        )}
        {choices.requests && (
          <SidebarRow
            label={t('Dateianfragen')}
            icon={ICONS.inbox}
            current={current.page === 'requests'}
            trailing={
              choices.unseenUploads > 0 ? (
                <span className="m-count-badge">{choices.unseenUploads}</span>
              ) : undefined
            }
            onClick={() => nav.open({ page: 'requests' })}
          />
        )}
        {choices.masked && (
          <SidebarRow
            label={t('Maskierte Adressen')}
            icon={ICONS.maskedAddress}
            current={current.page === 'masked'}
            onClick={() => nav.open({ page: 'masked' })}
          />
        )}
        {choices.suite && <SidebarHeading>{t('UwU-Apps')}</SidebarHeading>}
        {choices.suite &&
          (['ssh', 'rdp'] as const).map((space) => (
            <SidebarRow
              key={space}
              label={t(SPACE_TITLE[space])}
              icon={space === 'ssh' ? ICONS.terminal : ICONS.computer}
              current={current.page === 'suite' && current.space === space}
              onClick={() => nav.open({ page: 'suite', space })}
            />
          ))}
        {choices.trash && (
          <>
            <div className="m-gap" />
            {row({ kind: 'trash' }, ICONS.delete, t('Papierkorb'), counts.trash)}
          </>
        )}
      </div>
    </>
  );
}
