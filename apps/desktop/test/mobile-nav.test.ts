// The phone's pages and the iPad's columns (src/mobile/nav.ts), and the lists
// they show (src/lib/filters.ts) — run by Node itself: node --test apps/desktop/test/

import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { ItemSummary } from '../src/lib/api.ts';
import { countItems, matches, sameFilter, visibleItems } from '../src/lib/filters.ts';
import {
  freshStacks,
  isListPage,
  openColumns,
  pop,
  popToRoot,
  prune,
  push,
  routeKey,
  tabOf,
} from '../src/mobile/nav.ts';

test('a tab starts on its root page', () => {
  const stacks = freshStacks();
  assert.deepEqual(stacks.vault, [{ page: 'overview' }]);
  assert.deepEqual(stacks.check, [{ page: 'check' }]);
  assert.deepEqual(stacks.generator, [{ page: 'generator' }]);
  assert.deepEqual(stacks.settings, [{ page: 'settings' }]);
});

test('push and pop keep the root page', () => {
  let stacks = freshStacks();
  stacks = push(stacks, 'vault', { page: 'list', filter: { kind: 'all' } });
  stacks = push(stacks, 'vault', { page: 'item', id: 'a' });
  assert.equal(stacks.vault.length, 3);
  assert.equal(stacks.check.length, 1, 'other tabs untouched');
  stacks = pop(stacks, 'vault');
  stacks = pop(stacks, 'vault');
  stacks = pop(stacks, 'vault');
  assert.deepEqual(stacks.vault, [{ page: 'overview' }]);
});

test('the same page twice in a row is pushed once', () => {
  let stacks = push(freshStacks(), 'vault', { page: 'item', id: 'a' });
  const again = push(stacks, 'vault', { page: 'item', id: 'a' });
  assert.equal(again, stacks);
  stacks = push(stacks, 'vault', { page: 'item', id: 'b' });
  assert.equal(stacks.vault.length, 3);
});

test('popToRoot goes back to the first page', () => {
  let stacks = push(freshStacks(), 'vault', { page: 'sends' });
  stacks = push(stacks, 'vault', { page: 'send', id: 's' });
  assert.deepEqual(popToRoot(stacks, 'vault').vault, [{ page: 'overview' }]);
  const root = freshStacks();
  assert.equal(popToRoot(root, 'vault'), root);
});

test('prune drops pages that are gone, never the root', () => {
  let stacks = push(freshStacks(), 'vault', { page: 'list', filter: { kind: 'all' } });
  stacks = push(stacks, 'vault', { page: 'item', id: 'gone' });
  const pruned = prune(stacks, (route) => route.page === 'item' && route.id === 'gone');
  assert.equal(pruned.vault.length, 2);
  assert.equal(prune(pruned, () => true).vault.length, 1, 'everything but the root can go');
  assert.equal(
    prune(pruned, () => false),
    pruned,
    'unchanged stacks stay the same object',
  );
});

test('route keys tell pages apart', () => {
  assert.notEqual(
    routeKey({ page: 'list', filter: { kind: 'folder', id: 'a' } }),
    routeKey({ page: 'list', filter: { kind: 'folder', id: 'b' } }),
  );
  assert.equal(routeKey({ page: 'item', id: 'x' }), 'item:x');
  assert.equal(routeKey({ page: 'settings-page', section: 'about' }), 'settings:about');
});

test('pages belong to their tab', () => {
  assert.equal(tabOf({ page: 'findings', group: 'weak' }), 'check');
  assert.equal(tabOf({ page: 'item', id: 'x' }), 'vault');
  assert.equal(tabOf({ page: 'settings-page', section: 'security' }), 'settings');
});

test('iPad: a list replaces the column, a detail opens beside it', () => {
  const start = { list: { page: 'list', filter: { kind: 'all' } }, detail: null } as const;
  const opened = openColumns(start, { page: 'item', id: 'a' });
  assert.deepEqual(opened.detail, { page: 'item', id: 'a' });
  assert.deepEqual(opened.list, start.list);
  const sends = openColumns(opened, { page: 'sends' });
  assert.deepEqual(sends, { list: { page: 'sends' }, detail: null });
  assert.equal(isListPage({ page: 'masked' }), true);
  assert.equal(isListPage({ page: 'send', id: 's' }), false);
});

const item = (over: Partial<ItemSummary>): ItemSummary => ({
  id: 'x',
  kind: 'login',
  name: 'x',
  subtitle: null,
  host: null,
  favorite: false,
  folderId: null,
  organizationId: null,
  collectionIds: [],
  deleted: false,
  reprompt: false,
  viewPassword: true,
  hasTotp: false,
  hasPassword: true,
  hasUsername: true,
  broken: false,
  revisionDate: null,
  ...over,
});

const ITEMS = [
  item({ id: 'a', name: 'Bank', favorite: true, folderId: 'f' }),
  item({ id: 'b', name: 'forum', subtitle: 'nyu@example.com', host: 'forum.example.org' }),
  item({ id: 'c', name: 'Card', kind: 'card', organizationId: 'o', collectionIds: ['k'] }),
  item({ id: 'd', name: 'Alt', deleted: true }),
];

test('filters pick their items', () => {
  const due = new Set(['b']);
  const ids = (filter: Parameters<typeof matches>[0]) =>
    ITEMS.filter((i) => matches(filter, i, due)).map((i) => i.id);
  assert.deepEqual(ids({ kind: 'all' }), ['a', 'b', 'c']);
  assert.deepEqual(ids({ kind: 'favorites' }), ['a']);
  assert.deepEqual(ids({ kind: 'due' }), ['b']);
  assert.deepEqual(ids({ kind: 'type', type: 'card' }), ['c']);
  assert.deepEqual(ids({ kind: 'folder', id: 'f' }), ['a']);
  assert.deepEqual(ids({ kind: 'folder', id: null }), ['b'], 'an organisation item has no folder');
  assert.deepEqual(ids({ kind: 'organization', id: 'o' }), ['c']);
  assert.deepEqual(ids({ kind: 'collection', id: 'k' }), ['c']);
  assert.deepEqual(ids({ kind: 'trash' }), ['d']);
  assert.equal(sameFilter({ kind: 'folder', id: 'f' }, { kind: 'folder', id: 'f' }), true);
});

test('search looks at every live item, sorted by name', () => {
  const none = new Set<string>();
  const found = visibleItems(ITEMS, { kind: 'favorites' }, 'example', none).map((i) => i.id);
  assert.deepEqual(found, ['b'], 'the host and subtitle count, the filter does not');
  const sorted = visibleItems(ITEMS, { kind: 'all' }, '', none).map((i) => i.name);
  assert.deepEqual(sorted, ['Bank', 'Card', 'forum']);
  assert.deepEqual(
    visibleItems(ITEMS, { kind: 'trash' }, 'alt', none).map((i) => i.id),
    ['d'],
    'searching the trash finds deleted items',
  );
});

test('counts', () => {
  const counts = countItems(ITEMS, new Set(['a', 'd']));
  assert.equal(counts.all, 3);
  assert.equal(counts.trash, 1);
  assert.equal(counts.due, 1, 'a deleted item is not due');
  assert.equal(counts.type('login'), 2);
  assert.equal(counts.folder(null), 1);
});
