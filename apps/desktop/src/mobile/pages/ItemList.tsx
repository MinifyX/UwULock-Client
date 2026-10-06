/**
 * The items of one list (all, a type, a folder, the trash …), by name. On an
 * iPad it is the split view's middle column, with the search field on top.
 */

import { Fab, ICONS, ListSection, NavButton } from '@uwusuite/design';
import { useMemo } from 'react';
import type { Overview } from '../../lib/api';
import { visibleItems, type Filter } from '../../lib/filters';
import { t, useLanguage } from '../../lib/i18n';
import { useMobile } from '../state';
import { Empty, Page } from '../ui';
import { ItemRow } from './ItemRow';
import { TYPES } from './Overview';

/** What a list is called: "Alle Einträge", "Logins", a folder's name … */
export function listTitle(filter: Filter, overview: Overview | null): string {
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
}

/** The kind a new item from this list gets, and its folder. */
function newFrom(filter: Filter) {
  return {
    type: filter.kind === 'type' ? filter.type : null,
    folderId: filter.kind === 'folder' ? filter.id : null,
  };
}

export function ItemListPage({ filter }: { filter: Filter }) {
  useLanguage();
  const { android, ipad, data, openSheet, sync, query, setQuery } = useMobile();
  const searching = ipad && query.trim() !== '';
  const items = useMemo(
    () => visibleItems(data.items, filter, ipad ? query : '', data.due),
    [data.items, data.due, filter, ipad, query],
  );
  const title = searching ? t('Suche') : listTitle(filter, data.overview);
  const canAdd = filter.kind !== 'trash' && filter.kind !== 'due';
  const add = () => {
    const from = newFrom(filter);
    if (from.type) openSheet({ kind: 'edit', id: null, type: from.type, folderId: from.folderId });
    else openSheet({ kind: 'new' });
  };
  const count = items.length === 1 ? t('1 Eintrag') : t('{n} Einträge', { n: items.length });

  return (
    <>
      <Page
        title={title}
        largeTitle
        subtitle={data.loaded ? count : undefined}
        trailing={
          canAdd && !android ? (
            <NavButton label={t('Neuer Eintrag')} icon={ICONS.add} onClick={add} />
          ) : undefined
        }
        onRefresh={sync}
      >
        {ipad && (
          <label className="m-search-field m-ipad-search" data-uwu-field="">
            <ICONS.search aria-hidden />
            <input
              type="search"
              value={query}
              placeholder={t('Durchsuchen')}
              aria-label={t('Tresor durchsuchen')}
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>
        )}
        {filter.kind === 'trash' && (
          <p className="m-footnote">
            {t('Der Server hebt gelöschte Einträge 30 Tage auf, dann sind sie weg.')}
          </p>
        )}
        <ListSection>
          {items.length ? (
            items.map((item) => <ItemRow key={item.id} item={item} query={ipad ? query : ''} />)
          ) : (
            <Empty title={data.loaded ? (searching ? t('Nichts gefunden') : t('Leer')) : '…'}>
              {data.loaded &&
                (searching
                  ? t('Kein Eintrag passt zu „{query}“.', { query: query.trim() })
                  : t('Hier liegt nichts.'))}
            </Empty>
          )}
        </ListSection>
      </Page>
      {android && canAdd && <Fab label={t('Neuer Eintrag')} icon={ICONS.add} onClick={add} />}
    </>
  );
}
