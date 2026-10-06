import { listen } from '@tauri-apps/api/event';
import { Button, Icon, IconButton, ICONS } from '@uwusuite/design';
import type { LucideIcon } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import { copyGenerated, failure } from '../lib/api';
import { errorText, toastError } from '../lib/errors';
import { copiedText } from '../lib/format';
import { N_, t, useLanguage } from '../lib/i18n';
import { isMobile } from '../lib/platform';
import { getSettings } from '../lib/settings';
import {
  suiteCopy,
  suiteCreate,
  suiteOpenInApp,
  suiteReveal,
  suiteSave,
  suiteSaveKey,
  suiteSaveRdp,
  suiteView,
  type SuiteSaved,
} from '../lib/suite';
import {
  bool,
  byId,
  hostPort,
  loginOf,
  moveOps,
  num,
  obj,
  ofKind,
  rdpAddress,
  ref,
  siblingsOf,
  sshCommand,
  str,
  titleOf,
  tunnelSpec,
  tunnelsOf,
  words,
  workspaces,
  type SuiteKind,
  type SuiteOp,
  type SuiteRecord,
  type SuiteSpace,
  type SuiteView,
} from '../lib/suiteModel';
import { toast } from '../lib/toast';
import { ContextMenu } from './ContextMenu';
import { Modal } from './Modal';
import { NyuScene } from './nyu/scenes';
import {
  authLabel,
  GroupEditor,
  HostEditor,
  IdentityEditor,
  KeyEditor,
  NewKeyDialog,
  SnippetEditor,
  TunnelEditor,
  workspaceLabel,
  type EditorTarget,
  type SuiteCtx,
} from './SuiteEditors';

type Tab = 'host' | 'identity' | 'key' | 'snippet' | 'known_host';

const TABS: { tab: Tab; label: string; icon: LucideIcon }[] = [
  { tab: 'host', label: N_('Hosts'), icon: ICONS.computer },
  { tab: 'identity', label: N_('Anmeldungen'), icon: ICONS.account },
  { tab: 'key', label: N_('SSH-Schlüssel'), icon: ICONS.sshKey },
  { tab: 'snippet', label: N_('Snippets'), icon: ICONS.terminal },
  { tab: 'known_host', label: N_('Bekannte Hosts'), icon: ICONS.fingerprint },
];

export const SPACE_TITLE: Record<SuiteSpace, string> = {
  ssh: N_('SSH (UwUSSH)'),
  rdp: N_('Remote Desktop (UwURDP)'),
};

const APP: Record<SuiteSpace, string> = { ssh: 'UwUSSH', rdp: 'UwURDP' };

type Props = {
  space: SuiteSpace;
  phone: boolean;
  /** Phone layout: a record is open over the list. */
  detailOpen: boolean;
  onDetail: (open: boolean) => void;
  onMenu: () => void;
  searchRef: React.RefObject<HTMLInputElement>;
};

/**
 * One section: UwUSSH's or UwURDP's hosts (by workspace and group), logins,
 * keys, snippets and known hosts, with their editors. The records come from
 * Rust (`suite_view`), again when another device changes them (`suite-changed`).
 */
export function SuitePane({ space, phone, detailOpen, onDetail, onMenu, searchRef }: Props) {
  useLanguage();
  const [view, setView] = useState<SuiteView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [tab, setTab] = useState<Tab>('host');
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [editing, setEditing] = useState<EditorTarget | null>(null);
  const [newKey, setNewKey] = useState(false);
  const [newMenu, setNewMenu] = useState<{ x: number; y: number } | null>(null);

  const load = useCallback(async () => {
    try {
      setView(await suiteView(space));
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, [space]);

  useEffect(() => {
    setView(null);
    setSelected(null);
    setTab('host');
    setQuery('');
    void load();
    const stop = listen('suite-changed', () => void load());
    return () => void stop.then((unlisten) => unlisten());
  }, [load]);

  const records = useMemo(() => view?.records ?? [], [view]);
  const index = useMemo(() => byId(records), [records]);

  const took = useCallback((saved: SuiteSaved): boolean => {
    setView(saved.view);
    if (saved.conflicts.length) {
      toast(
        t(
          'Das wurde inzwischen woanders geändert. UwULock hat neu geladen und nichts überschrieben – ändere es bitte noch einmal.',
        ),
      );
      return false;
    }
    return true;
  }, []);

  const save = useCallback(
    async (ops: SuiteOp[], done?: string): Promise<boolean> => {
      if (!ops.length) return true;
      try {
        const ok = took(await suiteSave(space, ops));
        if (ok && done) toast(done);
        return ok;
      } catch (e) {
        const kind = failure(e).kind;
        if (kind === 'suite-conflict' || kind === 'space-changed') {
          toast(errorText(e));
          await load();
        } else {
          toastError(e);
        }
        return false;
      }
    },
    [space, took, load],
  );

  const ctx: SuiteCtx = { space, records, index, save, took };

  const tabs = TABS.filter(
    (x) =>
      x.tab === 'host' ||
      x.tab === 'identity' ||
      space === 'ssh' ||
      records.some((r) => r.kind === x.tab),
  );

  const flat = useMemo(() => {
    if (tab === 'host') return [];
    const w = words(query);
    return ofKind(records, tab).filter((r) => {
      if (!w.length) return true;
      const hay =
        `${titleOf(r)} ${str(r.data, 'username')} ${str(r.data, 'body')} ${str(r.data, 'fingerprint_sha256')}`.toLowerCase();
      return w.every((x) => hay.includes(x));
    });
  }, [records, tab, query]);
  const grouped = useMemo(
    () => (tab === 'host' ? workspaces(records, query) : []),
    [records, tab, query],
  );

  const current = selected ? (index.get(selected) ?? null) : null;
  const show = (id: string) => {
    setSelected(id);
    onDetail(true);
  };
  const title = t(SPACE_TITLE[space]);

  const create = async () => {
    setCreating(true);
    try {
      setView(await suiteCreate(space));
    } catch (e) {
      toastError(e);
    } finally {
      setCreating(false);
    }
  };

  const openEditor = (kind: Tab | 'group') => {
    if (kind === 'key') setNewKey(true);
    else if (kind !== 'known_host') setEditing({ kind, record: null });
  };

  const newItems: { kind: Tab | 'group'; label: string; icon: LucideIcon }[] = [
    { kind: 'host', label: t('Host'), icon: ICONS.computer },
    { kind: 'group', label: t('Gruppe'), icon: ICONS.folder },
    { kind: 'identity', label: t('Anmeldung'), icon: ICONS.account },
    ...(space === 'ssh'
      ? [
          { kind: 'key' as const, label: t('Schlüssel'), icon: ICONS.sshKey },
          { kind: 'snippet' as const, label: t('Snippet'), icon: ICONS.terminal },
        ]
      : []),
  ];

  const row = (record: SuiteRecord, sub: ReactNode, icon: LucideIcon, indent = false) => (
    <li
      key={record.id}
      data-id={record.id}
      role="option"
      aria-selected={record.id === selected}
      className="item-row"
      data-indent={indent || undefined}
      onClick={() => show(record.id)}
    >
      <span className="item-tile" aria-hidden>
        <Icon icon={icon} size="sm" />
      </span>
      <span className="item-text">
        <span className="item-name">
          {record.broken ? t('(lässt sich nicht öffnen)') : titleOf(record) || t('(ohne Namen)')}
        </span>
        {sub && <span className="item-sub">{sub}</span>}
      </span>
      {record.broken && (
        <span className="item-badges">
          <Icon icon={ICONS.warning} size="xs" className="badge-warning" />
        </span>
      )}
    </li>
  );

  const hostSub = (host: SuiteRecord) => {
    const login = loginOf(host, index);
    const user = str(login?.data, 'username');
    const at = hostPort(
      str(host.data, 'address'),
      num(host.data, 'port', space === 'rdp' ? 3389 : 22),
    );
    return user ? `${user}@${at}` : at;
  };

  const subOf = (r: SuiteRecord): string => {
    switch (r.kind) {
      case 'identity':
        return (
          [str(r.data, 'domain'), str(r.data, 'username')].filter(Boolean).join('\\') +
          ` · ${authLabel(str(r.data, 'auth_type'))}`
        );
      case 'key':
        return str(r.data, 'key_type');
      case 'snippet':
        return str(r.data, 'group_path') || str(r.data, 'body').split('\n')[0] || '';
      case 'known_host':
        return str(r.data, 'fingerprint_sha256');
      default:
        return '';
    }
  };

  const empty = !records.some((r) => r.kind !== 'secret');

  const list = !view ? (
    <div className="list-empty">{error ? <p>{error}</p> : <p>{t('Lädt …')}</p>}</div>
  ) : !view.exists ? (
    <div className="list-empty">
      <NyuScene name="pick" className="empty-scene" />
      <p>
        {t(
          'Hier ist noch nichts von {app}. Sobald {app} mit UwULock synchronisiert, erscheint alles hier – oder du fängst hier an.',
          {
            app: APP[space],
          },
        )}
      </p>
      <Button variant="primary" disabled={creating} onClick={() => void create()}>
        {creating ? t('Einen Moment …') : t('Bereich anlegen')}
      </Button>
    </div>
  ) : tab === 'host' ? (
    grouped.length ? (
      <ul className="item-list" role="listbox" aria-label={title}>
        {grouped.map((ws) => (
          <li key={ws.workspace} className="not-first:mt-3" role="presentation">
            <h3 className="mx-3 mt-2 mb-1 text-caption font-bold tracking-[0.06em] text-muted uppercase">
              {workspaceLabel(ws.workspace)}
            </h3>
            <ul role="presentation">
              {ws.groups.map((g) => (
                <li key={g.group?.id ?? 'loose'} role="presentation">
                  {g.group ? (
                    <button
                      type="button"
                      className="suite-group"
                      aria-current={g.group.id === selected || undefined}
                      onClick={() => show(g.group!.id)}
                    >
                      <Icon icon={ICONS.folder} size="xs" />
                      <span className="nav-label">{titleOf(g.group) || t('(ohne Namen)')}</span>
                      <span className="nav-count">{g.hosts.length}</span>
                    </button>
                  ) : (
                    <p className="suite-group" data-loose>
                      {t('Ohne Gruppe')}
                    </p>
                  )}
                  <ul role="presentation">
                    {g.hosts.map((h) =>
                      row(
                        h,
                        hostSub(h),
                        space === 'rdp' ? ICONS.computer : ICONS.terminal,
                        Boolean(g.group),
                      ),
                    )}
                  </ul>
                </li>
              ))}
            </ul>
          </li>
        ))}
      </ul>
    ) : (
      <EmptyList query={query} empty={empty} />
    )
  ) : flat.length ? (
    <ul className="item-list" role="listbox" aria-label={title}>
      {flat.map((r) => row(r, subOf(r), TABS.find((x) => x.tab === tab)?.icon ?? ICONS.sshKey))}
    </ul>
  ) : (
    <EmptyList query={query} empty={empty} />
  );

  return (
    <>
      <section className="list-pane" aria-label={title}>
        <div className="list-head">
          <div className="search-row">
            {phone && (
              <IconButton
                icon={ICONS.menu}
                className="menu-button"
                label={t('Ordner und Typen')}
                onClick={onMenu}
              />
            )}
            <label className="search-box">
              <Icon icon={ICONS.search} size="sm" className="icon" />
              <input
                ref={searchRef}
                className="search"
                type="search"
                value={query}
                placeholder={t('{section} durchsuchen', { section: title })}
                aria-label={t('{section} durchsuchen', { section: title })}
                spellCheck={false}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Escape' && query) {
                    e.preventDefault();
                    e.stopPropagation();
                    setQuery('');
                  }
                }}
              />
            </label>
          </div>
          <p className="list-title">
            <span>{title}</span>
            <span className="spacer" />
            <IconButton
              icon={ICONS.refresh}
              size="sm"
              label={t('Neu laden')}
              onClick={() => void load()}
            />
            {view?.exists && (
              <Button
                size="sm"
                icon={ICONS.add}
                className="new-item"
                aria-haspopup="menu"
                aria-expanded={Boolean(newMenu)}
                title={t('Neu')}
                onClick={(event) => {
                  const rect = event.currentTarget.getBoundingClientRect();
                  setNewMenu({ x: rect.right - 180, y: rect.bottom + 4 });
                }}
              >
                {t('Neu')}
              </Button>
            )}
          </p>
          {view?.exists && (
            <div className="suite-tabs" role="tablist" aria-label={title}>
              {tabs.map((x) => (
                <button
                  key={x.tab}
                  type="button"
                  role="tab"
                  aria-selected={tab === x.tab}
                  onClick={() => setTab(x.tab)}
                >
                  {t(x.label)}
                  <span className="nav-count">
                    {x.tab === 'host'
                      ? records.filter((r) => r.kind === 'host').length
                      : records.filter((r) => r.kind === x.tab).length}
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>
        {list}
      </section>

      <section className="detail-pane">
        {phone && detailOpen && (
          <div className="detail-back">
            <Button variant="ghost" size="sm" icon={ICONS.back} onClick={() => onDetail(false)}>
              {title}
            </Button>
          </div>
        )}
        {current ? (
          <RecordDetail
            key={current.id}
            ctx={ctx}
            record={current}
            onEdit={(target) => setEditing(target)}
            onShow={(id, kind) => {
              setTab(kind === 'group' || kind === 'host' ? 'host' : (kind as Tab));
              show(id);
            }}
            onGone={() => {
              setSelected(null);
              onDetail(false);
            }}
          />
        ) : (
          <div className="detail-empty">
            {view && <NyuScene name="vault" className="empty-scene" />}
          </div>
        )}
      </section>

      {newMenu && (
        <ContextMenu
          x={newMenu.x}
          y={newMenu.y}
          label={t('Neu')}
          onClose={() => setNewMenu(null)}
          items={newItems.map((item) => ({
            label: item.label,
            icon: item.icon,
            onSelect: () => openEditor(item.kind),
          }))}
        />
      )}

      {newKey && (
        <NewKeyDialog
          ctx={ctx}
          onClose={() => setNewKey(false)}
          onSaved={(id) => {
            setNewKey(false);
            setTab('key');
            show(id);
          }}
        />
      )}

      {editing && (
        <Editor
          ctx={ctx}
          target={editing}
          onClose={() => setEditing(null)}
          onSaved={(id, kind) => {
            setEditing(null);
            setTab(kind === 'group' || kind === 'host' ? 'host' : (kind as Tab));
            show(id);
          }}
        />
      )}
    </>
  );
}

function EmptyList({ query, empty }: { query: string; empty: boolean }) {
  useLanguage();
  return (
    <div className="list-empty">
      <NyuScene name={query ? 'puzzled' : 'sleepy'} className="empty-scene" />
      <p>
        {query
          ? t('Nichts gefunden für „{query}“.', { query: query.trim() })
          : empty
            ? t('Noch nichts hier. Leg oben rechts etwas an.')
            : t('Hier ist nichts. (˘ω˘)')}
      </p>
    </div>
  );
}

function Editor({
  ctx,
  target,
  onClose,
  onSaved,
}: {
  ctx: SuiteCtx;
  target: EditorTarget;
  onClose: () => void;
  onSaved: (id: string, kind: SuiteKind) => void;
}) {
  const saved = (id: string) => onSaved(id, target.kind);
  switch (target.kind) {
    case 'host':
      return <HostEditor ctx={ctx} host={target.record} onClose={onClose} onSaved={saved} />;
    case 'group':
      return <GroupEditor ctx={ctx} group={target.record} onClose={onClose} onSaved={saved} />;
    case 'identity':
      return (
        <IdentityEditor ctx={ctx} identity={target.record} onClose={onClose} onSaved={saved} />
      );
    case 'key':
      return target.record ? (
        <KeyEditor ctx={ctx} record={target.record} onClose={onClose} onSaved={saved} />
      ) : null;
    case 'snippet':
      return <SnippetEditor ctx={ctx} snippet={target.record} onClose={onClose} onSaved={saved} />;
    case 'port_forward':
      return (
        <TunnelEditor ctx={ctx} hostId={target.hostId} tunnel={target.record} onClose={onClose} />
      );
  }
}

// ── Detail ─────────────────────────────────────────────────

function Row({
  label,
  children,
  actions,
  mono,
}: {
  label: string;
  children: ReactNode;
  actions?: ReactNode;
  mono?: boolean;
}) {
  return (
    <div className="detail-row">
      <div className="detail-text">
        <span className="detail-label">{label}</span>
        <span className={mono ? 'detail-value mono' : 'detail-value'}>{children}</span>
      </div>
      {actions && <div className="detail-actions">{actions}</div>}
    </div>
  );
}

function Card({
  title,
  children,
  tools,
}: {
  title?: string;
  children: ReactNode;
  tools?: ReactNode;
}) {
  return (
    <section className="detail-card">
      {(title || tools) && (
        <h3 className="detail-card-title flex items-center justify-between gap-2">
          <span>{title}</span>
          {tools}
        </h3>
      )}
      {children}
    </section>
  );
}

async function copyText(text: string) {
  try {
    await copyGenerated(text);
    toast(copiedText('text', getSettings().clipboardClear));
  } catch (e) {
    toastError(e);
  }
}

function CopyText({ text, label }: { text: string; label: string }) {
  useLanguage();
  return (
    <IconButton
      icon={ICONS.copy}
      size="sm"
      onClick={() => void copyText(text)}
      label={t('{label} kopieren', { label })}
    />
  );
}

/** A secret record: dots until the eye; copied by Rust. */
function SecretRow({
  space,
  id,
  label,
  multiline,
  extra,
}: {
  space: SuiteSpace;
  id: string;
  label: string;
  multiline?: boolean;
  extra?: ReactNode;
}) {
  useLanguage();
  const [value, setValue] = useState<string | null>(null);
  useEffect(() => {
    if (value === null) return;
    const timer = window.setTimeout(() => setValue(null), 60_000);
    return () => window.clearTimeout(timer);
  }, [value]);
  const toggle = async () => {
    if (value !== null) return setValue(null);
    try {
      setValue(await suiteReveal(space, id));
    } catch (e) {
      toastError(e);
    }
  };
  const copy = async () => {
    try {
      await suiteCopy(space, id);
      toast(
        copiedText(label === t('Passwort') ? 'password' : 'text', getSettings().clipboardClear),
      );
    } catch (e) {
      toastError(e);
    }
  };
  return (
    <Row
      label={label}
      mono
      actions={
        <>
          <IconButton
            icon={value === null ? ICONS.show : ICONS.hide}
            size="sm"
            onClick={() => void toggle()}
            label={
              value === null ? t('{label} zeigen', { label }) : t('{label} verbergen', { label })
            }
            aria-pressed={value !== null}
          />
          <IconButton
            icon={ICONS.copy}
            size="sm"
            onClick={() => void copy()}
            label={t('{label} kopieren', { label })}
          />
          {extra}
        </>
      }
    >
      {value === null ? (
        <span className="masked">••••••••••••</span>
      ) : multiline ? (
        <pre className="secret-block">{value}</pre>
      ) : (
        value
      )}
    </Row>
  );
}

function Delete({
  ctx,
  record,
  onGone,
}: {
  ctx: SuiteCtx;
  record: SuiteRecord;
  onGone: () => void;
}) {
  useLanguage();
  const [asking, setAsking] = useState(false);
  const users = (record.usedBy ?? [])
    .map((id) => ctx.index.get(id))
    .filter(Boolean) as SuiteRecord[];
  const blocked = users.length > 0;
  const name = titleOf(record) || t('(ohne Namen)');
  const tunnels = record.kind === 'host' ? tunnelsOf(ctx.records, record.id).length : 0;
  const hosts =
    record.kind === 'group'
      ? ctx.records.filter((r) => r.kind === 'host' && ref(r.data, 'group_id') === record.id).length
      : 0;
  const lead =
    record.kind === 'host'
      ? tunnels
        ? t(
            '„{name}“ wird gelöscht, mit seinen {n} Weiterleitungen. Anmeldung und Schlüssel bleiben.',
            { name, n: tunnels },
          )
        : t('„{name}“ wird gelöscht. Anmeldung und Schlüssel bleiben.', { name })
      : record.kind === 'group'
        ? t('„{name}“ wird gelöscht. Ihre {n} Hosts bleiben – dann ohne Gruppe.', {
            name,
            n: hosts,
          })
        : record.kind === 'identity'
          ? t('„{name}“ wird gelöscht, mit ihrem Passwort.', { name })
          : record.kind === 'key'
            ? t('„{name}“ wird gelöscht, mit dem privaten Schlüssel und der Passphrase.', { name })
            : t('„{name}“ wird gelöscht.', { name });
  return (
    <>
      <IconButton
        icon={ICONS.delete}
        size="sm"
        disabled={blocked}
        label={t('Löschen')}
        title={
          blocked
            ? t('Wird noch verwendet von: {names}', { names: users.map(titleOf).join(', ') })
            : t('Löschen')
        }
        onClick={() => setAsking(true)}
      />
      {asking && (
        <Modal
          title={t('Löschen?')}
          tone="warning"
          size="small"
          onCancel={() => setAsking(false)}
          footer={
            <>
              <span className="spacer" />
              <Button
                variant="danger"
                size="sm"
                data-secondary
                onClick={() => {
                  setAsking(false);
                  void ctx
                    .save([{ op: 'delete', id: record.id, seq: record.seq }], t('Gelöscht.'))
                    .then((ok) => ok && onGone());
                }}
              >
                {t('Löschen')}
              </Button>
              <Button variant="primary" size="sm" data-autofocus onClick={() => setAsking(false)}>
                {t('Abbrechen')}
              </Button>
            </>
          }
        >
          <p className="dialog-lead">{lead}</p>
          <p className="field-hint">
            {t('Auf deinen anderen Geräten verschwindet es beim nächsten Synchronisieren.')}
          </p>
        </Modal>
      )}
    </>
  );
}

function RecordDetail({
  ctx,
  record,
  onEdit,
  onShow,
  onGone,
}: {
  ctx: SuiteCtx;
  record: SuiteRecord;
  onEdit: (target: EditorTarget) => void;
  onShow: (id: string, kind: SuiteKind) => void;
  onGone: () => void;
}) {
  useLanguage();
  const { space, index } = ctx;
  const d = record.data;
  const editable = record.kind !== 'known_host' && record.kind !== 'secret' && !record.broken;
  const kindLabel: Record<string, string> = {
    host: t('Host'),
    group: t('Gruppe'),
    identity: t('Anmeldung'),
    key: t('Schlüssel'),
    snippet: t('Snippet'),
    known_host: t('Bekannter Host'),
  };

  const link = (id: string | null) => {
    const target = id ? index.get(id) : undefined;
    if (!id) return <span className="muted">—</span>;
    if (!target) return <span className="muted">{t('(nicht vorhanden)')}</span>;
    return (
      <button type="button" className="link-button" onClick={() => onShow(target.id, target.kind)}>
        {titleOf(target) || t('(ohne Namen)')}
      </button>
    );
  };

  const move = (step: -1 | 1) =>
    void ctx.save(moveOps(siblingsOf(ctx.records, record), record.id, step));
  const siblings =
    record.kind === 'host' || record.kind === 'group' ? siblingsOf(ctx.records, record) : [];
  const sorted = [...siblings].sort((a, b) => num(a.data, 'position') - num(b.data, 'position'));
  const place = sorted.findIndex((r) => r.id === record.id);

  const users = (record.usedBy ?? []).map((id) => index.get(id)).filter(Boolean) as SuiteRecord[];

  return (
    <article className="detail" aria-label={titleOf(record)}>
      <header className="detail-head">
        <div className="detail-title">
          <h2>
            {record.broken ? t('(lässt sich nicht öffnen)') : titleOf(record) || t('(ohne Namen)')}
          </h2>
          <p className="chips">
            <span className="chip">
              {(Object.hasOwn(kindLabel, record.kind) && kindLabel[record.kind]) || record.kind}
            </span>
            {(record.kind === 'host' || record.kind === 'group') && (
              <span className="chip">{workspaceLabel(str(d, 'workspace') || 'private')}</span>
            )}
            {record.kind === 'host' && ref(d, 'group_id') && index.get(ref(d, 'group_id')!) && (
              <span className="chip">
                <Icon icon={ICONS.folder} size="xs" />
                {titleOf(index.get(ref(d, 'group_id')!)!)}
              </span>
            )}
          </p>
        </div>
        <div className="detail-tools">
          {siblings.length > 1 && (
            <>
              <IconButton
                icon={ICONS.moveUp}
                size="sm"
                disabled={place <= 0}
                label={t('Nach oben')}
                onClick={() => move(-1)}
              />
              <IconButton
                icon={ICONS.moveDown}
                size="sm"
                disabled={place < 0 || place >= sorted.length - 1}
                label={t('Nach unten')}
                onClick={() => move(1)}
              />
            </>
          )}
          {record.kind !== 'secret' && <Delete ctx={ctx} record={record} onGone={onGone} />}
          {editable && (
            <Button
              variant="primary"
              size="sm"
              icon={ICONS.edit}
              onClick={() =>
                onEdit({
                  kind: record.kind as Exclude<SuiteKind, 'secret' | 'known_host' | 'port_forward'>,
                  record,
                })
              }
            >
              {t('Bearbeiten')}
            </Button>
          )}
        </div>
      </header>

      {record.broken && (
        <p className="notice" data-tone="error">
          {t('Dieser Eintrag lässt sich nicht entschlüsseln. UwULock lässt ihn, wie er ist.')}
        </p>
      )}

      {record.kind === 'host' && d && (
        <HostBody ctx={ctx} host={record} link={link} onEdit={onEdit} />
      )}

      {record.kind === 'group' && d && (
        <Card>
          <Row label={t('Hosts')}>
            {
              ctx.records.filter((r) => r.kind === 'host' && ref(r.data, 'group_id') === record.id)
                .length
            }
          </Row>
          {space === 'rdp' && (
            <Row label={t('Anmeldung für alle Hosts')}>{link(ref(d, 'identity_id'))}</Row>
          )}
          {space === 'rdp' && (
            <Row label={t('Laufwerke umleiten')}>{drivesText(obj(d, 'drives'), false)}</Row>
          )}
        </Card>
      )}

      {record.kind === 'identity' && d && (
        <Card>
          <Row
            label={t('Benutzername')}
            mono
            actions={<CopyText text={str(d, 'username')} label={t('Benutzername')} />}
          >
            {str(d, 'username') || <span className="muted">—</span>}
          </Row>
          {space === 'rdp' && str(d, 'domain') && <Row label={t('Domäne')}>{str(d, 'domain')}</Row>}
          <Row label={t('Anmelden mit')}>{authLabel(str(d, 'auth_type'))}</Row>
          {ref(d, 'key_id') && <Row label={t('Schlüssel')}>{link(ref(d, 'key_id'))}</Row>}
          {index.get(ref(d, 'password_secret_id') ?? '')?.kind === 'secret' && (
            <SecretRow space={space} id={ref(d, 'password_secret_id')!} label={t('Passwort')} />
          )}
          <UsedBy users={users} onShow={onShow} />
        </Card>
      )}

      {record.kind === 'key' && d && (
        <KeyBody ctx={ctx} record={record} users={users} onShow={onShow} />
      )}

      {record.kind === 'snippet' && d && (
        <Card>
          {str(d, 'group_path') && <Row label={t('Ordner')}>{str(d, 'group_path')}</Row>}
          <Row
            label={t('Befehl')}
            mono
            actions={<CopyText text={str(d, 'body')} label={t('Befehl')} />}
          >
            <pre className="secret-block">{str(d, 'body')}</pre>
          </Row>
        </Card>
      )}

      {record.kind === 'known_host' && d && (
        <Card>
          <Row label={t('Adresse')} mono>
            {hostPort(str(d, 'address'), num(d, 'port', 22))}
          </Row>
          <Row label={t('Algorithmus')} mono>
            {str(d, 'algorithm')}
          </Row>
          <Row
            label={t('Fingerabdruck')}
            mono
            actions={<CopyText text={str(d, 'fingerprint_sha256')} label={t('Fingerabdruck')} />}
          >
            {str(d, 'fingerprint_sha256')}
          </Row>
          <Row label={t('Öffentlicher Schlüssel')} mono>
            <span className="uri">{str(d, 'public_key')}</span>
          </Row>
          {num(d, 'first_seen_ms') > 0 && (
            <Row label={t('Zuerst gesehen')}>
              {new Date(num(d, 'first_seen_ms')).toLocaleString()}
            </Row>
          )}
          <p className="field-hint">
            {t(
              'Löschen heißt: Beim nächsten Verbinden fragt {app} wieder, ob du dem Server vertraust.',
              { app: APP[space] },
            )}
          </p>
        </Card>
      )}
    </article>
  );
}

function UsedBy({
  users,
  onShow,
}: {
  users: SuiteRecord[];
  onShow: (id: string, kind: SuiteKind) => void;
}) {
  useLanguage();
  if (!users.length) return null;
  return (
    <Row label={t('Wird verwendet von')}>
      <span className="flex flex-wrap gap-x-2.5 gap-y-1">
        {users.map((u) => (
          <button
            key={u.id}
            type="button"
            className="link-button"
            onClick={() => onShow(u.id, u.kind)}
          >
            {titleOf(u) || t('(ohne Namen)')}
          </button>
        ))}
      </span>
      <small className="field-hint">{t('Löschen geht erst, wenn nichts mehr darauf zeigt.')}</small>
    </Row>
  );
}

function drivesText(drives: Record<string, unknown> | null, inherit: boolean): string {
  if (!drives) return inherit ? t('Wie die Gruppe') : t('Aus');
  if (!bool(drives, 'enabled')) return t('Aus');
  const list = Array.isArray(drives.drives) ? (drives.drives as Record<string, unknown>[]) : [];
  return list.length
    ? list
        .map(
          (x) =>
            `${str(x, 'name')} → ${str(x, 'path') === '*' ? t('alle Laufwerke') : str(x, 'path')}`,
        )
        .join(', ')
    : t('An');
}

function HostBody({
  ctx,
  host,
  link,
  onEdit,
}: {
  ctx: SuiteCtx;
  host: SuiteRecord;
  link: (id: string | null) => ReactNode;
  onEdit: (target: EditorTarget) => void;
}) {
  useLanguage();
  const { space, index } = ctx;
  const d = host.data;
  const login = loginOf(host, index);
  const rdp = obj(d, 'rdp');
  const gateway = obj(rdp, 'gateway');
  const command = space === 'ssh' ? sshCommand(d, login?.data ?? null) : rdpAddress(d);
  const desktop = !isMobile();

  const saveRdp = async () => {
    try {
      const where = await suiteSaveRdp(host.id);
      toast(t('Gespeichert unter {path} ✧', { path: where }));
    } catch (e) {
      toastError(e);
    }
  };
  const openApp = async () => {
    try {
      await suiteOpenInApp(space, host.id);
    } catch (e) {
      toastError(e);
    }
  };

  const display = str(rdp, 'display') || 'fit';
  const audio = str(rdp, 'audio') || 'local';
  const tunnels = tunnelsOf(ctx.records, host.id);

  return (
    <>
      <div className="mb-3 flex flex-wrap gap-2 phone:[&>button]:flex-auto">
        <Button variant="ghost" size="sm" icon={ICONS.copy} onClick={() => void copyText(command)}>
          {space === 'ssh' ? t('Befehl kopieren') : t('Adresse kopieren')}
        </Button>
        {space === 'rdp' && (
          <Button variant="ghost" size="sm" icon={ICONS.download} onClick={() => void saveRdp()}>
            {t('.rdp-Datei')}
          </Button>
        )}
        {desktop && (
          <Button
            variant="ghost"
            size="sm"
            icon={ICONS.openExternal}
            onClick={() => void openApp()}
          >
            {t('In {app} öffnen', { app: APP[space] })}
          </Button>
        )}
      </div>
      <Card>
        <Row
          label={t('Adresse')}
          mono
          actions={<CopyText text={str(d, 'address')} label={t('Adresse')} />}
        >
          {str(d, 'address')}
        </Row>
        <Row label={t('Port')} mono>
          {num(d, 'port', space === 'rdp' ? 3389 : 22)}
        </Row>
        <Row label={t('Anmeldung')}>
          {ref(d, 'identity_id') ? (
            link(ref(d, 'identity_id'))
          ) : login ? (
            <>
              {link(login.id)} <span className="muted">{t('(von der Gruppe)')}</span>
            </>
          ) : (
            <span className="muted">{t('Keine')}</span>
          )}
        </Row>
        {login && str(login.data, 'username') && (
          <Row label={t('Benutzername')} mono>
            {[str(login.data, 'domain'), str(login.data, 'username')].filter(Boolean).join('\\')}
          </Row>
        )}
        {space === 'ssh' && (
          <Row label={t('Befehl')} mono>
            {command}
          </Row>
        )}
        {str(d, 'comment') && <Row label={t('Notiz')}>{str(d, 'comment')}</Row>}
      </Card>

      {space === 'rdp' && (
        <Card title={t('Remote Desktop')}>
          <Row label={t('Größe')}>
            {display === 'fixed'
              ? `${num(rdp, 'width', 1920)} × ${num(rdp, 'height', 1080)}`
              : display === 'fullscreen'
                ? t('Vollbild')
                : display === 'fit'
                  ? t('An das Fenster anpassen')
                  : display}
          </Row>
          <Row label={t('Farbtiefe')}>{t('{n} Bit', { n: num(rdp, 'colorDepth', 32) })}</Row>
          <Row label={t('Ton')}>
            {audio === 'remote'
              ? t('Auf dem Server lassen')
              : audio === 'off'
                ? t('Aus')
                : audio === 'local'
                  ? t('Hier abspielen')
                  : audio}
          </Row>
          <Row label={t('Optionen')}>
            <span className="flex flex-wrap gap-1">
              {(
                [
                  [t('Verkleinern statt scrollen'), bool(rdp, 'smartSizing', true)],
                  [t('Zwischenablage teilen'), bool(rdp, 'clipboard', true)],
                  [t('Konsolensitzung (/admin)'), bool(rdp, 'admin', false)],
                  [t('Netzwerkebenen-Authentifizierung (NLA)'), bool(rdp, 'nla', true)],
                  [t('Hintergrundbild zeigen'), bool(rdp, 'wallpaper', true)],
                  [t('Grafik-Pipeline (RDPEGFX)'), bool(rdp, 'graphicsPipeline', true)],
                ] as const
              ).map(([label, on]) => (
                <span
                  key={label}
                  className={on ? 'chip' : 'chip chip-muted'}
                  title={on ? t('An') : t('Aus')}
                >
                  {on ? '✓' : '✗'} {label}
                </span>
              ))}
            </span>
          </Row>
          <Row label={t('Gateway')}>
            <span className="mono">
              {gateway ? hostPort(str(gateway, 'address'), num(gateway, 'port', 443)) : t('Keins')}
            </span>
            {gateway && bool(gateway, 'bypassLocal') && (
              <small className="field-hint block">{t('Für lokale Adressen umgehen')}</small>
            )}
          </Row>
          {gateway && (
            <Row label={t('Anmeldung am Gateway')}>
              {bool(gateway, 'useHostLogin')
                ? t('Wie der Host')
                : link(ref(d, 'gateway_identity_id'))}
            </Row>
          )}
          <Row label={t('Laufwerke umleiten')}>{drivesText(obj(rdp, 'drives'), true)}</Row>
        </Card>
      )}

      {space === 'ssh' && (
        <Card
          title={t('Weiterleitungen')}
          tools={
            <IconButton
              icon={ICONS.add}
              size="sm"
              label={t('Neue Weiterleitung')}
              onClick={() => onEdit({ kind: 'port_forward', record: null, hostId: host.id })}
            />
          }
        >
          {tunnels.length ? (
            tunnels.map((tunnel) => (
              <Row
                key={tunnel.id}
                label={str(tunnel.data, 'name') || t('(ohne Namen)')}
                mono
                actions={
                  <>
                    <IconButton
                      icon={ICONS.edit}
                      size="sm"
                      label={t('Bearbeiten')}
                      onClick={() =>
                        onEdit({ kind: 'port_forward', record: tunnel, hostId: host.id })
                      }
                    />
                    <Delete ctx={ctx} record={tunnel} onGone={() => undefined} />
                  </>
                }
              >
                {tunnelSpec(tunnel.data)}
                {bool(tunnel.data, 'autostart') && <span className="chip">{t('startet mit')}</span>}
              </Row>
            ))
          ) : (
            <p className="detail-empty-line">{t('Keine Weiterleitungen.')}</p>
          )}
        </Card>
      )}
    </>
  );
}

function KeyBody({
  ctx,
  record,
  users,
  onShow,
}: {
  ctx: SuiteCtx;
  record: SuiteRecord;
  users: SuiteRecord[];
  onShow: (id: string, kind: SuiteKind) => void;
}) {
  useLanguage();
  const { space, index } = ctx;
  const d = record.data;
  const privateId = ref(d, 'private_secret_id');
  const phraseId = ref(d, 'passphrase_secret_id');
  const saveFile = async (half: 'public' | 'private') => {
    try {
      const where = await suiteSaveKey(space, record.id, half);
      toast(t('Gespeichert unter {path} ✧', { path: where }));
    } catch (e) {
      toastError(e);
    }
  };
  const download = (half: 'public' | 'private', label: string) => (
    <IconButton
      icon={ICONS.download}
      size="sm"
      label={t('{label} als Datei speichern', { label })}
      onClick={() => void saveFile(half)}
    />
  );
  return (
    <Card>
      <Row label={t('Typ')} mono>
        {str(d, 'key_type') || <span className="muted">—</span>}
      </Row>
      {str(d, 'public_key') && (
        <Row
          label={t('Öffentlicher Schlüssel')}
          mono
          actions={
            <>
              <CopyText text={str(d, 'public_key')} label={t('Öffentlicher Schlüssel')} />
              {download('public', t('Öffentlicher Schlüssel'))}
            </>
          }
        >
          <span className="uri">{str(d, 'public_key')}</span>
        </Row>
      )}
      {privateId && index.get(privateId)?.kind === 'secret' && (
        <SecretRow
          space={space}
          id={privateId}
          label={t('Privater Schlüssel')}
          multiline
          extra={download('private', t('Privater Schlüssel'))}
        />
      )}
      {phraseId && index.get(phraseId)?.kind === 'secret' && (
        <SecretRow space={space} id={phraseId} label={t('Passphrase')} />
      )}
      <UsedBy users={users} onShow={onShow} />
    </Card>
  );
}
