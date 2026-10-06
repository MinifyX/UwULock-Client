/**
 * One item in a list: tile, name, user name. Tapping opens it; swiping right
 * makes it a favourite, swiping left copies the password or throws it away;
 * a long press opens the menu with all of that and more.
 */

import {
  ContextMenu,
  ICONS,
  ListRow,
  SwipeRow,
  useLongPress,
  type ContextMenuEntry,
  type SwipeAction,
} from '@uwusuite/design';
import { useState, type ReactNode } from 'react';
import { openItemUri, type ItemSummary } from '../../lib/api';
import { useBackLayer } from '../../lib/backStack';
import { t, useLanguage } from '../../lib/i18n';
import { sendsAvailable } from '../../lib/sends';
import { toast } from '../../lib/toast';
import { useUwu } from '../../lib/uwu';
import { ItemTile } from '../../components/ItemTile';
import { copyItemField, toggleFavorite, trashItem, useMobile, useNav } from '../state';

/** `text` with the parts that match the search marked. */
export function Highlight({ text, query }: { text: string; query?: string }): ReactNode {
  const q = query?.trim().toLowerCase();
  if (!q) return text;
  const at = text.toLowerCase().indexOf(q);
  if (at < 0) return text;
  return (
    <>
      {text.slice(0, at)}
      <mark className="m-mark">{text.slice(at, at + q.length)}</mark>
      {text.slice(at + q.length)}
    </>
  );
}

/** What a tap on "Kopieren" copies for each kind. */
function mainField(item: ItemSummary): string | null {
  if (item.kind === 'login') return item.hasPassword && item.viewPassword ? 'password' : null;
  if (item.kind === 'note') return 'notes';
  if (item.kind === 'card') return 'card-number';
  return null;
}

/** The long-press menu of an item (also the "…" menu on its page). */
export function useItemMenu(item: ItemSummary | undefined): ContextMenuEntry[] {
  useLanguage();
  const { openSheet } = useMobile();
  const uwu = useUwu();
  if (!item) return [];
  if (item.deleted) return [];
  const login = item.kind === 'login';
  const entries: ContextMenuEntry[] = [];
  if (login) {
    if (item.hasPassword && item.viewPassword)
      entries.push({
        label: t('Passwort kopieren'),
        icon: ICONS.secret,
        onSelect: () => void copyItemField(item.id, 'password'),
      });
    if (item.hasUsername)
      entries.push({
        label: t('Benutzername kopieren'),
        icon: ICONS.account,
        onSelect: () => void copyItemField(item.id, 'username'),
      });
    if (item.hasTotp)
      entries.push({
        label: t('Code kopieren'),
        icon: ICONS.oneTimeCode,
        onSelect: () => void copyItemField(item.id, 'totp'),
      });
    if (item.host)
      entries.push({
        label: t('Website öffnen'),
        icon: ICONS.openExternal,
        onSelect: () => void openItemUri(item.id, 0).catch((e) => toast(String(e), 'error')),
      });
  } else {
    const field = mainField(item);
    if (field)
      entries.push({
        label: t('Kopieren'),
        icon: ICONS.copy,
        onSelect: () => void copyItemField(item.id, field),
      });
  }
  if (entries.length) entries.push('separator');
  entries.push(
    {
      label: item.favorite ? t('Aus Favoriten entfernen') : t('Zu Favoriten'),
      icon: ICONS.favorite,
      onSelect: () => void toggleFavorite(item),
    },
    {
      label: t('Bearbeiten'),
      icon: ICONS.edit,
      disabled: item.broken,
      onSelect: () => openSheet({ kind: 'edit', id: item.id, type: item.kind }),
    },
  );
  if (sendsAvailable(uwu))
    entries.push({
      label: t('Als Send teilen'),
      icon: ICONS.send,
      disabled: item.broken,
      onSelect: () => openSheet({ kind: 'share', id: item.id }),
    });
  if (!item.organizationId)
    entries.push({
      label: t('Verschieben'),
      icon: ICONS.move,
      onSelect: () => openSheet({ kind: 'move', id: item.id }),
    });
  entries.push('separator', {
    label: t('In den Papierkorb'),
    icon: ICONS.delete,
    danger: true,
    onSelect: () => void trashItem(item),
  });
  return entries;
}

export function ItemRow({ item, query }: { item: ItemSummary; query?: string }) {
  useLanguage();
  const nav = useNav();
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const longPress = useLongPress((point) => setMenu({ x: point.x, y: point.y }));
  useBackLayer(menu !== null, () => setMenu(null));
  const entries = useItemMenu(item);
  const field = mainField(item);
  const selected =
    nav.selected?.page === 'item' && nav.selected.id === item.id && nav.column !== 'phone';

  const leading: SwipeAction[] = item.deleted
    ? []
    : [
        {
          label: item.favorite ? t('Entfernen') : t('Favorit'),
          icon: ICONS.favorite,
          tone: 'warning',
          onSelect: () => void toggleFavorite(item),
        },
      ];
  const trailing: SwipeAction[] = item.deleted
    ? []
    : [
        ...(field
          ? [
              {
                label: t('Kopieren'),
                icon: ICONS.copy,
                tone: 'accent' as const,
                onSelect: () => void copyItemField(item.id, field),
              },
            ]
          : []),
        {
          label: t('Löschen'),
          icon: ICONS.delete,
          tone: 'danger',
          onSelect: () => void trashItem(item),
        },
      ];

  const name = item.name || t('(ohne Namen)');
  const row = (
    <ListRow
      icon={<ItemTile item={item} />}
      iconTone="none"
      title={
        <span className="m-row-title">
          <span>
            <Highlight text={name} query={query} />
          </span>
          {item.favorite && (
            <ICONS.favorite className="m-star" role="img" aria-label={t('Favorit')} />
          )}
          {item.reprompt && <ICONS.masterPassword role="img" aria-label={t('Geschützt')} />}
        </span>
      }
      subtitle={item.subtitle ? <Highlight text={item.subtitle} query={query} /> : undefined}
      selected={selected}
      onClick={() => nav.open({ page: 'item', id: item.id })}
      longPress={entries.length ? longPress : undefined}
      data-item={item.id}
    />
  );
  return (
    <>
      {leading.length || trailing.length ? (
        <SwipeRow leading={leading} trailing={trailing}>
          {row}
        </SwipeRow>
      ) : (
        row
      )}
      {entries.length > 0 && (
        <ContextMenu
          open={menu !== null}
          onClose={() => setMenu(null)}
          at={menu ?? undefined}
          label={t('Aktionen für {name}', { name })}
          preview={
            <div className="m-menu-preview">
              <ItemTile item={item} />
              <span>
                <b>{name}</b>
                {item.subtitle && <span>{item.subtitle}</span>}
              </span>
            </div>
          }
          items={entries}
        />
      )}
    </>
  );
}
