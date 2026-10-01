import { listen } from '@tauri-apps/api/event';
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
import { Icon, type IconName } from './Icon';
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

const TABS: { tab: Tab; label: string; icon: IconName }[] = [
  { tab: 'host', label: N_('Hosts'), icon: 'monitor' },
  { tab: 'identity', label: N_('Anmeldungen'), icon: 'user' },
  { tab: 'key', label: N_('Schlüssel'), icon: 'key' },
  { tab: 'snippet', label: N_('Snippets'), icon: 'terminal' },
  { tab: 'known_host', label: N_('Bekannte Hosts'), icon: 'shield' },
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

  const newItems: { kind: Tab | 'group'; label: string; icon: IconName }[] = [
    { kind: 'host', label: t('Host'), icon: 'monitor' },
    { kind: 'group', label: t('Gruppe'), icon: 'folder' },
    { kind: 'identity', label: t('Anmeldung'), icon: 'user' },
    ...(space === 'ssh'
      ? [
          { kind: 'key' as const, label: t('Schlüssel'), icon: 'key' as const },
          { kind: 'snippet' as const, label: t('Snippet'), icon: 'terminal' as const },
        ]
      : []),
  ];

  const row = (record: SuiteRecord, sub: ReactNode, icon: IconName, indent = false) => (
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
        <Icon name={icon} size={16} />
      </span>
      <span className="item-text">
        <span className="item-name">
          {record.broken ? t('(lässt sich nicht öffnen)') : titleOf(record) || t('(ohne Namen)')}
        </span>
        {sub && <span className="item-sub">{sub}</span>}
      </span>
      {record.broken && (
        <span className="item-badges">
          <Icon name="warning" size={13} className="badge-warning" />
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
      <button className="primary" disabled={creating} onClick={() => void create()}>
        {creating ? t('Einen Moment …') : t('Bereich anlegen')}
      </button>
    </div>
  ) : tab === 'host' ? (
    grouped.length ? (
      <ul className="item-list suite-list" role="listbox" aria-label={title}>
        {grouped.map((ws) => (
          <li key={ws.workspace} className="suite-ws" role="presentation">
            <h3 className="suite-ws-title">{workspaceLabel(ws.workspace)}</h3>
            <ul role="presentation">
              {ws.groups.map((g) => (
                <li key={g.group?.id ?? 'loose'} role="presentation">
                  {g.group ? (
                    <button
                      className="suite-group"
                      aria-current={g.group.id === selected || undefined}
                      onClick={() => show(g.group!.id)}
                    >
                      <Icon name="folder" size={14} />
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
                        space === 'rdp' ? 'monitor' : 'terminal',
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
      {flat.map((r) => row(r, subOf(r), TABS.find((x) => x.tab === tab)?.icon ?? 'key'))}
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
              <button
                className="icon-button menu-button"
                aria-label={t('Ordner und Typen')}
                onClick={onMenu}
              >
                <Icon name="menu" size={18} />
              </button>
            )}
            <label className="search-box">
              <Icon name="search" size={15} />
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
            <button
              className="icon-button"
              title={t('Neu laden')}
              aria-label={t('Neu laden')}
              onClick={() => void load()}
            >
              <Icon name="refresh" size={15} />
            </button>
            {view?.exists && (
              <button
                className="new-item"
                aria-haspopup="menu"
                aria-expanded={Boolean(newMenu)}
                title={t('Neu')}
                onClick={(event) => {
                  const rect = event.currentTarget.getBoundingClientRect();
                  setNewMenu({ x: rect.right - 180, y: rect.bottom + 4 });
                }}
              >
                <Icon name="plus" size={15} />
                {t('Neu')}
              </button>
            )}
          </p>
          {view?.exists && (
            <div className="suite-tabs" role="tablist" aria-label={title}>
              {tabs.map((x) => (
                <button
                  key={x.tab}
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
            <button className="quiet" onClick={() => onDetail(false)}>
              <Icon name="back" size={16} />
              {title}
            </button>
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
        <h3 className="detail-card-title suite-card-title">
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
    <button
      className="icon-button"
      onClick={() => void copyText(text)}
      aria-label={t('{label} kopieren', { label })}
      title={t('Kopieren')}
    >
      <Icon name="copy" size={15} />
    </button>
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
          <button
            className="icon-button"
            onClick={() => void toggle()}
            aria-label={
              value === null ? t('{label} zeigen', { label }) : t('{label} verbergen', { label })
            }
            aria-pressed={value !== null}
            title={value === null ? t('Zeigen') : t('Verbergen')}
          >
            <Icon name={value === null ? 'eye' : 'eyeOff'} size={15} />
          </button>
          <button
            className="icon-button"
            onClick={() => void copy()}
            aria-label={t('{label} kopieren', { label })}
            title={t('Kopieren')}
          >
            <Icon name="copy" size={15} />
          </button>
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
      <button
        className="icon-button"
        disabled={blocked}
        title={
          blocked
            ? t('Wird noch verwendet von: {names}', { names: users.map(titleOf).join(', ') })
            : t('Löschen')
        }
        aria-label={t('Löschen')}
        onClick={() => setAsking(true)}
      >
        <Icon name="trash" size={15} />
      </button>
      {asking && (
        <Modal
          title={t('Löschen?')}
          onCancel={() => setAsking(false)}
          footer={
            <>
              <span className="spacer" />
              <button
                className="danger"
                data-secondary
                onClick={() => {
                  setAsking(false);
                  void ctx
                    .save([{ op: 'delete', id: record.id, seq: record.seq }], t('Gelöscht.'))
                    .then((ok) => ok && onGone());
                }}
              >
                {t('Löschen')}
              </button>
              <button className="primary" data-autofocus onClick={() => setAsking(false)}>
                {t('Abbrechen')}
              </button>
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
      <button className="link-button" onClick={() => onShow(target.id, target.kind)}>
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
            <span className="chip">{kindLabel[record.kind] ?? record.kind}</span>
            {(record.kind === 'host' || record.kind === 'group') && (
              <span className="chip">{workspaceLabel(str(d, 'workspace') || 'private')}</span>
            )}
            {record.kind === 'host' && ref(d, 'group_id') && index.get(ref(d, 'group_id')!) && (
              <span className="chip">
                <Icon name="folder" size={12} />
                {titleOf(index.get(ref(d, 'group_id')!)!)}
              </span>
            )}
          </p>
        </div>
        <div className="detail-tools">
          {siblings.length > 1 && (
            <>
              <button
                className="icon-button"
                disabled={place <= 0}
                title={t('Nach oben')}
                aria-label={t('Nach oben')}
                onClick={() => move(-1)}
              >
                <Icon name="up" size={15} />
              </button>
              <button
                className="icon-button turned"
                disabled={place < 0 || place >= sorted.length - 1}
                title={t('Nach unten')}
                aria-label={t('Nach unten')}
                onClick={() => move(1)}
              >
                <Icon name="up" size={15} />
              </button>
            </>
          )}
          {record.kind !== 'secret' && <Delete ctx={ctx} record={record} onGone={onGone} />}
          {editable && (
            <button
              className="primary"
              onClick={() =>
                onEdit({
                  kind: record.kind as Exclude<SuiteKind, 'secret' | 'known_host' | 'port_forward'>,
                  record,
                })
              }
            >
              <Icon name="pencil" size={15} />
              {t('Bearbeiten')}
            </button>
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
          {ref(d, 'password_secret_id') && index.get(ref(d, 'password_secret_id')!) && (
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
      <span className="suite-links">
        {users.map((u) => (
          <button key={u.id} className="link-button" onClick={() => onShow(u.id, u.kind)}>
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
  const yes = (on: boolean) => (on ? t('An') : t('Aus'));
  const tunnels = tunnelsOf(ctx.records, host.id);

  return (
    <>
      <div className="suite-actions">
        <button className="quiet" onClick={() => void copyText(command)}>
          <Icon name="copy" size={15} />
          {space === 'ssh' ? t('Befehl kopieren') : t('Adresse kopieren')}
        </button>
        {space === 'rdp' && (
          <button className="quiet" onClick={() => void saveRdp()}>
            <Icon name="download" size={15} />
            {t('.rdp-Datei')}
          </button>
        )}
        {desktop && (
          <button className="quiet" onClick={() => void openApp()}>
            <Icon name="external" size={15} />
            {t('In {app} öffnen', { app: APP[space] })}
          </button>
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
        {str(d, 'comment') && (
          <Row label={t('Notiz')}>
            <span className="notes">{str(d, 'comment')}</span>
          </Row>
        )}
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
          <Row label={t('Verkleinern statt scrollen')}>{yes(bool(rdp, 'smartSizing', true))}</Row>
          <Row label={t('Zwischenablage teilen')}>{yes(bool(rdp, 'clipboard', true))}</Row>
          <Row label={t('Konsolensitzung (/admin)')}>{yes(bool(rdp, 'admin', false))}</Row>
          <Row label={t('Netzwerkebenen-Authentifizierung (NLA)')}>
            {yes(bool(rdp, 'nla', true))}
          </Row>
          <Row label={t('Hintergrundbild zeigen')}>{yes(bool(rdp, 'wallpaper', true))}</Row>
          <Row label={t('Grafik-Pipeline (RDPEGFX)')}>
            {yes(bool(rdp, 'graphicsPipeline', true))}
          </Row>
          <Row label={t('Gateway')} mono>
            {gateway ? hostPort(str(gateway, 'address'), num(gateway, 'port', 443)) : t('Keins')}
          </Row>
          {gateway && (
            <Row label={t('Anmeldung am Gateway')}>
              {bool(gateway, 'useHostLogin')
                ? t('Wie der Host')
                : link(ref(d, 'gateway_identity_id'))}
              {bool(gateway, 'bypassLocal') && (
                <small className="field-hint">{t('Für lokale Adressen umgehen')}</small>
              )}
            </Row>
          )}
          <Row label={t('Laufwerke umleiten')}>{drivesText(obj(rdp, 'drives'), true)}</Row>
        </Card>
      )}

      {space === 'ssh' && (
        <Card
          title={t('Weiterleitungen')}
          tools={
            <button
              className="icon-button tiny"
              title={t('Neue Weiterleitung')}
              aria-label={t('Neue Weiterleitung')}
              onClick={() => onEdit({ kind: 'port_forward', record: null, hostId: host.id })}
            >
              <Icon name="plus" size={14} />
            </button>
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
                    <button
                      className="icon-button"
                      title={t('Bearbeiten')}
                      aria-label={t('Bearbeiten')}
                      onClick={() =>
                        onEdit({ kind: 'port_forward', record: tunnel, hostId: host.id })
                      }
                    >
                      <Icon name="pencil" size={15} />
                    </button>
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
    <button
      className="icon-button"
      title={t('Als Datei speichern')}
      aria-label={t('{label} als Datei speichern', { label })}
      onClick={() => void saveFile(half)}
    >
      <Icon name="download" size={15} />
    </button>
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
      {privateId && index.get(privateId) && (
        <SecretRow
          space={space}
          id={privateId}
          label={t('Privater Schlüssel')}
          multiline
          extra={download('private', t('Privater Schlüssel'))}
        />
      )}
      {phraseId && index.get(phraseId) && (
        <SecretRow space={space} id={phraseId} label={t('Passphrase')} />
      )}
      <UsedBy users={users} onShow={onShow} />
    </Card>
  );
}
