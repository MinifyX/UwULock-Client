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

/**
 * How many items each list has; the trash counts the deleted ones. One pass
 * over the items: the sidebar asks once per folder, collection and type.
 */
export function countItems(items: readonly ItemSummary[], due: ReadonlySet<string>) {
  const bump = <K>(map: Map<K, number>, key: K) => map.set(key, (map.get(key) ?? 0) + 1);
  const types = new Map<ItemKind, number>();
  const folders = new Map<string | null, number>();
  const collections = new Map<string, number>();
  const organizations = new Map<string, number>();
  let all = 0;
  let favorites = 0;
  let dueCount = 0;
  for (const item of items) {
    if (item.deleted) continue;
    all += 1;
    if (item.favorite) favorites += 1;
    if (due.has(item.id)) dueCount += 1;
    bump(types, item.kind);
    if (item.organizationId) bump(organizations, item.organizationId);
    else bump(folders, item.folderId ?? null);
    for (const id of item.collectionIds) bump(collections, id);
  }
  return {
    all,
    favorites,
    trash: items.length - all,
    type: (type: ItemKind) => types.get(type) ?? 0,
    folder: (id: string | null) => folders.get(id) ?? 0,
    collection: (id: string) => collections.get(id) ?? 0,
    organization: (id: string) => organizations.get(id) ?? 0,
    due: dueCount,
  };
}

export type Counts = ReturnType<typeof countItems>;
