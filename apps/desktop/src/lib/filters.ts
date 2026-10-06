/**
 * Which items a list shows: all, the favourites, a type, a folder, an
 * organisation or one of its collections, the ones due for a new password,
 * the trash. Shared by the desktop's sidebar and the phone's overview, and
 * tested on its own (`apps/desktop/test/filters.test.ts`): no imports with
 * effects in here.
 */

import type { ItemKind, ItemSummary } from './api';

export type Filter =
  | { kind: 'all' }
  | { kind: 'favorites' }
  | { kind: 'type'; type: ItemKind }
  | { kind: 'folder'; id: string | null }
  | { kind: 'collection'; id: string }
  | { kind: 'organization'; id: string }
  | { kind: 'due' }
  | { kind: 'trash' };

export function matches(filter: Filter, item: ItemSummary, due: ReadonlySet<string>): boolean {
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

export function sameFilter(a: Filter, b: Filter): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/**
 * The items a list shows, by name: those of `filter`, or with a search every
 * item (but the trash's, unless that is the list) whose name, subtitle or
 * host has every word.
 */
export function visibleItems(
  items: readonly ItemSummary[],
  filter: Filter,
  query: string,
  due: ReadonlySet<string>,
): ItemSummary[] {
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
}

/** How many items each list has; the trash counts the deleted ones. */
export function countItems(items: readonly ItemSummary[], due: ReadonlySet<string>) {
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
}

export type Counts = ReturnType<typeof countItems>;
