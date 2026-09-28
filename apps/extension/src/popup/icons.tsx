/**
 * Icons in front of items: what the background has for an item (its own icon, else the
 * server's automatic one, as a data URL), else the desktop app's tile with a letter or a
 * glyph. Asked for only when a row comes into view, a few rows at a time, and kept while the
 * popup is open.
 */

import { useEffect, useRef, useSyncExternalStore } from 'react';
import { ItemTile } from '@desktop/components/ItemTile';
import type { ItemSummary } from '../shared/protocol';
import { itemIcons } from './api';

const known = new Map<string, string | null>();
const queued = new Set<string>();
const listeners = new Set<() => void>();
let enabled = false;
let timer: number | undefined;
let version = 0;

/** Icons only where the server has them (UwULock Server) and the setting is on. */
export function setIconsEnabled(next: boolean) {
  if (next === enabled) return;
  enabled = next;
  known.clear();
  version += 1;
  for (const listener of listeners) listener();
}

async function flush() {
  timer = undefined;
  const ids = [...queued].slice(0, 60);
  for (const id of ids) queued.delete(id);
  if (queued.size) timer = window.setTimeout(() => void flush(), 0);
  let found: Record<string, string> = {};
  try {
    found = await itemIcons(ids);
  } catch {
    // Locked meanwhile, or the server is away: the glyphs stay.
  }
  for (const id of ids) known.set(id, found[id] ?? null);
  version += 1;
  for (const listener of listeners) listener();
}

function want(id: string) {
  if (!enabled || known.has(id) || queued.has(id)) return;
  queued.add(id);
  timer ??= window.setTimeout(() => void flush(), 30);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** An item's icon, or its tile while there is none. */
export function ItemIcon({
  item,
  size = 'small',
}: {
  item: ItemSummary;
  size?: 'small' | 'large';
}) {
  useSyncExternalStore(subscribe, () => version);
  const ref = useRef<HTMLSpanElement>(null);
  const url = known.get(item.id) ?? null;
  // Switched on after the row appeared (the status came later): look again.
  const on = enabled;
  useEffect(() => {
    const el = ref.current;
    if (!el || !on || known.has(item.id)) return;
    if (typeof IntersectionObserver === 'undefined') {
      want(item.id);
      return;
    }
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        want(item.id);
        observer.disconnect();
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [item.id, url, on]);
  return (
    <span ref={ref} className="item-icon-slot">
      {url ? (
        <img className="item-tile item-icon" data-size={size} src={url} alt="" aria-hidden />
      ) : (
        <ItemTile item={item} size={size} />
      )}
    </span>
  );
}
