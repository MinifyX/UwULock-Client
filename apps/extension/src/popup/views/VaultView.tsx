import { useEffect, useMemo, useState } from 'react';
import { Icon } from '@desktop/components/Icon';
import { ItemTile } from '@desktop/components/ItemTile';
import { NyuScene } from '@desktop/components/nyu/scenes';
import { N_, t } from '../../shared/i18n';
import type { ItemKind, ItemSummary, Overview, StatusMessage } from '../../shared/protocol';
import { ext } from '../../shared/browser';
import { vaultItems, vaultOverview } from '../api';
import { errorText, toast } from '../lib';

type Filter =
  | { kind: 'all' }
  | { kind: 'favorites' }
  | { kind: 'type'; type: ItemKind }
  | { kind: 'folder'; id: string | null }
  | { kind: 'trash' };

const TYPES: { type: ItemKind; label: string }[] = [
  { type: 'login', label: N_('Logins') },
  { type: 'card', label: N_('Karten') },
  { type: 'identity', label: N_('Identitäten') },
  { type: 'note', label: N_('Notizen') },
  { type: 'ssh-key', label: N_('SSH-Schlüssel') },
];

function matches(item: ItemSummary, query: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  return [item.name, item.subtitle, item.host].some((text) => text?.toLowerCase().includes(q));
}

/** Everything in the vault: search, favourites, by kind and by folder, and the trash. */
export function VaultView({ onOpen }: { onOpen: (id: string) => void }) {
  const [items, setItems] = useState<ItemSummary[] | null>(null);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>({ kind: 'all' });

  useEffect(() => {
    const load = async () => {
      try {
        setItems(await vaultItems());
        setOverview(await vaultOverview());
      } catch (e) {
        // Locked meanwhile: the lock screen takes over, nothing to say here.
        if ((e as { kind?: string }).kind !== 'locked') toast(errorText(e), 'error');
      }
    };
    void load();
    const listener = (message: unknown) => {
      if ((message as StatusMessage | null)?.type === 'bg:status-changed') void load();
    };
    ext.runtime.onMessage.addListener(listener);
    return () => ext.runtime.onMessage.removeListener(listener);
  }, []);

  const shown = useMemo(() => {
    if (!items) return [];
    return items
      .filter((item) => (filter.kind === 'trash' ? item.deleted : !item.deleted && !item.archived))
      .filter((item) => {
        switch (filter.kind) {
          case 'favorites':
            return item.favorite;
          case 'type':
            return item.kind === filter.type;
          case 'folder':
            return item.folderId === filter.id && !item.organizationId;
          default:
            return true;
        }
      })
      .filter((item) => matches(item, query.trim()))
      .sort((a, b) => a.name.localeCompare(b.name));
  }, [items, filter, query]);

  const value =
    filter.kind === 'type'
      ? `type:${filter.type}`
      : filter.kind === 'folder'
        ? `folder:${filter.id ?? ''}`
        : filter.kind;

  const choose = (next: string) => {
    if (next.startsWith('type:')) setFilter({ kind: 'type', type: next.slice(5) as ItemKind });
    else if (next.startsWith('folder:')) setFilter({ kind: 'folder', id: next.slice(7) || null });
    else setFilter({ kind: next as 'all' | 'favorites' | 'trash' });
  };

  return (
    <div className="popup-scroll">
      <div className="vault-tools">
        <label className="search-box">
          <Icon name="search" size={15} />
          <input
            className="search"
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t('Tresor durchsuchen')}
            aria-label={t('Tresor durchsuchen')}
            autoFocus
            spellCheck={false}
          />
        </label>
        <select
          className="select"
          value={value}
          onChange={(e) => choose(e.target.value)}
          aria-label={t('Anzeigen')}
        >
          <option value="all">{t('Alle Einträge')}</option>
          <option value="favorites">{t('Favoriten')}</option>
          <optgroup label={t('Typen')}>
            {TYPES.map((entry) => (
              <option key={entry.type} value={`type:${entry.type}`}>
                {t(entry.label)}
              </option>
            ))}
          </optgroup>
          {overview && (
            <optgroup label={t('Ordner')}>
              {overview.folders.map((folder) => (
                <option key={folder.id} value={`folder:${folder.id}`}>
                  {folder.name}
                </option>
              ))}
              <option value="folder:">{t('Ohne Ordner')}</option>
            </optgroup>
          )}
          <option value="trash">{t('Papierkorb')}</option>
        </select>
      </div>
      {!items && <div aria-busy />}
      {items && shown.length === 0 && (
        <div className="empty-block">
          <NyuScene name={query ? 'puzzled' : 'sleepy'} className="empty-scene small" />
          <p className="empty-line">
            {query ? t('Nichts gefunden (・・?)') : t('Hier ist noch nichts.')}
          </p>
        </div>
      )}
      <ul className="item-list plain">
        {shown.map((item) => (
          <li
            key={item.id}
            className="item-row"
            tabIndex={0}
            onClick={() => onOpen(item.id)}
            onKeyDown={(e) => e.key === 'Enter' && onOpen(item.id)}
          >
            <ItemTile item={item} />
            <span className="item-text">
              <span className="item-name">{item.name || t('(ohne Namen)')}</span>
              {item.subtitle && <span className="item-sub">{item.subtitle}</span>}
            </span>
            <span className="item-badges">
              {item.hasTotp && <Icon name="clock" size={13} title={t('Einmal-Code')} />}
              {item.reprompt && (
                <Icon name="shield" size={13} title={t('Fragt nach dem Master-Passwort')} />
              )}
              {item.favorite && (
                <Icon name="star" size={13} className="badge-star" title={t('Favorit')} />
              )}
              {item.broken && <Icon name="warning" size={13} className="badge-warning" />}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
