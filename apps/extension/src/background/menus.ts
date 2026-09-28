/**
 * The context menu, the keyboard shortcut and the badge.
 *
 * Right-click in a page: fill one of the tab's logins, copy its password, fill a card or an
 * address, or generate a password (copied, and typed into the field that was clicked). The
 * lists follow the active tab; `Ctrl+Shift+L` fills its best login. The badge counts the tab's
 * logins, like Bitwarden's.
 */

import { ext } from '../shared/browser';
import { t, N_, resolveLanguage, setLanguage } from '../shared/i18n';
import { isFillableUrl } from '../shared/uri';
import { fillBest, matchingLogins, offer, tabUrl } from './autofill';
import * as clipboard from './clipboard';
import { generate } from './generator';
import * as session from './session';
import { settings } from './settings';
import * as vault from './vault';

const ROOT = 'uwulock';
const MAX_ITEMS = 8;

type Entry = { id: string; parentId?: string; title: string; enabled?: boolean };

let building: Promise<void> = Promise.resolve();
let shownFor: number | null = null;

async function activeTab(): Promise<chrome.tabs.Tab | undefined> {
  const [tab] = await ext.tabs.query({ active: true, lastFocusedWindow: true }).catch(() => []);
  return tab;
}

/** Build the menu for the active tab again. */
export function refreshMenus(): Promise<void> {
  building = building.then(build, build);
  return building;
}

async function build() {
  const config = await settings();
  setLanguage(resolveLanguage(config.language));
  const tab = await activeTab();
  const url = tab?.id !== undefined ? (tab.url ?? tabUrl(tab.id)) : undefined;
  shownFor = tab?.id ?? null;
  const unlocked = Boolean(session.unlockedAccountId());
  const logins = unlocked && url && isFillableUrl(url) ? await matchingLogins(url) : [];
  const index = unlocked ? vault.autofillIndex() : [];
  const cards = index.filter((e) => e.kind === 'card').slice(0, MAX_ITEMS);
  const identities = index.filter((e) => e.kind === 'identity').slice(0, MAX_ITEMS);

  const entries: Entry[] = [{ id: ROOT, title: 'UwULock' }];
  const list = (parent: string, prefix: string, items: typeof logins, empty: string) => {
    entries.push({ id: parent, parentId: ROOT, title: t(prefix) });
    if (items.length === 0) {
      entries.push({ id: `${parent}:none`, parentId: parent, title: t(empty), enabled: false });
      return;
    }
    for (const item of items.slice(0, MAX_ITEMS)) {
      const label = item.subtitle ? `${item.name} (${item.subtitle})` : item.name;
      entries.push({ id: `${parent}:${item.id}`, parentId: parent, title: label });
    }
  };
  if (!unlocked) {
    entries.push({ id: 'open', parentId: ROOT, title: t('UwULock entsperren') });
  } else {
    list('fill', N_('Login ausfüllen'), logins, N_('Keine Logins für diese Seite'));
    list('copy-password', N_('Passwort kopieren'), logins, N_('Keine Logins für diese Seite'));
    list('fill-card', N_('Karte ausfüllen'), cards, N_('Keine Karten'));
    list('fill-identity', N_('Adresse ausfüllen'), identities, N_('Keine Identitäten'));
  }
  entries.push({ id: 'generate', parentId: ROOT, title: t('Passwort generieren (kopiert)') });

  await ext.contextMenus.removeAll();
  for (const entry of entries) {
    ext.contextMenus.create(
      {
        id: entry.id,
        parentId: entry.parentId,
        title: entry.title,
        enabled: entry.enabled ?? true,
        contexts: ['page', 'editable', 'frame', 'selection'],
      },
      () => void ext.runtime.lastError,
    );
  }
  await updateBadge(tab?.id, logins.length);
}

async function updateBadge(tabId: number | undefined, count: number) {
  if (tabId === undefined) return;
  await ext.action
    .setBadgeText({ tabId, text: count > 0 ? String(Math.min(count, 99)) : '' })
    .catch(() => undefined);
  await ext.action.setBadgeBackgroundColor({ color: '#e11d74' }).catch(() => undefined);
}

export async function onMenuClick(info: chrome.contextMenus.OnClickData, tab?: chrome.tabs.Tab) {
  const id = String(info.menuItemId);
  if (id === 'open') {
    await session.openPopup();
    return;
  }
  if (id === 'generate') {
    const password = await generate();
    await clipboard.copy(password);
    return;
  }
  const [action, itemId] = id.split(':');
  if (!itemId || itemId === 'none' || tab?.id === undefined) return;
  if (!session.unlockedAccountId()) {
    await session.openPopup();
    return;
  }
  if (action === 'copy-password') {
    const password = await vault.reveal(itemId, 'password').catch(() => null);
    if (password) await clipboard.copy(password);
    return;
  }
  // A pick from the menu is a pick like in the popup: the tab's page itself may have it.
  if (action === 'fill' || action === 'fill-card' || action === 'fill-identity') {
    await offer(tab.id, itemId, action !== 'fill');
  }
}

export async function onCommand(command: string, tab?: chrome.tabs.Tab) {
  if (command !== 'autofill-login') return;
  const target = tab ?? (await activeTab());
  if (target) await fillBest(target);
}

/** Another tab became active: its lists, if they aren't already shown. */
export function onTabActivated(tabId: number) {
  if (tabId !== shownFor) void refreshMenus();
}
