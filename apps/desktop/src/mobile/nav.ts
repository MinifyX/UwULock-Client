/**
 * Where the phone and iPad layout are: one stack of pages per tab (iPhone,
 * Android) or the columns of a split view (iPad). Pure and tested on its own
 * (`apps/desktop/test/mobile-nav.test.ts`): no imports with effects in here.
 */

import type { Filter } from '../lib/filters';
import type { SuiteSpace } from '../lib/suiteModel';

export type Tab = 'vault' | 'check' | 'generator' | 'settings';

export const TABS: readonly Tab[] = ['vault', 'check', 'generator', 'settings'];

/** A group of the password check's findings, shown as a list of items. */
export type FindingGroup =
  'breached' | 'siteBreach' | 'reused' | 'weak' | 'unsecured' | 'twofa' | 'ignored';

export type SettingsPage = 'appearance' | 'security' | 'autofill' | 'account' | 'updates' | 'about';

export type Route =
  | { page: 'overview' }
  | { page: 'list'; filter: Filter }
  | { page: 'item'; id: string }
  | { page: 'sends' }
  | { page: 'send'; id: string }
  | { page: 'requests' }
  | { page: 'request'; id: string }
  | { page: 'masked' }
  | { page: 'suite'; space: SuiteSpace }
  | { page: 'check' }
  | { page: 'findings'; group: FindingGroup }
  | { page: 'review' }
  | { page: 'emails' }
  | { page: 'generator' }
  | { page: 'settings' }
  | { page: 'settings-page'; section: SettingsPage };

export type Stacks = Record<Tab, Route[]>;

const ROOT: Record<Tab, Route> = {
  vault: { page: 'overview' },
  check: { page: 'check' },
  generator: { page: 'generator' },
  settings: { page: 'settings' },
};

export function rootOf(tab: Tab): Route {
  return ROOT[tab];
}

export function freshStacks(): Stacks {
  return {
    vault: [ROOT.vault],
    check: [ROOT.check],
    generator: [ROOT.generator],
    settings: [ROOT.settings],
  };
}

/** A stable name for a page, for React keys and for "is this one open". */
export function routeKey(route: Route): string {
  switch (route.page) {
    case 'list':
      return `list:${JSON.stringify(route.filter)}`;
    case 'item':
    case 'send':
    case 'request':
      return `${route.page}:${route.id}`;
    case 'suite':
      return `suite:${route.space}`;
    case 'findings':
      return `findings:${route.group}`;
    case 'settings-page':
      return `settings:${route.section}`;
    default:
      return route.page;
  }
}

export const sameRoute = (a: Route, b: Route) => routeKey(a) === routeKey(b);

/** `route` on top of the tab's stack; the same page twice in a row stays once. */
export function push(stacks: Stacks, tab: Tab, route: Route): Stacks {
  const stack = stacks[tab];
  const top = stack[stack.length - 1];
  if (top && sameRoute(top, route)) return stacks;
  return { ...stacks, [tab]: [...stack, route] };
}

/** One page back; the root page stays. */
export function pop(stacks: Stacks, tab: Tab): Stacks {
  const stack = stacks[tab];
  if (stack.length < 2) return stacks;
  return { ...stacks, [tab]: stack.slice(0, -1) };
}

/** Back to the tab's first page (tapping the tab that is already open). */
export function popToRoot(stacks: Stacks, tab: Tab): Stacks {
  const stack = stacks[tab];
  if (stack.length < 2) return stacks;
  return { ...stacks, [tab]: stack.slice(0, 1) };
}

/** Pages that no longer exist (an item deleted, a Send gone) leave every stack. */
export function prune(stacks: Stacks, gone: (route: Route) => boolean): Stacks {
  let changed = false;
  const next = { ...stacks };
  for (const tab of TABS) {
    const kept = stacks[tab].filter((route, index) => index === 0 || !gone(route));
    if (kept.length !== stacks[tab].length) {
      next[tab] = kept;
      changed = true;
    }
  }
  return changed ? next : stacks;
}

/** Which tab a page belongs to when it is opened from elsewhere (search, a link). */
export function tabOf(route: Route): Tab {
  switch (route.page) {
    case 'check':
    case 'findings':
    case 'review':
    case 'emails':
      return 'check';
    case 'generator':
      return 'generator';
    case 'settings':
    case 'settings-page':
      return 'settings';
    default:
      return 'vault';
  }
}

// ── iPad ─────────────────────────────────────────────────────────────────────

/**
 * The iPad's vault: the list column (a list of items, Sends, file requests or
 * masked addresses, or a space of UwU-Apps) and what is open beside it.
 */
export type Columns = { list: Route; detail: Route | null };

/** Pages that fill the list column; everything else opens beside it. */
const LIST_PAGES = new Set<Route['page']>(['list', 'sends', 'requests', 'masked', 'suite']);

export function isListPage(route: Route): boolean {
  return LIST_PAGES.has(route.page);
}

/**
 * Opening `route` on an iPad: a list replaces the list column and closes the
 * detail; a detail opens beside the list.
 */
export function openColumns(columns: Columns, route: Route): Columns {
  if (isListPage(route)) return { list: route, detail: null };
  return { list: columns.list, detail: route };
}
