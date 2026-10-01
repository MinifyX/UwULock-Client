/**
 * The sections "SSH (UwUSSH)" and "Remote Desktop (UwURDP)" without React or
 * Tauri: the records as Rust hands them over, how the list is grouped and
 * sorted, the copied command, and the edits that reorder hosts and groups.
 * Tested on its own (`apps/desktop/test/suite.test.ts`, plain `node --test`),
 * so no imports with effects in here.
 *
 * The payloads are UwUSSH's and UwURDP's JSON (snake_case; UwURDP's `rdp`
 * settings camelCase). The page reads the fields it shows and sends back only
 * the ones it changed — Rust lays them over the record, so fields of a newer
 * app stay.
 */

export type SuiteSpace = 'ssh' | 'rdp';

export type SuiteKind =
  'host' | 'group' | 'identity' | 'key' | 'snippet' | 'port_forward' | 'known_host' | 'secret';

export type Json = Record<string, unknown>;

export type SuiteRecord = {
  id: string;
  kind: SuiteKind;
  seq: number;
  updatedMs: number;
  /** The payload; `null` for a secret, or a record that didn't open. */
  data: Json | null;
  broken: boolean;
  /** Identities and keys: who points at them. */
  usedBy?: string[];
};

export type SuiteView = { space: SuiteSpace; exists: boolean; records: SuiteRecord[] };

export type SuiteOp =
  | { op: 'put'; id: string; kind: SuiteKind; seq?: number; patch: Json }
  | { op: 'secret'; id: string; text: string; seq?: number }
  | { op: 'delete'; id: string; seq?: number };

export const WORKSPACES = ['private', 'business'] as const;

export const AUTH_TYPES = ['password', 'key', 'agent', 'keyboard-interactive', 'cert'] as const;

/** A random UUID (v4), for a record made here. */
export function newId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  bytes[6] = (bytes[6]! & 0x0f) | 0x40;
  bytes[8] = (bytes[8]! & 0x3f) | 0x80;
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function str(data: Json | null | undefined, field: string): string {
  const value = data?.[field];
  return typeof value === 'string' ? value : '';
}

export function num(data: Json | null | undefined, field: string, fallback = 0): number {
  const value = data?.[field];
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

export function bool(data: Json | null | undefined, field: string, fallback = false): boolean {
  const value = data?.[field];
  return typeof value === 'boolean' ? value : fallback;
}

export function obj(data: Json | null | undefined, field: string): Json | null {
  const value = data?.[field];
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Json) : null;
}

/** A pointer at another record, or `null`. */
export function ref(data: Json | null | undefined, field: string): string | null {
  return str(data, field) || null;
}

const collator = new Intl.Collator(undefined, { sensitivity: 'base', numeric: true });

/** What a record is called in a list. */
export function titleOf(record: SuiteRecord): string {
  const d = record.data;
  switch (record.kind) {
    case 'host':
      return str(d, 'name') || str(d, 'address');
    case 'group':
    case 'port_forward':
      return str(d, 'name');
    case 'identity':
      return str(d, 'label') || str(d, 'username');
    case 'key':
    case 'snippet':
      return str(d, 'label');
    case 'known_host':
      return hostPort(str(d, 'address'), num(d, 'port', 22));
    case 'secret':
      return '';
  }
}

/** `host:port`, an IPv6 address in brackets. */
export function hostPort(address: string, port: number): string {
  return address.includes(':') && !address.startsWith('[')
    ? `[${address}]:${port}`
    : `${address}:${port}`;
}

/**
 * One line of text for the clipboard: control characters (also U+0085) and
 * Unicode line breaks out, so a pasted command can't run a second one.
 */
export function oneLine(text: string): string {
  return text.replace(/[\p{Cc}\u2028\u2029]/gu, '');
}

/** Words a shell takes as they are; anything else goes in single quotes. */
export function shellQuote(word: string): string {
  if (word === '') return "''";
  return /^[A-Za-z0-9@%+=:,./_\-[\]]+$/.test(word) ? word : `'${word.replace(/'/g, `'\\''`)}'`;
}

/** `ssh -p 2222 user@host`: what to type to reach a host. */
export function sshCommand(host: Json | null, identity: Json | null): string {
  const port = num(host, 'port', 22);
  const user = oneLine(str(identity, 'username'));
  const target = (user ? `${user}@` : '') + oneLine(str(host, 'address'));
  // A target starting with `-` would be one of ssh's options (`-oProxyCommand=…`).
  return [
    'ssh',
    ...(port !== 22 ? ['-p', String(port)] : []),
    ...(target.startsWith('-') ? ['--'] : []),
    shellQuote(target),
  ].join(' ');
}

/** `host:port` for a remote desktop client. */
export function rdpAddress(host: Json | null): string {
  return hostPort(oneLine(str(host, 'address')), num(host, 'port', 3389));
}

export function byId(records: SuiteRecord[]): Map<string, SuiteRecord> {
  return new Map(records.map((r) => [r.id, r]));
}

export function ofKind(records: SuiteRecord[], kind: SuiteKind): SuiteRecord[] {
  return records
    .filter((r) => r.kind === kind)
    .sort((a, b) => collator.compare(titleOf(a), titleOf(b)));
}

/** The login a host connects with: its own, else (UwURDP) its group's. */
export function loginOf(host: SuiteRecord, index: Map<string, SuiteRecord>): SuiteRecord | null {
  const own = index.get(ref(host.data, 'identity_id') ?? '');
  if (own?.kind === 'identity') return own;
  const group = index.get(ref(host.data, 'group_id') ?? '');
  const inherited = group && index.get(ref(group.data, 'identity_id') ?? '');
  return inherited?.kind === 'identity' ? inherited : null;
}

function byPosition(a: SuiteRecord, b: SuiteRecord): number {
  return (
    num(a.data, 'position') - num(b.data, 'position') ||
    collator.compare(titleOf(a), titleOf(b)) ||
    a.id.localeCompare(b.id)
  );
}

export type HostGroup = { group: SuiteRecord | null; hosts: SuiteRecord[] };
export type Workspace = { workspace: string; groups: HostGroup[] };

/** Words of a search, all of which must be somewhere in the host. */
function matches(host: SuiteRecord, login: SuiteRecord | null, words: string[]): boolean {
  if (!words.length) return true;
  const hay = [
    str(host.data, 'name'),
    str(host.data, 'address'),
    str(host.data, 'comment'),
    str(login?.data, 'username'),
  ]
    .join(' ')
    .toLowerCase();
  return words.every((w) => hay.includes(w));
}

export function words(query: string): string[] {
  return query.trim().toLowerCase().split(/\s+/).filter(Boolean);
}

/**
 * The hosts by workspace (private first, then business, then any a newer app
 * made), each by group in the dragged order; hosts without a group last.
 * Empty groups show only without a search.
 */
export function workspaces(records: SuiteRecord[], query = ''): Workspace[] {
  const index = byId(records);
  const w = words(query);
  const groups = records.filter((r) => r.kind === 'group' && r.data).sort(byPosition);
  const hosts = records.filter((r) => r.kind === 'host' && r.data).sort(byPosition);
  const names = new Set<string>(WORKSPACES);
  for (const r of [...groups, ...hosts]) names.add(str(r.data, 'workspace') || 'private');
  const order = [
    ...WORKSPACES,
    ...[...names].filter((n) => !(WORKSPACES as readonly string[]).includes(n)).sort(),
  ];
  const result: Workspace[] = [];
  for (const workspace of order) {
    const inWorkspace = (r: SuiteRecord) => (str(r.data, 'workspace') || 'private') === workspace;
    const shown = hosts.filter((h) => inWorkspace(h) && matches(h, loginOf(h, index), w));
    const list: HostGroup[] = [];
    for (const group of groups.filter(inWorkspace)) {
      const members = shown.filter((h) => ref(h.data, 'group_id') === group.id);
      if (members.length || !w.length) list.push({ group, hosts: members });
    }
    const known = new Set(groups.map((g) => g.id));
    const loose = shown.filter((h) => !known.has(ref(h.data, 'group_id') ?? ''));
    if (loose.length) list.push({ group: null, hosts: loose });
    if (list.length) result.push({ workspace, groups: list });
  }
  return result;
}

/**
 * The edits that move a host (or a group) one place up (`-1`) or down (`1`)
 * among its siblings: everyone gets the position of their new place, and only
 * those whose position changes are written.
 */
export function moveOps(siblings: SuiteRecord[], id: string, step: -1 | 1): SuiteOp[] {
  const list = [...siblings].sort(byPosition);
  const from = list.findIndex((r) => r.id === id);
  const to = from + step;
  if (from < 0 || to < 0 || to >= list.length) return [];
  const [moved] = list.splice(from, 1);
  list.splice(to, 0, moved!);
  return list.flatMap((r, position) =>
    num(r.data, 'position') === position && r.data?.position !== undefined
      ? []
      : [{ op: 'put' as const, id: r.id, kind: r.kind, seq: r.seq, patch: { position } }],
  );
}

/** A host's siblings: same workspace, same group. */
export function siblingsOf(records: SuiteRecord[], host: SuiteRecord): SuiteRecord[] {
  const ws = str(host.data, 'workspace') || 'private';
  if (host.kind === 'group')
    return records.filter(
      (r) => r.kind === 'group' && (str(r.data, 'workspace') || 'private') === ws,
    );
  const group = ref(host.data, 'group_id');
  return records.filter(
    (r) =>
      r.kind === 'host' &&
      (str(r.data, 'workspace') || 'private') === ws &&
      ref(r.data, 'group_id') === group,
  );
}

/** The next free position at the end of a list. */
export function nextPosition(siblings: SuiteRecord[]): number {
  return siblings.reduce((max, r) => Math.max(max, num(r.data, 'position') + 1), 0);
}

/** A host's port forwards, by name. */
export function tunnelsOf(records: SuiteRecord[], hostId: string): SuiteRecord[] {
  return ofKind(records, 'port_forward').filter((r) => ref(r.data, 'host_id') === hostId);
}

/** `-L 8080:localhost:80` or `-R …`, as ssh would take it. */
export function tunnelSpec(tunnel: Json | null): string {
  const flag =
    str(tunnel, 'kind') === 'remote' ? '-R' : str(tunnel, 'kind') === 'local' ? '-L' : '?';
  const bind = str(tunnel, 'bind_address');
  const listen = (bind ? `${bind}:` : '') + num(tunnel, 'bind_port');
  return `${flag} ${listen}:${str(tunnel, 'target_host')}:${num(tunnel, 'target_port')}`;
}
