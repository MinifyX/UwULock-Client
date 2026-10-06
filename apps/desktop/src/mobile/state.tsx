/**
 * What every page of the phone and iPad layout shares: the vault's items and
 * folders, the device, the tabs, opening pages and the app's sheets. Pages
 * read it with `useMobile()`; `useNav()` opens and closes pages in the column
 * (or phone stack) the page sits in.
 */

import { listen } from '@tauri-apps/api/event';
import { haptic, type MobilePlatform } from '@uwusuite/design';
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type RefObject,
} from 'react';
import {
  copyField,
  deleteItem,
  restoreItem,
  setFavorite,
  syncNow,
  vaultItems,
  vaultOverview,
  type ItemKind,
  type ItemSummary,
  type Overview,
  type Status,
} from '../lib/api';
import { toastError } from '../lib/errors';
import { countItems, type Counts } from '../lib/filters';
import { t } from '../lib/i18n';
import { getSettings, useSettings } from '../lib/settings';
import { note } from '../lib/toast';
import { loadIcons, useUwu } from '../lib/uwu';
import { playNyu } from '../components/nyu/stage';
import type { Route, Tab } from './nav';

// ── The vault's data ─────────────────────────────────────────────────────────

export type VaultData = {
  items: ItemSummary[];
  overview: Overview | null;
  /** Items whose new password is due (UwULock Server's reminders). */
  due: ReadonlySet<string>;
  counts: Counts;
  loaded: boolean;
  reload: () => Promise<void>;
  byId: (id: string) => ItemSummary | undefined;
};

export function useVaultData(status: Status): VaultData {
  const [items, setItems] = useState<ItemSummary[]>([]);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [loaded, setLoaded] = useState(false);
  const settings = useSettings();
  const uwu = useUwu();

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

  // Also on an account switch: the other account's items must not stay.
  useEffect(() => {
    void reload();
    const stop = listen('vault-changed', () => void reload());
    return () => void stop.then((unlisten) => unlisten());
  }, [reload, status.accountId]);

  const due = useMemo(
    () =>
      new Set(
        Object.entries(uwu.reminders)
          .filter(([, reminder]) => reminder.isDue)
          .map(([id]) => id),
      ),
    [uwu.reminders],
  );
  const counts = useMemo(() => countItems(items, due), [items, due]);

  // The icons of the live items, asked of Rust once the list is there.
  useEffect(() => {
    if (!uwu.uwu || !items.length) return;
    const timer = window.setTimeout(
      () =>
        void loadIcons(
          items
            .filter((item) => !item.deleted)
            .slice(0, 600)
            .map((item) => item.id),
          settings.siteIcons,
        ),
      150,
    );
    return () => window.clearTimeout(timer);
  }, [items, uwu.uwu, uwu.ownIcons, settings.siteIcons]);

  const byId = useCallback((id: string) => items.find((item) => item.id === id), [items]);

  return { items, overview, due, counts, loaded, reload, byId };
}

// ── Sheets the whole app opens ───────────────────────────────────────────────

export type AppSheet =
  /** "Neuer Eintrag": which kind. */
  | { kind: 'new' }
  /** The editor: an item (`id`) or a new one of `type`. */
  | { kind: 'edit'; id: string | null; type: ItemKind; folderId?: string | null }
  /** "Als Send teilen". */
  | { kind: 'share'; id: string }
  /** The account switcher. */
  | { kind: 'accounts' }
  /** A new folder, or a folder's new name. */
  | { kind: 'folder'; id: string | null; name: string }
  /** Into another folder. */
  | { kind: 'move'; id: string };

// ── The context ──────────────────────────────────────────────────────────────

export type DeviceLook = 'phone-ios' | 'phone-android' | 'ipad';

export type Mobile = {
  kind: DeviceLook;
  platform: MobilePlatform;
  ios: boolean;
  android: boolean;
  ipad: boolean;
  status: Status;
  data: VaultData;
  tab: Tab;
  /** Switch tabs; the tab that is already open goes back to its first page. */
  selectTab: (tab: Tab) => void;
  /** Opens `route` in its own tab (from search, from another tab). */
  openInTab: (route: Route) => void;
  sheet: AppSheet | null;
  openSheet: (sheet: AppSheet) => void;
  closeSheet: () => void;
  /** Pull to sync, the sync button. */
  sync: () => Promise<void>;
  /** The search (iOS: the tab bar's; Android: the search view; iPad: the list's field). */
  searchOpen: boolean;
  setSearchOpen: (open: boolean) => void;
  query: string;
  setQuery: (query: string) => void;
  onAddAccount: () => void;
};

export const MobileContext = createContext<Mobile | null>(null);

export function useMobile(): Mobile {
  const value = useContext(MobileContext);
  if (!value) throw new Error('useMobile outside <MobileApp>');
  return value;
}

/** Opening and closing pages from where a page sits: a phone stack or an iPad column. */
export type Nav = {
  open: (route: Route) => void;
  back: () => void;
  /** Whether this page has a page to go back to (shows the back button). */
  canBack: boolean;
  /** Where the page is drawn. */
  column: 'phone' | 'list' | 'detail' | 'single';
  /** The page open beside a list on an iPad, to mark its row. */
  selected: Route | null;
  /** The page below (phones), which the iOS edge swipe slides in. */
  underRef?: RefObject<HTMLElement | null>;
};

export const NavContext = createContext<Nav | null>(null);

export function useNav(): Nav {
  const value = useContext(NavContext);
  if (!value) throw new Error('useNav outside a page');
  return value;
}

// ── Actions every list and page uses ─────────────────────────────────────────

/** "Passwort kopiert", with when the clipboard is cleared as the second line. */
export function copiedNote(field: string): void {
  const seconds = getSettings().clipboardClear;
  const what =
    field === 'username'
      ? t('Benutzername kopiert')
      : field === 'password'
        ? t('Passwort kopiert')
        : field === 'totp' || field === 'totp-next'
          ? t('Code kopiert')
          : field.startsWith('uri:')
            ? t('Adresse kopiert')
            : t('Kopiert');
  note(what, {
    tone: 'success',
    detail: seconds > 0 ? t('Wird nach {n} s geleert', { n: seconds }) : undefined,
  });
}

/** Copies one value of an item through Rust (which clears the clipboard later). */
export async function copyItemField(id: string, field: string): Promise<void> {
  try {
    await copyField(id, field);
    haptic('success');
    copiedNote(field);
    playNyu('copied');
  } catch (e) {
    toastError(e);
  }
}

export async function toggleFavorite(item: ItemSummary): Promise<void> {
  try {
    await setFavorite(item.id, !item.favorite);
    haptic('light');
    note(item.favorite ? t('Kein Favorit mehr') : t('Favorit ✧'), { tone: 'success' });
  } catch (e) {
    toastError(e);
  }
}

/** Into the trash, with "Rückgängig" in the toast. */
export async function trashItem(item: ItemSummary): Promise<void> {
  try {
    await deleteItem(item.id, false);
    haptic('warning');
    playNyu('trashed');
    note(t('„{name}“ ist im Papierkorb', { name: item.name || t('(ohne Namen)') }), {
      action: {
        label: t('Rückgängig'),
        run: () => void restoreItem(item.id).catch((e) => toastError(e)),
      },
    });
  } catch (e) {
    toastError(e);
  }
}

export async function runSync(reload: () => Promise<void>): Promise<void> {
  try {
    const next = await syncNow();
    await reload();
    if (next.syncError) note(next.syncError, { tone: 'error' });
    else note(t('Synchronisiert'), { tone: 'success', detail: t('Alles aktuell') });
  } catch (e) {
    toastError(e);
  }
}
