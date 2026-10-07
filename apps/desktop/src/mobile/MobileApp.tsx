/**
 * UwULock on a phone (iOS, Android) and on an iPad: four tabs — Tresor,
 * Prüfung, Generator, Einstellungen — built from @uwusuite/design's mobile
 * components. A phone keeps one stack of pages per tab (nav.ts) and pushes
 * pages in from the right; an iPad shows columns instead (sidebar | list |
 * detail). The desktop layout (VaultScreen) is not used here at all.
 */

import {
  haptic,
  ICONS,
  MobileShell,
  MobileToaster,
  platformOf,
  prefersReducedMotion,
  SplitView,
  TabBar,
  useDeviceKind,
  useKeyboardShortcut,
  type TabItem,
} from '@uwusuite/design';
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import type { Status } from '../lib/api';
import { useBackLayer } from '../lib/backStack';
import { HANDED_OVER } from '../lib/credentialExchange';
import { t, useLanguage } from '../lib/i18n';
import { toasts } from '../lib/toast';
import {
  freshStacks,
  isListPage,
  pop,
  popToRoot,
  prune,
  push,
  rootOf,
  routeKey,
  sameRoute,
  tabOf,
  TABS,
  type Route,
  type Stacks,
  type Tab,
} from './nav';
import { renderPage } from './pages';
import { AppSheets } from './sheets/AppSheets';
import { SearchView } from './pages/Search';
import { VaultSidebar } from './pages/Overview';
import {
  MobileContext,
  NavContext,
  runSync,
  useVaultData,
  type AppSheet,
  type DeviceLook,
  type Mobile,
  type Nav,
} from './state';
import { BackLayer } from './ui';

/** The iPad's columns per tab: what the list column shows, and the pages beside it. */
type IpadColumns = Record<Exclude<Tab, 'generator'>, { list: Route; detail: Route[] }>;

const freshColumns = (): IpadColumns => ({
  vault: { list: { page: 'list', filter: { kind: 'all' } }, detail: [] },
  // As in the prototype: the breached passwords beside the report, the most urgent group.
  check: { list: rootOf('check'), detail: [{ page: 'findings', group: 'breached' }] },
  settings: {
    list: rootOf('settings'),
    detail: [{ page: 'settings-page', section: 'appearance' }],
  },
});

const POP_MS = 380;

export function MobileApp({ status, onAddAccount }: { status: Status; onAddAccount: () => void }) {
  useLanguage();
  const kind = useDeviceKind() as DeviceLook;
  const platform = platformOf(kind) ?? 'ios';
  const ipad = kind === 'ipad';
  const data = useVaultData(status);
  const portrait = usePortrait();

  const [tab, setTab] = useState<Tab>('vault');
  const [visited, setVisited] = useState<ReadonlySet<Tab>>(() => new Set<Tab>(['vault']));
  const [stacks, setStacks] = useState<Stacks>(freshStacks);
  const [columns, setColumns] = useState<IpadColumns>(freshColumns);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [sheet, setSheet] = useState<AppSheet | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState('');
  // The page that was just pushed (it slides in) and the one going back (it slides out).
  const [entering, setEntering] = useState<string | null>(null);
  const [leaving, setLeaving] = useState<{ tab: Tab; route: Route; depth: number } | null>(null);
  const stacksRef = useRef(stacks);
  stacksRef.current = stacks;
  const layers = useRef(new Map<string, HTMLDivElement | null>());

  // A new account: everything starts over.
  useEffect(() => {
    setStacks(freshStacks());
    setColumns(freshColumns());
    setSheet(null);
    setSearchOpen(false);
    setQuery('');
  }, [status.accountId]);

  // Items that are gone (deleted for good, or the sync took them) leave the stacks.
  useEffect(() => {
    if (!data.loaded) return;
    const gone = (route: Route) => route.page === 'item' && !data.byId(route.id);
    setStacks((now) => prune(now, gone));
    setColumns((now) => {
      let changed = false;
      const next = { ...now };
      for (const key of Object.keys(now) as (keyof IpadColumns)[]) {
        const detail = now[key].detail.filter((route) => !gone(route));
        if (detail.length !== now[key].detail.length) {
          next[key] = { ...now[key], detail };
          changed = true;
        }
      }
      return changed ? next : now;
    });
  }, [data.loaded, data.byId]);

  const selectTab = useCallback(
    (next: Tab) => {
      setSearchOpen(false);
      setSidebarOpen(false);
      setVisited((seen) => (seen.has(next) ? seen : new Set([...seen, next])));
      if (next === tab) setStacks((now) => popToRoot(now, next));
      setTab(next);
    },
    [tab],
  );

  // ── Phone stacks ───────────────────────────────────────────────────────────

  const pushPage = useCallback((into: Tab, route: Route) => {
    const before = stacksRef.current[into];
    const top = before[before.length - 1];
    if (top && sameRoute(top, route)) return;
    setEntering(`${into}:${before.length}:${routeKey(route)}`);
    setStacks((now) => push(now, into, route));
  }, []);

  const popPage = useCallback((from: Tab) => {
    const stack = stacksRef.current[from];
    if (stack.length < 2) return;
    const depth = stack.length - 1;
    const top = stack[depth]!;
    const layer = layers.current.get(`${from}:${depth}`);
    const screen = layer?.querySelector<HTMLElement>('.uwu-screen');
    // After an edge swipe the page has already left the screen.
    const swiped = Boolean(screen?.style.transform);
    setStacks((now) => pop(now, from));
    if (!swiped && !prefersReducedMotion()) {
      setLeaving({ tab: from, route: top, depth });
      window.setTimeout(() => setLeaving((now) => (now?.route === top ? null : now)), POP_MS);
    }
  }, []);

  // ── iPad columns ───────────────────────────────────────────────────────────

  const openColumn = useCallback(
    (inTab: keyof IpadColumns, from: 'list' | 'detail' | 'sidebar', route: Route) => {
      setColumns((now) => {
        const current = now[inTab];
        if (inTab === 'vault' && isListPage(route))
          return { ...now, vault: { list: route, detail: [] } };
        if (from === 'detail') {
          const top = current.detail[current.detail.length - 1];
          if (top && sameRoute(top, route)) return now;
          return { ...now, [inTab]: { ...current, detail: [...current.detail, route] } };
        }
        return { ...now, [inTab]: { ...current, detail: [route] } };
      });
      if (from === 'sidebar') setSidebarOpen(false);
    },
    [],
  );

  const openInTab = useCallback(
    (route: Route) => {
      const target = tabOf(route);
      setSearchOpen(false);
      setVisited((seen) => (seen.has(target) ? seen : new Set([...seen, target])));
      setTab(target);
      if (ipad) {
        if (target === 'generator') return;
        openColumn(target, 'list', route);
      } else if (!sameRoute(route, rootOf(target))) {
        pushPage(target, route);
      }
    },
    [ipad, openColumn, pushPage],
  );

  const sync = useCallback(() => runSync(data.reload), [data.reload]);

  // iOS 26: Apple Passwords handed credentials over and the person wants to see them.
  useEffect(() => {
    const open = () => openInTab({ page: 'settings-page', section: 'import' });
    window.addEventListener(HANDED_OVER, open);
    return () => window.removeEventListener(HANDED_OVER, open);
  }, [openInTab]);

  const mobile: Mobile = useMemo(
    () => ({
      kind,
      platform,
      ios: platform === 'ios',
      android: platform === 'android',
      ipad,
      status,
      data,
      tab,
      selectTab,
      openInTab,
      sheet,
      openSheet: setSheet,
      closeSheet: () => setSheet(null),
      sync,
      searchOpen,
      setSearchOpen,
      query,
      setQuery,
      onAddAccount,
    }),
    [
      kind,
      platform,
      ipad,
      status,
      data,
      tab,
      selectTab,
      openInTab,
      sheet,
      sync,
      searchOpen,
      query,
      onAddAccount,
    ],
  );

  // iPad keyboard: ⌘F searches the list, ⌘N makes a new item.
  useKeyboardShortcut(
    'CmdOrCtrl+F',
    () => {
      setTab('vault');
      setSearchOpen(true);
      window.setTimeout(
        () => document.querySelector<HTMLInputElement>('.m-ipad-search input')?.focus(),
        0,
      );
    },
    { enabled: ipad && !sheet },
  );
  useKeyboardShortcut('CmdOrCtrl+N', () => setSheet({ kind: 'new' }), { enabled: ipad && !sheet });

  useBackLayer(searchOpen && !ipad, () => setSearchOpen(false));

  const tabs: TabItem<Tab>[] = [
    { id: 'vault', label: t('Tresor'), icon: ICONS.vault },
    { id: 'check', label: t('Prüfung'), icon: ICONS.securityCheck },
    { id: 'generator', label: t('Generator'), icon: ICONS.generate },
    { id: 'settings', label: t('Einstellungen'), icon: ICONS.settings },
  ];

  // ── Drawing ────────────────────────────────────────────────────────────────

  const phoneTab = (which: Tab) => {
    const stack = stacks[which];
    const shown =
      leaving && leaving.tab === which && leaving.depth === stack.length
        ? [...stack, leaving.route]
        : stack;
    return (
      <div key={which} className="m-tab" hidden={which !== tab}>
        {shown.map((route, depth) => {
          const id = `${which}:${depth}`;
          const isLeaving = depth === stack.length;
          const top = depth === stack.length - 1;
          const nav: Nav = {
            open: (next) => pushPage(which, next),
            back: () => popPage(which),
            canBack: depth > 0,
            column: 'phone',
            selected: null,
            active: which === tab && top && !searchOpen,
            underRef: {
              get current() {
                return layers.current.get(`${which}:${depth - 1}`) ?? null;
              },
            },
          };
          const pushed = entering === `${which}:${depth}:${routeKey(route)}`;
          return (
            <div
              key={`${depth}:${routeKey(route)}`}
              ref={(el) => {
                if (isLeaving) return;
                layers.current.set(id, el);
              }}
              className={
                isLeaving
                  ? 'm-layer uwu-pop-out'
                  : pushed && top
                    ? 'm-layer uwu-push-in'
                    : 'm-layer'
              }
              data-platform={platform}
              data-depth={depth}
              data-below={
                !top && !isLeaving ? (depth === stack.length - 2 ? 'near' : 'far') : undefined
              }
              // React 18 doesn't know `inert` yet; the pages below can't be reached.
              {...(!top || isLeaving ? { inert: '' } : {})}
              onAnimationEnd={(event) => {
                if (event.target === event.currentTarget && pushed) setEntering(null);
              }}
            >
              <NavContext.Provider value={nav}>
                {depth > 0 && !isLeaving && (
                  <BackLayer open={nav.active} close={() => popPage(which)} />
                )}
                {renderPage(route)}
              </NavContext.Provider>
            </div>
          );
        })}
      </div>
    );
  };

  const column = (
    inTab: keyof IpadColumns,
    where: 'list' | 'detail',
    route: Route | null,
    depth: number,
    empty?: ReactNode,
  ) => {
    const current = columns[inTab];
    const nav: Nav = {
      open: (next) => openColumn(inTab, where, next),
      back: () =>
        setColumns((now) => ({
          ...now,
          [inTab]: { ...now[inTab], detail: now[inTab].detail.slice(0, -1) },
        })),
      canBack: where === 'detail' && depth > 0,
      column: where,
      selected: current.detail[0] ?? null,
      active: true,
    };
    if (!route) return empty ?? <IpadEmpty />;
    return (
      <NavContext.Provider value={nav}>
        <div key={routeKey(route)} className="m-layer m-column-page">
          {renderPage(route)}
        </div>
      </NavContext.Provider>
    );
  };

  const ipadTab = (which: Tab) => {
    if (which === 'generator') {
      const nav: Nav = {
        open: () => undefined,
        back: () => undefined,
        canBack: false,
        column: 'single',
        selected: null,
        active: true,
      };
      return (
        <NavContext.Provider value={nav}>
          <div className="m-layer m-ipad-single">{renderPage(rootOf('generator'))}</div>
        </NavContext.Provider>
      );
    }
    const current = columns[which];
    const detailTop = current.detail[current.detail.length - 1] ?? null;
    const detail = column(which, 'detail', detailTop, current.detail.length - 1);
    if (which === 'vault') {
      const suite = current.list.page === 'suite';
      const sidebarNav: Nav = {
        open: (next) => openColumn('vault', 'sidebar', next),
        back: () => undefined,
        canBack: false,
        column: 'list',
        selected: current.list,
        active: true,
      };
      return (
        <SplitView
          sidebarOpen={sidebarOpen}
          onSidebarOpenChange={setSidebarOpen}
          sidebar={
            <NavContext.Provider value={sidebarNav}>
              <VaultSidebar onClose={() => setSidebarOpen(false)} current={current.list} />
            </NavContext.Provider>
          }
          list={suite ? undefined : column('vault', 'list', current.list, 0)}
          detail={suite ? column('vault', 'list', current.list, 0) : detail}
        />
      );
    }
    return (
      <SplitView listWidth={440} list={column(which, 'list', current.list, 0)} detail={detail} />
    );
  };

  return (
    <MobileContext.Provider value={mobile}>
      <MobileShell kind={kind} className="m-app">
        <div className="m-tabs" data-searching={searchOpen && !ipad ? '' : undefined}>
          {ipad
            ? ipadTab(tab)
            : TABS.filter((which) => visited.has(which)).map((which) => phoneTab(which))}
        </div>
        {searchOpen && !ipad && <SearchView />}
        {!(searchOpen && platform === 'android') && (
          <TabBar
            tabs={tabs}
            value={tab}
            onChange={selectTab}
            label={t('Bereiche')}
            search={
              platform === 'ios'
                ? {
                    open: searchOpen,
                    onOpenChange: (open) => {
                      if (open) haptic('selection');
                      setSearchOpen(open);
                      if (!open) setQuery('');
                    },
                    value: query,
                    onChange: setQuery,
                    placeholder: t('Tresor durchsuchen'),
                  }
                : undefined
            }
          />
        )}
        {ipad && portrait && tab === 'vault' && !sidebarOpen && (
          <IpadSidebarButton onClick={() => setSidebarOpen(true)} />
        )}
        <AppSheets />
        <MobileToaster store={toasts} />
      </MobileShell>
    </MobileContext.Provider>
  );
}

/** Nothing chosen beside the list yet. */
function IpadEmpty() {
  useLanguage();
  return (
    <div className="m-ipad-empty">
      <ICONS.masterPassword aria-hidden />
      <b>{t('Nichts ausgewählt')}</b>
      <span>{t('Wähle links einen Eintrag.')}</span>
    </div>
  );
}

/** iPad portrait: the sidebar is an overlay, opened from here. */
function IpadSidebarButton({ onClick }: { onClick: () => void }) {
  useLanguage();
  return (
    <button
      type="button"
      className="m-ipad-sidebar-button uwu-glass"
      aria-label={t('Seitenleiste zeigen')}
      onClick={onClick}
    >
      <ICONS.sidebar aria-hidden />
    </button>
  );
}

function subscribeOrientation(listener: () => void) {
  const query = window.matchMedia?.('(orientation: portrait)');
  query?.addEventListener('change', listener);
  return () => query?.removeEventListener('change', listener);
}

/** Portrait, as SplitView sees it (its sidebar is an overlay then). */
function usePortrait(): boolean {
  return useSyncExternalStore(
    subscribeOrientation,
    () => window.matchMedia?.('(orientation: portrait)').matches ?? false,
    () => false,
  );
}
