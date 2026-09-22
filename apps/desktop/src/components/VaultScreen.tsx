import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  copyField,
  syncNow,
  vaultItems,
  vaultOverview,
  type ItemKind,
  type ItemSummary,
  type Overview,
  type Status,
} from '../lib/api';
import { errorText } from '../lib/errors';
import { ago, copiedText } from '../lib/format';
import { N_, t, useLanguage } from '../lib/i18n';
import { useSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import { Icon, type IconName } from './Icon';
import { ItemDetail } from './ItemDetail';
import { ItemTile } from './ItemTile';
import { NyuScene } from './nyu/scenes';

export type Filter =
  | { kind: 'all' }
  | { kind: 'favorites' }
  | { kind: 'type'; type: ItemKind }
  | { kind: 'folder'; id: string | null }
  | { kind: 'collection'; id: string }
  | { kind: 'trash' };

const TYPES: { type: ItemKind; label: string; icon: IconName }[] = [
  { type: 'login', label: N_('Logins'), icon: 'globe' },
  { type: 'card', label: N_('Karten'), icon: 'card' },
  { type: 'identity', label: N_('Identitäten'), icon: 'id' },
  { type: 'note', label: N_('Notizen'), icon: 'note' },
  { type: 'ssh-key', label: N_('SSH-Schlüssel'), icon: 'key' },
];

function matches(filter: Filter, item: ItemSummary): boolean {
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
  }
}

function same(a: Filter, b: Filter) {
  return JSON.stringify(a) === JSON.stringify(b);
}

type Props = {
  status: Status;
  /** The search field, for Ctrl+F from the app. */
  searchRef: React.RefObject<HTMLInputElement>;
};

export function VaultScreen({ status, searchRef }: Props) {
  useLanguage();
  const settings = useSettings();
  const [items, setItems] = useState<ItemSummary[]>([]);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [filter, setFilter] = useState<Filter>({ kind: 'all' });
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const listRef = useRef<HTMLUListElement>(null);

  const reload = useCallback(async () => {
    try {
      const [list, info] = await Promise.all([vaultItems(), vaultOverview()]);
      setItems(list);
      setOverview(info);
    } catch (e) {
      toast(errorText(e), 'error');
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void reload();
    const stop = listen('vault-changed', () => void reload());
    return () => void stop.then((unlisten) => unlisten());
  }, [reload]);

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
    };
  }, [items]);

  const visible = useMemo(() => {
    const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    const collator = new Intl.Collator(undefined, { sensitivity: 'base', numeric: true });
    return items
      .filter((item) =>
        words.length ? !item.deleted || filter.kind === 'trash' : matches(filter, item),
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
  }, [items, filter, query]);

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
        .catch((e) => toast(errorText(e), 'error'));
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [current, settings.clipboardClear]);

  const title = query.trim()
    ? t('Suche')
    : filter.kind === 'all'
      ? t('Alle Einträge')
      : filter.kind === 'favorites'
        ? t('Favoriten')
        : filter.kind === 'trash'
          ? t('Papierkorb')
          : filter.kind === 'type'
            ? t(TYPES.find((x) => x.type === filter.type)?.label ?? '')
            : filter.kind === 'folder'
              ? (overview?.folders.find((f) => f.id === filter.id)?.name ?? t('Ohne Ordner'))
              : (overview?.collections.find((c) => c.id === filter.id)?.name ?? '');

  const pick = (next: Filter) => {
    setFilter(next);
    setQuery('');
  };

  const nav = (target: Filter, icon: IconName, label: string, count: number) => (
    <li key={JSON.stringify(target)}>
      <button
        className="nav-row"
        aria-current={!query.trim() && same(filter, target) ? 'true' : undefined}
        onClick={() => pick(target)}
      >
        <Icon name={icon} size={16} />
        <span className="nav-label">{label}</span>
        {count > 0 && <span className="nav-count">{count}</span>}
      </button>
    </li>
  );

  const noFolder = counts.folder(null);

  return (
    <div className="vault">
      <nav className="sidebar" aria-label={t('Tresor')}>
        <ul className="nav-list">
          {nav({ kind: 'all' }, 'layers', t('Alle Einträge'), counts.all)}
          {nav({ kind: 'favorites' }, 'star', t('Favoriten'), counts.favorites)}
        </ul>

        <h2>{t('Typen')}</h2>
        <ul className="nav-list">
          {TYPES.filter((x) => counts.type(x.type) > 0 || x.type === 'login').map((x) =>
            nav({ kind: 'type', type: x.type }, x.icon, t(x.label), counts.type(x.type)),
          )}
        </ul>

        {overview && (overview.folders.length > 0 || noFolder > 0) && (
          <>
            <h2>{t('Ordner')}</h2>
            <ul className="nav-list">
              {[...overview.folders]
                .sort((a, b) => a.name.localeCompare(b.name))
                .map((f) =>
                  nav({ kind: 'folder', id: f.id }, 'folder', f.name, counts.folder(f.id)),
                )}
              {overview.folders.length > 0 &&
                noFolder > 0 &&
                nav({ kind: 'folder', id: null }, 'folder', t('Ohne Ordner'), noFolder)}
            </ul>
          </>
        )}

        {overview?.organizations.map((org) => (
          <div key={org.id}>
            <h2 className="org-heading">
              <Icon name="building" size={13} />
              {org.name}
            </h2>
            <ul className="nav-list">
              {overview.collections
                .filter((c) => c.organizationId === org.id)
                .sort((a, b) => a.name.localeCompare(b.name))
                .map((c) =>
                  nav({ kind: 'collection', id: c.id }, 'grid', c.name, counts.collection(c.id)),
                )}
            </ul>
          </div>
        ))}

        {settings.showTrash && counts.trash > 0 && (
          <ul className="nav-list nav-trash">
            {nav({ kind: 'trash' }, 'trash', t('Papierkorb'), counts.trash)}
          </ul>
        )}

        <span className="spacer" />
        <SyncCard status={status} />
      </nav>

      <section className="list-pane" aria-label={title}>
        <div className="list-head">
          <label className="search-box">
            <Icon name="search" size={15} />
            <input
              ref={searchRef}
              className="search"
              type="search"
              value={query}
              placeholder={t('Tresor durchsuchen (Strg+F)')}
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
          <p className="list-title">
            <span>{title}</span>
            <span className="list-count">{visible.length}</span>
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
                onClick={() => setSelected(item.id)}
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
                      : t(
                          'Dein Tresor ist noch leer. Neue Einträge legst du vorerst im Web-Tresor an.',
                        )}
                </p>
              </>
            )}
          </div>
        )}
      </section>

      <section className="detail-pane">
        {current ? (
          <ItemDetail key={current.id} summary={current} overview={overview} />
        ) : (
          <div className="detail-empty">
            {loaded && <NyuScene name="vault" className="empty-scene" />}
          </div>
        )}
      </section>
    </div>
  );
}

function SyncCard({ status }: { status: Status }) {
  useLanguage();
  const [, force] = useState(0);
  // "vor 3 Min." keeps itself up to date.
  useEffect(() => {
    const timer = window.setInterval(() => force((n) => n + 1), 30_000);
    return () => window.clearInterval(timer);
  }, []);
  const initial = (status.name || status.email || '?').trim().charAt(0).toUpperCase();
  return (
    <div className="account-card">
      <span className="avatar" aria-hidden>
        {initial}
      </span>
      <span className="account-text">
        <span className="account-email" title={status.email ?? ''}>
          {status.email}
        </span>
        <span
          className="account-sync"
          data-tone={status.syncError ? 'error' : undefined}
          title={status.server ?? undefined}
        >
          {status.syncing
            ? t('Synchronisiert …')
            : status.syncError
              ? t('Sync fehlgeschlagen')
              : t('Synchronisiert {when}', { when: ago(status.lastSync) })}
        </span>
      </span>
      <button
        className="icon-button"
        disabled={status.syncing}
        aria-label={t('Jetzt synchronisieren')}
        title={status.syncError ?? t('Jetzt synchronisieren')}
        onClick={() =>
          void syncNow()
            .then(() => toast(t('Synchronisiert ✧')))
            .catch((e) => toast(errorText(e), 'error'))
        }
      >
        <Icon name="refresh" size={15} className={status.syncing ? 'spin' : undefined} />
      </button>
    </div>
  );
}
