import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  copyField,
  deleteFolder,
  saveFolder,
  vaultItems,
  vaultOverview,
  type ItemKind,
  type ItemSummary,
  type Overview,
  type Status,
} from '../lib/api';
import { useBackLayer } from '../lib/backStack';
import { toastError } from '../lib/errors';
import { copiedText } from '../lib/format';
import { N_, t, useLanguage } from '../lib/i18n';
import { KIND_LABEL } from '../lib/items';
import { usePhoneLayout } from '../lib/phone';
import { platform } from '../lib/platform';
import { useSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import { has, loadIcons, openWebVaultAt, useUwu } from '../lib/uwu';
import { AccountCard } from './AccountCard';
import { ContextMenu, type MenuItem } from './ContextMenu';
import { FileRequestsDialog } from './FileRequestsDialog';
import { HealthPane } from './HealthPane';
import { Icon, type IconName } from './Icon';
import { ItemDetail } from './ItemDetail';
import { ItemEditor } from './ItemEditor';
import { ItemTile } from './ItemTile';
import { WifiConnect } from './WifiConnect';
import { MaskedDialog } from './MaskedDialog';
import { Modal } from './Modal';
import { NyuScene } from './nyu/scenes';

export type Filter =
  | { kind: 'all' }
  | { kind: 'favorites' }
  | { kind: 'type'; type: ItemKind }
  | { kind: 'folder'; id: string | null }
  | { kind: 'collection'; id: string }
  | { kind: 'organization'; id: string }
  | { kind: 'due' }
  | { kind: 'trash' };

const TYPES: { type: ItemKind; label: string; icon: IconName }[] = [
  { type: 'login', label: N_('Logins'), icon: 'globe' },
  { type: 'card', label: N_('Karten'), icon: 'card' },
  { type: 'identity', label: N_('Identitäten'), icon: 'id' },
  { type: 'note', label: N_('Notizen'), icon: 'note' },
  { type: 'ssh-key', label: N_('SSH-Schlüssel'), icon: 'key' },
  { type: 'wifi', label: N_('WLAN'), icon: 'wifi' },
];

function matches(filter: Filter, item: ItemSummary, due: Set<string>): boolean {
  if (filter.kind === 'trash') return item.deleted;
  if (item.deleted) return false;
  switch (filter.kind) {
    case 'all':
      return true;
    case 'favorites':
      return item.favorite;
    case 'type':
      return item.kind === filter.type;
    case 'folder':
      return !item.organizationId && item.folderId === filter.id;
    case 'collection':
      return item.collectionIds.includes(filter.id);
    case 'organization':
      return item.organizationId === filter.id;
    case 'due':
      return due.has(item.id);
  }
}

function same(a: Filter, b: Filter) {
  return JSON.stringify(a) === JSON.stringify(b);
}

type Props = {
  status: Status;
  /** The search field, for Ctrl+F from the app. */
  searchRef: React.RefObject<HTMLInputElement>;
  onAddAccount: () => void;
};

/** What the editor is open for: an item to change, or a new one of that kind. */
type Editing = { summary: ItemSummary | null; kind: ItemKind };

export function VaultScreen({ status, searchRef, onAddAccount }: Props) {
  useLanguage();
  const settings = useSettings();
  const [items, setItems] = useState<ItemSummary[]>([]);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [filter, setFilter] = useState<Filter>({ kind: 'all' });
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  // Phone layout (lib/phone.ts): one pane at a time. The list is the start;
  // tapping an item opens it over the list, the folders are a drawer.
  const phone = usePhoneLayout();
  const [opened, setOpened] = useState(false);
  const [drawer, setDrawer] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [newMenu, setNewMenu] = useState<{ x: number; y: number } | null>(null);
  const [folderMenu, setFolderMenu] = useState<{ x: number; y: number; id: string } | null>(null);
  const [folderDialog, setFolderDialog] = useState<null | { id: string | null; name: string }>(
    null,
  );
  const [folderToDelete, setFolderToDelete] = useState<{ id: string; name: string } | null>(null);
  const [extrasDialog, setExtrasDialog] = useState<null | 'file-requests' | 'masked'>(null);
  // The password check in place of the list and the item: its report, or the review.
  const [health, setHealth] = useState<null | 'report' | 'review'>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const uwu = useUwu();
  const due = useMemo(
    () =>
      new Set(
        Object.entries(uwu.reminders)
          .filter(([, reminder]) => reminder.isDue)
          .map(([id]) => id),
      ),
    [uwu.reminders],
  );
  // An admin switched an extra off while its list or window was open: back
  // to all items, the window closes.
  const remindersOn = has(uwu, 'reminders');
  useEffect(() => {
    if (!remindersOn && filter.kind === 'due') setFilter({ kind: 'all' });
  }, [remindersOn, filter.kind]);
  const extrasAllowed =
    extrasDialog === 'file-requests'
      ? has(uwu, 'file-requests')
      : extrasDialog === 'masked'
        ? has(uwu, 'masked-addresses')
        : true;
  useEffect(() => {
    if (!extrasAllowed) setExtrasDialog(null);
  }, [extrasAllowed]);

  const reload = useCallback(async () => {
    try {
      const [list, info] = await Promise.all([vaultItems(), vaultOverview()]);
      setItems(list);
      setOverview(info);
    } catch (e) {
      toastError(e);
    } finally {
      setLoaded(true);
    }
  }, []);

  // Also on a switch: the other account's items must not stay on screen while
  // its sync is still on its way.
  useEffect(() => {
    void reload();
    const stop = listen('vault-changed', () => void reload());
    return () => void stop.then((unlisten) => unlisten());
  }, [reload, status.accountId]);

  const counts = useMemo(() => {
    const live = items.filter((i) => !i.deleted);
    return {
      all: live.length,
      favorites: live.filter((i) => i.favorite).length,
      trash: items.length - live.length,
      type: (type: ItemKind) => live.filter((i) => i.kind === type).length,
      folder: (id: string | null) =>
        live.filter((i) => !i.organizationId && i.folderId === id).length,
      collection: (id: string) => live.filter((i) => i.collectionIds.includes(id)).length,
      organization: (id: string) => live.filter((i) => i.organizationId === id).length,
      due: live.filter((i) => due.has(i.id)).length,
    };
  }, [items, due]);

  const visible = useMemo(() => {
    const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    const collator = new Intl.Collator(undefined, { sensitivity: 'base', numeric: true });
    return items
      .filter((item) =>
        words.length ? !item.deleted || filter.kind === 'trash' : matches(filter, item, due),
      )
      .filter((item) => {
        if (!words.length) return true;
        const haystack = `${item.name} ${item.subtitle ?? ''} ${item.host ?? ''}`.toLowerCase();
        return words.every((word) => haystack.includes(word));
      })
      .sort(
        (a, b) =>
          collator.compare(a.name, b.name) || collator.compare(a.subtitle ?? '', b.subtitle ?? ''),
      );
  }, [items, filter, query, due]);

  // Icons for what is on screen: asked of Rust, which asks the server; again
  // when an own icon changed.
  useEffect(() => {
    if (!uwu.uwu) return;
    const timer = window.setTimeout(
      () =>
        void loadIcons(
          visible.slice(0, 400).map((item) => item.id),
          settings.siteIcons,
        ),
      150,
    );
    return () => window.clearTimeout(timer);
  }, [visible, uwu.uwu, uwu.ownIcons, settings.siteIcons]);

  // Keep a selection that is still visible, or take the first.
  useEffect(() => {
    if (!loaded) return;
    if (selected && visible.some((i) => i.id === selected)) return;
    setSelected(visible[0]?.id ?? null);
  }, [visible, selected, loaded]);

  const move = (step: number) => {
    if (!visible.length) return;
    const index = visible.findIndex((i) => i.id === selected);
    const next = visible[Math.max(0, Math.min(visible.length - 1, index + step))];
    if (!next) return;
    setSelected(next.id);
    listRef.current
      ?.querySelector<HTMLElement>(`[data-id="${CSS.escape(next.id)}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  };

  const current = items.find((i) => i.id === selected) ?? null;
  const showItem = (id: string) => {
    setSelected(id);
    setOpened(true);
  };
  const detailOpen = phone && opened && current !== null;
  useBackLayer(phone && drawer, () => setDrawer(false));
  useBackLayer(detailOpen, () => setOpened(false));
  useBackLayer(phone && health !== null, () =>
    setHealth((now) => (now === 'review' ? 'report' : null)),
  );

  // Ctrl+U, Ctrl+P, Ctrl+T copy username, password and code of the selected
  // item, as in Bitwarden's desktop app.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      if (document.querySelector('.modal')) return;
      const field = { u: 'username', p: 'password', t: 'totp' }[event.key.toLowerCase()];
      if (!field || !current || current.kind !== 'login') return;
      event.preventDefault();
      void copyField(current.id, field)
        .then(() => toast(copiedText(field, settings.clipboardClear)))
        .catch((e) => toastError(e));
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [current, settings.clipboardClear]);

  const filterTitle = (): string => {
    switch (filter.kind) {
      case 'all':
        return t('Alle Einträge');
      case 'favorites':
        return t('Favoriten');
      case 'trash':
        return t('Papierkorb');
      case 'due':
        return t('Neues Passwort fällig');
      case 'type':
        return t(TYPES.find((x) => x.type === filter.type)?.label ?? '');
      case 'folder':
        return overview?.folders.find((f) => f.id === filter.id)?.name ?? t('Ohne Ordner');
      case 'organization':
        return overview?.organizations.find((o) => o.id === filter.id)?.name ?? '';
      case 'collection':
        return overview?.collections.find((c) => c.id === filter.id)?.name ?? '';
    }
  };
  const title = query.trim() ? t('Suche') : filterTitle();

  const pick = (next: Filter) => {
    setFilter(next);
    setQuery('');
    setHealth(null);
  };

  const nav = (target: Filter, icon: IconName, label: string, count: number) => (
    <li key={JSON.stringify(target)}>
      <button
        className="nav-row"
        aria-current={!health && !query.trim() && same(filter, target) ? 'true' : undefined}
        onClick={() => pick(target)}
      >
        <Icon name={icon} size={16} />
        <span className="nav-label">{label}</span>
        {count > 0 && <span className="nav-count">{count}</span>}
      </button>
    </li>
  );

  const folderActions = (id: string): MenuItem[] => {
    const folder = overview?.folders.find((f) => f.id === id);
    if (!folder) return [];
    return [
      {
        label: t('Umbenennen'),
        icon: 'pencil',
        onSelect: () => setFolderDialog({ id, name: folder.name }),
      },
      {
        label: t('Löschen'),
        icon: 'trash',
        danger: true,
        onSelect: () => setFolderToDelete({ id, name: folder.name }),
      },
    ];
  };

  const noFolder = counts.folder(null);

  return (
    <div
      className="vault"
      data-pane={health ? 'report' : detailOpen ? 'detail' : 'list'}
      data-drawer={phone && drawer ? 'open' : undefined}
    >
      {phone && drawer && (
        <div className="drawer-backdrop" aria-hidden onClick={() => setDrawer(false)} />
      )}
      <nav
        className="sidebar"
        aria-label={t('Tresor')}
        onClick={(event) => {
          // On a phone, picking a list closes the drawer.
          if (phone && (event.target as HTMLElement).closest('.nav-row')) setDrawer(false);
        }}
      >
        <ul className="nav-list">
          {nav({ kind: 'all' }, 'layers', t('Alle Einträge'), counts.all)}
          {nav({ kind: 'favorites' }, 'star', t('Favoriten'), counts.favorites)}
          {has(uwu, 'reminders') &&
            (counts.due > 0 || filter.kind === 'due') &&
            nav({ kind: 'due' }, 'bell', t('Neues Passwort fällig'), counts.due)}
          <li>
            <button
              className="nav-row"
              aria-current={health ? 'true' : undefined}
              data-testid="nav-health"
              onClick={() => setHealth('report')}
            >
              <Icon name="shield" size={16} />
              <span className="nav-label">{t('Passwortprüfung')}</span>
            </button>
          </li>
        </ul>

        <h2>{t('Typen')}</h2>
        <ul className="nav-list">
          {TYPES.filter((x) => counts.type(x.type) > 0 || x.type === 'login').map((x) =>
            nav({ kind: 'type', type: x.type }, x.icon, t(x.label), counts.type(x.type)),
          )}
        </ul>

        {overview && (
          <>
            <h2 className="nav-heading">
              {t('Ordner')}
              <button
                className="icon-button tiny"
                title={t('Neuer Ordner')}
                aria-label={t('Neuer Ordner')}
                onClick={() => setFolderDialog({ id: null, name: '' })}
              >
                <Icon name="folderPlus" size={14} />
              </button>
            </h2>
            <ul className="nav-list">
              {[...overview.folders]
                .sort((a, b) => a.name.localeCompare(b.name))
                .map((f) => (
                  <li
                    key={f.id}
                    onContextMenu={(event) => {
                      event.preventDefault();
                      setFolderMenu({ x: event.clientX, y: event.clientY, id: f.id });
                    }}
                  >
                    <button
                      className="nav-row"
                      aria-current={
                        !query.trim() && same(filter, { kind: 'folder', id: f.id })
                          ? 'true'
                          : undefined
                      }
                      onClick={() => pick({ kind: 'folder', id: f.id })}
                    >
                      <Icon name="folder" size={16} />
                      <span className="nav-label">{f.name}</span>
                      {counts.folder(f.id) > 0 && (
                        <span className="nav-count">{counts.folder(f.id)}</span>
                      )}
                    </button>
                  </li>
                ))}
              {overview.folders.length > 0 &&
                noFolder > 0 &&
                nav({ kind: 'folder', id: null }, 'folder', t('Ohne Ordner'), noFolder)}
            </ul>
          </>
        )}

        {overview?.organizations.map((org) => (
          <div key={org.id}>
            <h2 className="nav-heading org-heading">
              <Icon name="building" size={13} />
              <span className="nav-label">{org.name}</span>
              <button
                className="icon-button tiny"
                title={t('Im Web-Tresor verwalten')}
                aria-label={t('{name} im Web-Tresor verwalten', { name: org.name })}
                onClick={() =>
                  void openWebVaultAt('organization', org.id).catch((e) => toastError(e))
                }
              >
                <Icon name="external" size={13} />
              </button>
            </h2>
            <ul className="nav-list">
              {nav(
                { kind: 'organization', id: org.id },
                'layers',
                t('Alle Einträge'),
                counts.organization(org.id),
              )}
              {overview.collections
                .filter((c) => c.organizationId === org.id)
                .sort((a, b) => a.name.localeCompare(b.name))
                .map((c) =>
                  nav({ kind: 'collection', id: c.id }, 'grid', c.name, counts.collection(c.id)),
                )}
            </ul>
          </div>
        ))}

        {(has(uwu, 'file-requests') || has(uwu, 'masked-addresses')) && (
          <>
            <h2>{t('Extras')}</h2>
            <ul className="nav-list">
              {has(uwu, 'file-requests') && (
                <li>
                  <button className="nav-row" onClick={() => setExtrasDialog('file-requests')}>
                    <Icon name="inbox" size={16} />
                    <span className="nav-label">{t('Dateianfragen')}</span>
                    {uwu.unseen.fileRequestSubmissions > 0 && (
                      <span
                        className="nav-badge"
                        title={t('{n} neue Uploads', { n: uwu.unseen.fileRequestSubmissions })}
                      >
                        {uwu.unseen.fileRequestSubmissions}
                      </span>
                    )}
                  </button>
                </li>
              )}
              {has(uwu, 'masked-addresses') && (
                <li>
                  <button className="nav-row" onClick={() => setExtrasDialog('masked')}>
                    <Icon name="mask" size={16} />
                    <span className="nav-label">{t('Maskierte Adressen')}</span>
                  </button>
                </li>
              )}
            </ul>
          </>
        )}

        {settings.showTrash && counts.trash > 0 && (
          <ul className="nav-list nav-trash">
            {nav({ kind: 'trash' }, 'trash', t('Papierkorb'), counts.trash)}
          </ul>
        )}

        <span className="spacer" />
        <AccountCard status={status} onAddAccount={onAddAccount} />
      </nav>

      {health ? (
        <HealthPane
          mode={health}
          onMode={setHealth}
          items={items}
          phone={phone}
          onMenu={() => setDrawer(true)}
          onOpen={(id) => {
            pick({ kind: 'all' });
            showItem(id);
          }}
        />
      ) : (
        <>
          <section className="list-pane" aria-label={title}>
            <div className="list-head">
              <div className="search-row">
                {phone && (
                  <button
                    className="icon-button menu-button"
                    aria-label={t('Ordner und Typen')}
                    aria-expanded={drawer}
                    onClick={() => setDrawer(true)}
                  >
                    <Icon name="menu" size={18} />
                  </button>
                )}
                <label className="search-box">
                  <Icon name="search" size={15} />
                  <input
                    ref={searchRef}
                    className="search"
                    type="search"
                    value={query}
                    placeholder={phone ? t('Tresor durchsuchen') : t('Tresor durchsuchen (Strg+F)')}
                    aria-label={t('Tresor durchsuchen')}
                    spellCheck={false}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
                        e.preventDefault();
                        move(e.key === 'ArrowDown' ? 1 : -1);
                      } else if (e.key === 'Escape' && query) {
                        e.preventDefault();
                        e.stopPropagation();
                        setQuery('');
                      }
                    }}
                  />
                </label>
              </div>
              <p className="list-title">
                <span>{title}</span>
                <span className="list-count">{visible.length}</span>
                <span className="spacer" />
                <button
                  className="new-item"
                  aria-haspopup="menu"
                  aria-expanded={Boolean(newMenu)}
                  title={t('Neuer Eintrag')}
                  onClick={(event) => {
                    const rect = event.currentTarget.getBoundingClientRect();
                    setNewMenu({ x: rect.right - 180, y: rect.bottom + 4 });
                  }}
                >
                  <Icon name="plus" size={15} />
                  {t('Neu')}
                </button>
              </p>
            </div>

            {visible.length > 0 ? (
              <ul
                ref={listRef}
                className="item-list"
                role="listbox"
                aria-label={title}
                tabIndex={0}
                aria-activedescendant={selected ? `item-${selected}` : undefined}
                onKeyDown={(e) => {
                  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
                    e.preventDefault();
                    move(e.key === 'ArrowDown' ? 1 : -1);
                  } else if (e.key === 'Home' || e.key === 'End') {
                    e.preventDefault();
                    move(e.key === 'Home' ? -visible.length : visible.length);
                  }
                }}
              >
                {visible.map((item) => (
                  <li
                    key={item.id}
                    id={`item-${item.id}`}
                    data-id={item.id}
                    role="option"
                    aria-selected={item.id === selected}
                    className="item-row"
                    onClick={() => showItem(item.id)}
                  >
                    <ItemTile item={item} />
                    <span className="item-text">
                      <span className="item-name">{item.name || t('(ohne Namen)')}</span>
                      {item.subtitle && <span className="item-sub">{item.subtitle}</span>}
                    </span>
                    <span className="item-badges">
                      {item.broken && (
                        <span title={t('Nicht alles ließ sich entschlüsseln')}>
                          <Icon name="warning" size={13} className="badge-warning" />
                        </span>
                      )}
                      {item.reprompt && (
                        <Icon name="lock" size={13} title={t('Fragt nach dem Master-Passwort')} />
                      )}
                      {due.has(item.id) && !item.deleted && (
                        <Icon
                          name="bell"
                          size={13}
                          className="badge-due"
                          title={t('Neues Passwort fällig')}
                        />
                      )}
                      {uwu.masked[item.id] && (
                        <Icon name="mask" size={13} title={t('Mit maskierter Adresse')} />
                      )}
                      {item.hasTotp && <Icon name="clock" size={13} title={t('Mit Einmal-Code')} />}
                      {item.organizationId && (
                        <Icon name="building" size={13} title={t('Organisation')} />
                      )}
                      {item.favorite && (
                        <Icon name="star" size={13} className="badge-star" title={t('Favorit')} />
                      )}
                    </span>
                  </li>
                ))}
              </ul>
            ) : (
              <div className="list-empty">
                {loaded && (
                  <>
                    <NyuScene
                      name={query ? 'puzzled' : items.length ? 'sleepy' : 'pick'}
                      className="empty-scene"
                    />
                    <p>
                      {query
                        ? t('Nichts gefunden für „{query}“.', { query: query.trim() })
                        : items.length
                          ? t('Hier ist nichts. (˘ω˘)')
                          : t('Dein Tresor ist noch leer. Leg oben rechts den ersten Eintrag an.')}
                    </p>
                  </>
                )}
              </div>
            )}
          </section>

          <section className="detail-pane">
            {detailOpen && (
              <div className="detail-back">
                <button className="quiet" onClick={() => setOpened(false)}>
                  <Icon name="back" size={16} />
                  {title}
                </button>
              </div>
            )}
            {current ? (
              <ItemDetail
                key={current.id}
                summary={current}
                overview={overview}
                onEdit={() => setEditing({ summary: current, kind: current.kind })}
                wifiActions={
                  // Only Android lets an app add a network; iOS needs an entitlement a
                  // sideloaded app never gets (docs/mobile.md).
                  platform() === 'android'
                    ? (wifi) => <WifiConnect id={current.id} wifi={wifi} />
                    : undefined
                }
              />
            ) : (
              <div className="detail-empty">
                {loaded && <NyuScene name="vault" className="empty-scene" />}
              </div>
            )}
          </section>
        </>
      )}

      {newMenu && (
        <ContextMenu
          x={newMenu.x}
          y={newMenu.y}
          label={t('Neuer Eintrag')}
          onClose={() => setNewMenu(null)}
          items={TYPES.map((type) => ({
            label: t(KIND_LABEL[type.type]),
            icon: type.icon,
            onSelect: () => setEditing({ summary: null, kind: type.type }),
          }))}
        />
      )}

      {folderMenu && (
        <ContextMenu
          x={folderMenu.x}
          y={folderMenu.y}
          label={t('Ordner')}
          onClose={() => setFolderMenu(null)}
          items={folderActions(folderMenu.id)}
        />
      )}

      {folderDialog && (
        <FolderDialog
          folder={folderDialog}
          onClose={() => setFolderDialog(null)}
          onSaved={() => {
            setFolderDialog(null);
            void reload();
          }}
        />
      )}

      {folderToDelete && (
        <Modal
          title={t('Ordner löschen?')}
          onCancel={() => setFolderToDelete(null)}
          footer={
            <>
              <span className="spacer" />
              <button
                className="danger"
                data-secondary
                onClick={() => {
                  const id = folderToDelete.id;
                  setFolderToDelete(null);
                  void deleteFolder(id)
                    .then(() => {
                      if (filter.kind === 'folder' && filter.id === id) setFilter({ kind: 'all' });
                      toast(t('Ordner gelöscht.'));
                    })
                    .catch((e) => toastError(e));
                }}
              >
                {t('Löschen')}
              </button>
              <button className="primary" data-autofocus onClick={() => setFolderToDelete(null)}>
                {t('Abbrechen')}
              </button>
            </>
          }
        >
          <p className="dialog-lead">
            {t('„{name}“ verschwindet. Die Einträge darin bleiben – dann ohne Ordner.', {
              name: folderToDelete.name,
            })}
          </p>
        </Modal>
      )}

      {extrasAllowed && extrasDialog === 'file-requests' && (
        <FileRequestsDialog
          onClose={() => setExtrasDialog(null)}
          onTakenOver={(id) => {
            setExtrasDialog(null);
            pick({ kind: 'all' });
            showItem(id);
            void reload();
          }}
        />
      )}
      {extrasAllowed && extrasDialog === 'masked' && (
        <MaskedDialog
          items={items}
          onClose={() => setExtrasDialog(null)}
          onOpenItem={(id) => {
            setExtrasDialog(null);
            pick({ kind: 'all' });
            showItem(id);
          }}
        />
      )}

      {editing && (
        <ItemEditor
          key={editing.summary?.id ?? `new-${editing.kind}`}
          summary={editing.summary}
          kind={editing.kind}
          overview={overview}
          onClose={() => setEditing(null)}
          onSaved={(id) => {
            setEditing(null);
            showItem(id);
            void reload();
          }}
        />
      )}
    </div>
  );
}

function FolderDialog({
  folder,
  onClose,
  onSaved,
}: {
  folder: { id: string | null; name: string };
  onClose: () => void;
  onSaved: () => void;
}) {
  useLanguage();
  const [name, setName] = useState(folder.name);
  const [busy, setBusy] = useState(false);
  const save = async () => {
    if (!name.trim() || busy) return;
    setBusy(true);
    try {
      await saveFolder(folder.id, name.trim());
      onSaved();
    } catch (e) {
      toastError(e);
      setBusy(false);
    }
  };
  return (
    <Modal
      title={folder.id ? t('Ordner umbenennen') : t('Neuer Ordner')}
      onCancel={onClose}
      footer={
        <>
          <button className="quiet" data-secondary onClick={onClose}>
            {t('Abbrechen')}
          </button>
          <span className="spacer" />
          <button className="primary" disabled={!name.trim() || busy} onClick={() => void save()}>
            {folder.id ? t('Übernehmen') : t('Anlegen')}
          </button>
        </>
      }
    >
      <label className="field">
        <span>{t('Name')}</span>
        <input
          type="text"
          value={name}
          maxLength={100}
          autoFocus
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && void save()}
        />
      </label>
    </Modal>
  );
}
