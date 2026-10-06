import type { ItemKind, ItemSummary } from '../lib/api';
import { useItemIcon } from '../lib/uwu';
import { Icon, ICONS } from '@uwusuite/design';
import type { LucideIcon } from 'lucide-react';

const KIND_ICON: Record<ItemKind, LucideIcon> = {
  login: ICONS.website,
  card: ICONS.card,
  identity: ICONS.identity,
  note: ICONS.note,
  'ssh-key': ICONS.sshKey,
  wifi: ICONS.wifi,
};

/** Six soft tile colours (the package's avatar pairs); each item keeps its own, by name. */
const HUES = 6;

function hue(text: string): number {
  let hash = 0;
  for (const char of text) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return hash % HUES;
}

/**
 * The little square in front of an item: its icon on UwULock Server (an own
 * one, or the site's, which the server fetches and Rust asks it for — never
 * the page, never the site itself); otherwise a login shows the first letter
 * of its name, everything else what kind it is.
 */
export function ItemTile({
  item,
  size = 'small',
}: {
  item: ItemSummary;
  size?: 'small' | 'large';
}) {
  const letter = (item.name || item.host || '?').trim().charAt(0).toUpperCase();
  const icon = useItemIcon(item.id);
  if (icon)
    return (
      <span className="item-tile" data-size={size} data-image aria-hidden>
        <img src={icon} alt="" draggable={false} />
      </span>
    );
  return (
    <span className="item-tile" data-size={size} data-hue={hue(item.name || item.id)} aria-hidden>
      {item.kind === 'login' ? (
        letter
      ) : (
        <Icon icon={KIND_ICON[item.kind]} size={size === 'large' ? 'xl' : 'sm'} />
      )}
    </span>
  );
}
