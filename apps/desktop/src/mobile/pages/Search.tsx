/**
 * Searching the vault on a phone. iOS: the tab bar's search field at the
 * bottom, the results on a page above it. Android: a search view with the
 * field at the top. Before anything is typed: the items opened last and a
 * few lists to jump to. While the keyboard is up the title is small, so the
 * hits fit above it.
 */

import {
  ICONS,
  ListSection,
  NavButton,
  Screen,
  SearchBar,
  useKeyboardOpen,
} from '@uwusuite/design';
import type { LucideIcon } from 'lucide-react';
import { useEffect, useMemo, useRef } from 'react';
import { visibleItems, type Filter } from '../../lib/filters';
import { N_, t, useLanguage } from '../../lib/i18n';
import { NavContext, useMobile, type Nav } from '../state';
import { Empty } from '../ui';
import { ItemRow } from './ItemRow';

/** The items opened in this session, the newest first (never stored). */
const recent: string[] = [];

export function rememberOpened(id: string) {
  const at = recent.indexOf(id);
  if (at !== -1) recent.splice(at, 1);
  recent.unshift(id);
  recent.length = Math.min(recent.length, 5);
}

const SUGGESTIONS: { label: string; icon: LucideIcon; filter: Filter }[] = [
  { label: N_('Logins'), icon: ICONS.website, filter: { kind: 'type', type: 'login' } },
  { label: N_('Favoriten'), icon: ICONS.favorite, filter: { kind: 'favorites' } },
  { label: N_('Karten'), icon: ICONS.card, filter: { kind: 'type', type: 'card' } },
  { label: N_('Notizen'), icon: ICONS.note, filter: { kind: 'type', type: 'note' } },
  { label: N_('Fällig'), icon: ICONS.reminder, filter: { kind: 'due' } },
];

export function SearchView() {
  useLanguage();
  const { android, data, query, setQuery, setSearchOpen, openInTab } = useMobile();
  const field = useRef<HTMLDivElement>(null);
  const keyboard = useKeyboardOpen();
  const q = query.trim();
  const found = useMemo(
    () => (q ? visibleItems(data.items, { kind: 'all' }, q, data.due) : []),
    [data.items, data.due, q],
  );
  const close = () => {
    setSearchOpen(false);
    setQuery('');
  };
  const nav: Nav = {
    open: (route) => openInTab(route),
    back: close,
    canBack: false,
    column: 'phone',
    selected: null,
    active: true,
  };
  useEffect(() => {
    if (android) field.current?.querySelector('input')?.focus();
  }, [android]);
  const recentItems = recent
    .map((id) => data.byId(id))
    .filter((item) => item && !item.deleted)
    .slice(0, 3);

  const body = !q ? (
    <>
      {recentItems.length > 0 && (
        <ListSection header={t('Zuletzt geöffnet')}>
          {recentItems.map((item) => item && <ItemRow key={item.id} item={item} />)}
        </ListSection>
      )}
      <ListSection header={t('Vorschläge')}>
        <div className="m-chips">
          {SUGGESTIONS.map((suggestion) => (
            <button
              key={suggestion.label}
              type="button"
              onClick={() => openInTab({ page: 'list', filter: suggestion.filter })}
            >
              <suggestion.icon aria-hidden />
              {t(suggestion.label)}
            </button>
          ))}
        </div>
      </ListSection>
    </>
  ) : found.length ? (
    <ListSection
      header={found.length === 1 ? t('1 Treffer') : t('{n} Treffer', { n: found.length })}
    >
      {found.map((item) => (
        <ItemRow key={item.id} item={item} query={q} />
      ))}
    </ListSection>
  ) : (
    <Empty title={t('Nichts gefunden')}>
      {t('Kein Eintrag passt zu „{query}“.', { query: q })}
    </Empty>
  );

  return (
    <NavContext.Provider value={nav}>
      <div className="m-search" ref={field}>
        <Screen
          title={t('Suche')}
          largeTitle={!android && !keyboard}
          className="m-search-screen"
          searchBar={
            android ? (
              <SearchBar
                value={query}
                onChange={setQuery}
                placeholder={t('Im Tresor suchen')}
                leading={<NavButton label={t('Zurück')} icon={ICONS.back} onClick={close} />}
                trailing={
                  query ? (
                    <NavButton
                      label={t('Suche leeren')}
                      icon={ICONS.close}
                      onClick={() => setQuery('')}
                    />
                  ) : undefined
                }
              />
            ) : undefined
          }
        >
          {body}
        </Screen>
      </div>
    </NavContext.Provider>
  );
}
