/**
 * The icon library (UwULock Server §7.2) without React or Tauri: its index as
 * the server sends it, and the search over it — the same as the web vault's,
 * so both find the same icons. Tested on its own
 * (`apps/desktop/test/icons.test.ts`). The calls are in `uwu.ts`
 * (`iconLibrary`, `libraryIcon`).
 */

export type LibrarySource = {
  id: string;
  name: string;
  url: string;
  license: string;
  licenseUrl: string;
  attribution: string;
};

export type LibraryIcon = {
  source: string;
  id: string;
  name: string;
  /** `default`, and `light` / `dark` where the library has them. */
  variants: string[];
  aliases: string[];
};

export type IconLibrary = { updated: string; sources: LibrarySource[]; icons: LibraryIcon[] };

/** Icons whose name, id or aliases have every word of `query`, the best first. */
export function searchLibrary(index: IconLibrary, query: string, most = 60): LibraryIcon[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  const score = (icon: LibraryIcon) => {
    const name = icon.name.toLowerCase();
    const haystack = [name, icon.id, ...icon.aliases].join(' ').toLowerCase();
    if (!words.every((word) => haystack.includes(word))) return -1;
    return (name === words.join(' ') ? 0 : name.startsWith(words[0]!) ? 1 : 2) * 1000 + name.length;
  };
  return index.icons
    .map((icon) => ({ icon, rank: score(icon) }))
    .filter((found) => found.rank >= 0)
    .sort((a, b) => a.rank - b.rank)
    .slice(0, most)
    .map((found) => found.icon);
}

/**
 * The first label of a host in the home network, as an app's name:
 * `jellyfin` of `jellyfin.local`; none for an address or `localhost`. The
 * caller has checked the host is local (`isLocalHost`).
 */
export function localLabel(host: string | null | undefined): string | null {
  if (!host) return null;
  const bare = host.toLowerCase().replace(/^\[|\]$/g, '');
  if (bare.includes(':') || /^\d+\.\d+\.\d+\.\d+$/.test(bare)) return null;
  const label = bare.split('.')[0] ?? '';
  return /^[a-z0-9_-]{1,63}$/.test(label) && /[a-z]/.test(label) && label !== 'localhost'
    ? label
    : null;
}

/**
 * Library icons that fit an item in the home network: by the device's name
 * (`jellyfin.local`), then by the item's name — whole, else word by word
 * (`My Jellyfin`) — the best first, each once.
 */
export function suggestLibrary(
  index: IconLibrary,
  host: string | null,
  name: string,
  most = 6,
): LibraryIcon[] {
  const found = new Map<string, LibraryIcon>();
  const add = (query: string | null | undefined) => {
    if (!query?.trim()) return 0;
    const icons = searchLibrary(index, query, most);
    for (const icon of icons) found.set(`${icon.source}/${icon.id}`, icon);
    return icons.length;
  };
  add(localLabel(host)?.replace(/[-_]+/g, ' '));
  if (!add(name)) {
    for (const word of name.split(/\s+/).filter((word) => word.length >= 4)) add(word);
  }
  return [...found.values()].slice(0, most);
}

/** The host of an address as somebody typed it into an item (`nas.local:5000`, `https://…`). */
export function hostOf(address: string): string | null {
  const text = address.trim();
  if (!text) return null;
  try {
    return new URL(text.includes('://') ? text : `http://${text}`).hostname || null;
  } catch {
    return null;
  }
}
