import { useMemo, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { failure } from '../lib/api';
import { errorText, toastError } from '../lib/errors';
import { N_, t, useLanguage } from '../lib/i18n';
import { suiteGenerateKey, suiteImportKey, suiteReveal, type SuiteSaved } from '../lib/suite';
import {
  AUTH_TYPES,
  WORKSPACES,
  bool,
  newId,
  nextPosition,
  num,
  obj,
  ofKind,
  ref,
  siblingsOf,
  str,
  titleOf,
  type Json,
  type SuiteKind,
  type SuiteOp,
  type SuiteRecord,
  type SuiteSpace,
} from '../lib/suiteModel';
import { useCloseGuard } from './CloseGuard';
import { GeneratorDialog } from './GeneratorDialog';
import { Icon } from './Icon';
import { Modal } from './Modal';

/** What the editors need of the section. */
export type SuiteCtx = {
  space: SuiteSpace;
  records: SuiteRecord[];
  index: Map<string, SuiteRecord>;
  /** Saves a batch; `true` when it went through without a conflict. */
  save: (ops: SuiteOp[], done?: string) => Promise<boolean>;
  /** A key generation or import went through: the section takes the answer. */
  took: (saved: SuiteSaved) => boolean;
};

export const WORKSPACE_LABEL: Record<string, string> = {
  private: N_('Privat'),
  business: N_('Geschäftlich'),
};

export function workspaceLabel(workspace: string): string {
  const known = WORKSPACE_LABEL[workspace];
  return known ? t(known) : workspace;
}

export const AUTH_LABEL: Record<string, string> = {
  password: N_('Passwort'),
  key: N_('Schlüssel'),
  agent: N_('SSH-Agent'),
  'keyboard-interactive': N_('Tastatur-interaktiv'),
  cert: N_('Zertifikat'),
};

export function authLabel(auth: string): string {
  const known = AUTH_LABEL[auth];
  return known ? t(known) : auth;
}

function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: string }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
      {hint && <small className="field-hint">{hint}</small>}
    </label>
  );
}

function Check({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <label className="check">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>{label}</span>
    </label>
  );
}

/** A port as typed: 1–65535, else `null`. */
function port(text: string): number | null {
  const n = Number(text.trim());
  return Number.isInteger(n) && n >= 1 && n <= 65535 ? n : null;
}

/** The dialog every editor sits in: save in the footer, asks before losing input. */
function EditorModal({
  title,
  dirty,
  busy,
  canSave,
  error,
  onClose,
  onSubmit,
  children,
}: {
  title: string;
  dirty: boolean;
  busy: boolean;
  canSave: boolean;
  error?: string | null;
  onClose: () => void;
  onSubmit: () => void;
  children: ReactNode;
}) {
  useLanguage();
  const guard = useCloseGuard(dirty && !busy, onClose);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (canSave && !busy) onSubmit();
  };
  return (
    <>
      <Modal
        title={title}
        size="wide"
        onCancel={guard.request}
        footer={
          <>
            <button type="button" className="quiet" data-secondary onClick={guard.request}>
              {t('Abbrechen')}
            </button>
            <span className="spacer" />
            <button
              type="submit"
              form="suite-editor"
              className="primary"
              disabled={busy || !canSave}
            >
              {busy ? t('Speichert …') : t('Speichern')}
            </button>
          </>
        }
      >
        <form id="suite-editor" className="editor" onSubmit={submit}>
          {error && (
            <p className="form-error" role="alert">
              {error}
            </p>
          )}
          {children}
        </form>
      </Modal>
      {guard.dialog}
    </>
  );
}

/** A form's state, and whether it changed since it opened. */
function useForm<T extends object>(initial: () => T) {
  const [start] = useState(initial);
  const [form, setForm] = useState<T>(start);
  const set = (patch: Partial<T>) => setForm((now) => ({ ...now, ...patch }));
  const dirty = useMemo(() => JSON.stringify(form) !== JSON.stringify(start), [form, start]);
  return { form, set, dirty };
}

function WorkspaceSelect({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const options = (WORKSPACES as readonly string[]).includes(value)
    ? [...WORKSPACES]
    : [...WORKSPACES, value];
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)}>
      {options.map((w) => (
        <option key={w} value={w}>
          {workspaceLabel(w)}
        </option>
      ))}
    </select>
  );
}

function RecordSelect({
  records,
  value,
  none,
  onChange,
  extra,
}: {
  records: SuiteRecord[];
  value: string;
  none: string;
  onChange: (v: string) => void;
  extra?: ReactNode;
}) {
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)}>
      <option value="">{none}</option>
      {records.map((r) => (
        <option key={r.id} value={r.id}>
          {titleOf(r) || t('(ohne Namen)')}
        </option>
      ))}
      {/* A pointer at something this device doesn't have stays as it is. */}
      {value && value !== NEW && !records.some((r) => r.id === value) && (
        <option value={value}>{t('(nicht vorhanden)')}</option>
      )}
      {extra}
    </select>
  );
}

const NEW = '__new';

// ── Drive redirection (UwURDP) ─────────────────────────────

type Drive = { name: string; path: string; orig: Json };
type Drives = { mode: 'inherit' | 'off' | 'on'; list: Drive[]; orig: Json };

function readDrives(value: Json | null, inherit: boolean): Drives {
  if (!value) return { mode: inherit ? 'inherit' : 'off', list: [], orig: {} };
  const raw = Array.isArray(value.drives) ? (value.drives as unknown[]) : [];
  return {
    mode: bool(value, 'enabled') ? 'on' : 'off',
    orig: value,
    list: raw
      .filter((d): d is Json => Boolean(d) && typeof d === 'object')
      .map((d) => ({ name: str(d, 'name'), path: str(d, 'path'), orig: d })),
  };
}

/** `null` takes the group's (a host) or is off (a group). */
function writeDrives(drives: Drives): Json | null {
  if (drives.mode === 'inherit') return null;
  if (drives.mode === 'off' && !drives.list.length && !Object.keys(drives.orig).length) return null;
  return {
    ...drives.orig,
    enabled: drives.mode === 'on',
    drives: drives.list
      .filter((d) => d.name.trim() || d.path.trim())
      .map((d) => ({ ...d.orig, name: d.name.trim(), path: d.path.trim() })),
  };
}

function DrivesFields({
  drives,
  inherit,
  onChange,
}: {
  drives: Drives;
  inherit: boolean;
  onChange: (d: Drives) => void;
}) {
  useLanguage();
  const setDrive = (i: number, patch: Partial<Drive>) =>
    onChange({ ...drives, list: drives.list.map((d, j) => (j === i ? { ...d, ...patch } : d)) });
  return (
    <fieldset className="editor-list">
      <legend>{t('Laufwerke umleiten')}</legend>
      <select
        value={drives.mode}
        onChange={(e) => onChange({ ...drives, mode: e.target.value as Drives['mode'] })}
      >
        {inherit && <option value="inherit">{t('Wie die Gruppe')}</option>}
        <option value="off">{t('Aus')}</option>
        <option value="on">{t('An')}</option>
      </select>
      {drives.mode === 'on' && (
        <>
          {drives.list.map((d, i) => (
            <div className="editor-row" key={i}>
              <Field label={t('Name auf dem Server')}>
                <input
                  type="text"
                  value={d.name}
                  onChange={(e) => setDrive(i, { name: e.target.value })}
                />
              </Field>
              <Field label={t('Ordner auf diesem Gerät (* = alle Laufwerke)')}>
                <input
                  type="text"
                  className="mono"
                  value={d.path}
                  onChange={(e) => setDrive(i, { path: e.target.value })}
                />
              </Field>
              <button
                type="button"
                className="icon-button"
                title={t('Entfernen')}
                aria-label={t('Entfernen')}
                onClick={() => onChange({ ...drives, list: drives.list.filter((_, j) => j !== i) })}
              >
                <Icon name="trash" size={15} />
              </button>
            </div>
          ))}
          <button
            type="button"
            className="quiet"
            onClick={() =>
              onChange({ ...drives, list: [...drives.list, { name: '', path: '', orig: {} }] })
            }
          >
            <Icon name="plus" size={14} />
            {t('Laufwerk hinzufügen')}
          </button>
        </>
      )}
    </fieldset>
  );
}

// ── Host ───────────────────────────────────────────────────

export function HostEditor({
  ctx,
  host,
  onClose,
  onSaved,
}: {
  ctx: SuiteCtx;
  host: SuiteRecord | null;
  onClose: () => void;
  onSaved: (id: string) => void;
}) {
  useLanguage();
  const rdpSpace = ctx.space === 'rdp';
  const d = host?.data ?? null;
  const rdp = obj(d, 'rdp');
  const gateway = obj(rdp, 'gateway');
  const { form, set, dirty } = useForm(() => ({
    name: str(d, 'name'),
    address: str(d, 'address'),
    port: String(num(d, 'port', rdpSpace ? 3389 : 22)),
    workspace: str(d, 'workspace') || 'private',
    groupId: ref(d, 'group_id') ?? '',
    identityId: ref(d, 'identity_id') ?? '',
    newUser: '',
    newDomain: '',
    newPassword: '',
    comment: str(d, 'comment'),
    gatewayIdentityId: ref(d, 'gateway_identity_id') ?? '',
    display: str(rdp, 'display') || 'fit',
    width: String(num(rdp, 'width', 1920)),
    height: String(num(rdp, 'height', 1080)),
    smartSizing: bool(rdp, 'smartSizing', true),
    colorDepth: String(num(rdp, 'colorDepth', 32)),
    audio: str(rdp, 'audio') || 'local',
    clipboard: bool(rdp, 'clipboard', true),
    admin: bool(rdp, 'admin', false),
    nla: bool(rdp, 'nla', true),
    wallpaper: bool(rdp, 'wallpaper', true),
    graphicsPipeline: bool(rdp, 'graphicsPipeline', true),
    gatewayOn: Boolean(gateway),
    gatewayAddress: str(gateway, 'address'),
    gatewayPort: String(num(gateway, 'port', 443)),
    gatewayUseHostLogin: bool(gateway, 'useHostLogin', false),
    gatewayBypassLocal: bool(gateway, 'bypassLocal', false),
    drives: readDrives(obj(rdp, 'drives'), true),
  }));
  const [busy, setBusy] = useState(false);
  const [generating, setGenerating] = useState(false);
  const groups = ofKind(ctx.records, 'group').filter(
    (g) => (str(g.data, 'workspace') || 'private') === form.workspace,
  );
  const identities = ofKind(ctx.records, 'identity');

  const problem = (): string | null => {
    if (!form.address.trim()) return t('Die Adresse fehlt.');
    if (port(form.port) === null) return t('Der Port ist 1 bis 65535.');
    if (form.identityId === NEW && !form.newUser.trim()) return t('Der Benutzername fehlt.');
    if (
      rdpSpace &&
      form.display === 'fixed' &&
      (port(form.width) === null || port(form.height) === null)
    )
      return t('Breite und Höhe sind Zahlen.');
    if (
      rdpSpace &&
      form.gatewayOn &&
      (!form.gatewayAddress.trim() || port(form.gatewayPort) === null)
    )
      return t('Das Gateway braucht eine Adresse und einen Port.');
    return null;
  };

  const save = async () => {
    const id = host?.id ?? newId();
    const ops: SuiteOp[] = [];
    let identityId: string | null = form.identityId || null;
    if (form.identityId === NEW) {
      identityId = newId();
      let secret: string | null = null;
      if (form.newPassword) {
        secret = newId();
        ops.push({ op: 'secret', id: secret, text: form.newPassword });
      }
      ops.push({
        op: 'put',
        id: identityId,
        kind: 'identity',
        patch: {
          label: form.newUser.trim(),
          username: form.newUser.trim(),
          ...(rdpSpace && form.newDomain.trim() ? { domain: form.newDomain.trim() } : {}),
          auth_type: 'password',
          password_secret_id: secret,
        },
      });
    }
    const groupId = form.groupId || null;
    const patch: Json = {
      name: form.name.trim(),
      address: form.address.trim(),
      port: port(form.port),
      workspace: form.workspace,
      group_id: groupId,
      identity_id: identityId,
    };
    const moved =
      !host ||
      groupId !== ref(d, 'group_id') ||
      form.workspace !== (str(d, 'workspace') || 'private');
    if (moved) {
      const probe: SuiteRecord = {
        id,
        kind: 'host',
        seq: 0,
        updatedMs: 0,
        broken: false,
        data: { workspace: form.workspace, group_id: groupId },
      };
      patch.position = nextPosition(siblingsOf(ctx.records, probe).filter((r) => r.id !== id));
    }
    if (rdpSpace) {
      if (form.comment.trim() || d?.comment !== undefined) patch.comment = form.comment;
      if (form.gatewayIdentityId || d?.gateway_identity_id !== undefined)
        patch.gateway_identity_id = form.gatewayIdentityId || null;
      patch.rdp = {
        display: form.display,
        width: port(form.width) ?? 1920,
        height: port(form.height) ?? 1080,
        smartSizing: form.smartSizing,
        colorDepth: Number(form.colorDepth),
        audio: form.audio,
        clipboard: form.clipboard,
        admin: form.admin,
        nla: form.nla,
        wallpaper: form.wallpaper,
        graphicsPipeline: form.graphicsPipeline,
        gateway: form.gatewayOn
          ? {
              address: form.gatewayAddress.trim(),
              port: port(form.gatewayPort),
              useHostLogin: form.gatewayUseHostLogin,
              bypassLocal: form.gatewayBypassLocal,
            }
          : null,
        drives: writeDrives(form.drives),
      };
    }
    ops.push({ op: 'put', id, kind: 'host', seq: host?.seq, patch });
    setBusy(true);
    const ok = await ctx.save(ops, host ? t('Gespeichert ✧') : t('Host angelegt ✧'));
    setBusy(false);
    if (ok) onSaved(id);
    else onClose();
  };

  const loginNone = rdpSpace ? t('Wie die Gruppe') : t('Keine (fragt beim Verbinden)');
  return (
    <>
      <EditorModal
        title={host ? t('Host bearbeiten') : t('Neuer Host')}
        dirty={dirty}
        busy={busy}
        canSave={problem() === null}
        error={dirty ? problem() : null}
        onClose={onClose}
        onSubmit={() => void save()}
      >
        <div className="editor-row">
          <Field label={t('Name')}>
            <input
              type="text"
              value={form.name}
              autoFocus
              maxLength={200}
              onChange={(e) => set({ name: e.target.value })}
            />
          </Field>
          <Field label={t('Arbeitsbereich')}>
            <WorkspaceSelect
              value={form.workspace}
              onChange={(w) => set({ workspace: w, groupId: '' })}
            />
          </Field>
        </div>
        <div className="editor-row">
          <Field label={t('Adresse')}>
            <input
              type="text"
              className="mono"
              value={form.address}
              spellCheck={false}
              autoCapitalize="off"
              placeholder={rdpSpace ? 'desktop.example.com' : 'server.example.com'}
              onChange={(e) => set({ address: e.target.value })}
            />
          </Field>
          <Field label={t('Port')}>
            <input
              type="text"
              inputMode="numeric"
              className="mono narrow"
              value={form.port}
              onChange={(e) => set({ port: e.target.value })}
            />
          </Field>
        </div>
        <div className="editor-row">
          <Field label={t('Gruppe')}>
            <RecordSelect
              records={groups}
              value={form.groupId}
              none={t('Ohne Gruppe')}
              onChange={(v) => set({ groupId: v })}
            />
          </Field>
          <Field label={t('Anmeldung')}>
            <RecordSelect
              records={identities}
              value={form.identityId}
              none={loginNone}
              onChange={(v) => set({ identityId: v })}
              extra={<option value={NEW}>{t('Neue Anmeldung …')}</option>}
            />
          </Field>
        </div>
        {form.identityId === NEW && (
          <div className="editor-row">
            <Field label={t('Benutzername')}>
              <input
                type="text"
                value={form.newUser}
                autoCapitalize="off"
                spellCheck={false}
                onChange={(e) => set({ newUser: e.target.value })}
              />
            </Field>
            {rdpSpace && (
              <Field label={t('Domäne')}>
                <input
                  type="text"
                  value={form.newDomain}
                  autoCapitalize="off"
                  onChange={(e) => set({ newDomain: e.target.value })}
                />
              </Field>
            )}
            <div className="field">
              <span className="field-label-row">
                <span>{t('Passwort')}</span>
                <span className="field-actions">
                  <button
                    type="button"
                    className="icon-button"
                    title={t('Passwort erzeugen')}
                    aria-label={t('Passwort erzeugen')}
                    onClick={() => setGenerating(true)}
                  >
                    <Icon name="dice" size={15} />
                  </button>
                </span>
              </span>
              <input
                type="text"
                className="mono"
                value={form.newPassword}
                autoComplete="off"
                spellCheck={false}
                onChange={(e) => set({ newPassword: e.target.value })}
              />
            </div>
          </div>
        )}

        {rdpSpace && (
          <>
            <h3 className="editor-heading">{t('Anzeige')}</h3>
            <div className="editor-row">
              <Field label={t('Größe')}>
                <select value={form.display} onChange={(e) => set({ display: e.target.value })}>
                  <option value="fit">{t('An das Fenster anpassen')}</option>
                  <option value="fixed">{t('Feste Auflösung')}</option>
                  <option value="fullscreen">{t('Vollbild')}</option>
                  {!['fit', 'fixed', 'fullscreen'].includes(form.display) && (
                    <option value={form.display}>{form.display}</option>
                  )}
                </select>
              </Field>
              {form.display === 'fixed' && (
                <>
                  <Field label={t('Breite')}>
                    <input
                      type="text"
                      inputMode="numeric"
                      className="mono narrow"
                      value={form.width}
                      onChange={(e) => set({ width: e.target.value })}
                    />
                  </Field>
                  <Field label={t('Höhe')}>
                    <input
                      type="text"
                      inputMode="numeric"
                      className="mono narrow"
                      value={form.height}
                      onChange={(e) => set({ height: e.target.value })}
                    />
                  </Field>
                </>
              )}
              <Field label={t('Farbtiefe')}>
                <select
                  value={form.colorDepth}
                  onChange={(e) => set({ colorDepth: e.target.value })}
                >
                  {['15', '16', '24', '32'].map((bits) => (
                    <option key={bits} value={bits}>
                      {t('{n} Bit', { n: bits })}
                    </option>
                  ))}
                  {!['15', '16', '24', '32'].includes(form.colorDepth) && (
                    <option value={form.colorDepth}>{form.colorDepth}</option>
                  )}
                </select>
              </Field>
              <Field label={t('Ton')}>
                <select value={form.audio} onChange={(e) => set({ audio: e.target.value })}>
                  <option value="local">{t('Hier abspielen')}</option>
                  <option value="remote">{t('Auf dem Server lassen')}</option>
                  <option value="off">{t('Aus')}</option>
                  {!['local', 'remote', 'off'].includes(form.audio) && (
                    <option value={form.audio}>{form.audio}</option>
                  )}
                </select>
              </Field>
            </div>
            <div className="check-grid">
              <Check
                label={t('Verkleinern statt scrollen')}
                checked={form.smartSizing}
                onChange={(v) => set({ smartSizing: v })}
              />
              <Check
                label={t('Zwischenablage teilen')}
                checked={form.clipboard}
                onChange={(v) => set({ clipboard: v })}
              />
              <Check
                label={t('Konsolensitzung (/admin)')}
                checked={form.admin}
                onChange={(v) => set({ admin: v })}
              />
              <Check
                label={t('Netzwerkebenen-Authentifizierung (NLA)')}
                checked={form.nla}
                onChange={(v) => set({ nla: v })}
              />
              <Check
                label={t('Hintergrundbild zeigen')}
                checked={form.wallpaper}
                onChange={(v) => set({ wallpaper: v })}
              />
              <Check
                label={t('Grafik-Pipeline (RDPEGFX)')}
                checked={form.graphicsPipeline}
                onChange={(v) => set({ graphicsPipeline: v })}
              />
            </div>

            <h3 className="editor-heading">{t('Gateway')}</h3>
            <Check
              label={t('Über ein Remotedesktop-Gateway verbinden')}
              checked={form.gatewayOn}
              onChange={(v) => set({ gatewayOn: v })}
            />
            {form.gatewayOn && (
              <>
                <div className="editor-row">
                  <Field label={t('Gateway-Adresse')}>
                    <input
                      type="text"
                      className="mono"
                      value={form.gatewayAddress}
                      autoCapitalize="off"
                      spellCheck={false}
                      placeholder="gateway.example.com"
                      onChange={(e) => set({ gatewayAddress: e.target.value })}
                    />
                  </Field>
                  <Field label={t('Port')}>
                    <input
                      type="text"
                      inputMode="numeric"
                      className="mono narrow"
                      value={form.gatewayPort}
                      onChange={(e) => set({ gatewayPort: e.target.value })}
                    />
                  </Field>
                </div>
                <div className="check-grid">
                  <Check
                    label={t('Mit der Anmeldung des Hosts')}
                    checked={form.gatewayUseHostLogin}
                    onChange={(v) => set({ gatewayUseHostLogin: v })}
                  />
                  <Check
                    label={t('Für lokale Adressen umgehen')}
                    checked={form.gatewayBypassLocal}
                    onChange={(v) => set({ gatewayBypassLocal: v })}
                  />
                </div>
                {!form.gatewayUseHostLogin && (
                  <Field label={t('Anmeldung am Gateway')}>
                    <RecordSelect
                      records={identities}
                      value={form.gatewayIdentityId}
                      none={t('Keine')}
                      onChange={(v) => set({ gatewayIdentityId: v })}
                    />
                  </Field>
                )}
              </>
            )}

            <DrivesFields drives={form.drives} inherit onChange={(drives) => set({ drives })} />

            <Field label={t('Notiz')}>
              <textarea
                rows={3}
                value={form.comment}
                onChange={(e) => set({ comment: e.target.value })}
              />
            </Field>
          </>
        )}
      </EditorModal>
      {generating && (
        <GeneratorDialog
          onClose={() => setGenerating(false)}
          onUse={(pw) => {
            set({ newPassword: pw });
            setGenerating(false);
          }}
        />
      )}
    </>
  );
}

// ── Group ──────────────────────────────────────────────────

export function GroupEditor({
  ctx,
  group,
  onClose,
  onSaved,
}: {
  ctx: SuiteCtx;
  group: SuiteRecord | null;
  onClose: () => void;
  onSaved: (id: string) => void;
}) {
  useLanguage();
  const rdpSpace = ctx.space === 'rdp';
  const d = group?.data ?? null;
  const { form, set, dirty } = useForm(() => ({
    name: str(d, 'name'),
    workspace: str(d, 'workspace') || 'private',
    identityId: ref(d, 'identity_id') ?? '',
    drives: readDrives(obj(d, 'drives'), false),
  }));
  const [busy, setBusy] = useState(false);

  const save = async () => {
    const id = group?.id ?? newId();
    const before = str(d, 'workspace') || 'private';
    const patch: Json = { name: form.name.trim(), workspace: form.workspace };
    const ops: SuiteOp[] = [];
    if (!group || before !== form.workspace) {
      const probe: SuiteRecord = {
        id,
        kind: 'group',
        seq: 0,
        updatedMs: 0,
        broken: false,
        data: { workspace: form.workspace },
      };
      patch.position = nextPosition(siblingsOf(ctx.records, probe).filter((r) => r.id !== id));
    }
    // Its hosts move along to the other workspace, as in the apps.
    if (group && before !== form.workspace) {
      for (const host of ctx.records.filter(
        (r) => r.kind === 'host' && ref(r.data, 'group_id') === id,
      ))
        ops.push({
          op: 'put',
          id: host.id,
          kind: 'host',
          seq: host.seq,
          patch: { workspace: form.workspace },
        });
    }
    if (rdpSpace) {
      if (form.identityId || d?.identity_id !== undefined)
        patch.identity_id = form.identityId || null;
      const drives = writeDrives(form.drives);
      if (drives || d?.drives !== undefined) patch.drives = drives;
    }
    ops.push({ op: 'put', id, kind: 'group', seq: group?.seq, patch });
    setBusy(true);
    const ok = await ctx.save(ops, group ? t('Gespeichert ✧') : t('Gruppe angelegt ✧'));
    setBusy(false);
    if (ok) onSaved(id);
    else onClose();
  };

  return (
    <EditorModal
      title={group ? t('Gruppe bearbeiten') : t('Neue Gruppe')}
      dirty={dirty}
      busy={busy}
      canSave={Boolean(form.name.trim())}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <div className="editor-row">
        <Field label={t('Name')}>
          <input
            type="text"
            value={form.name}
            autoFocus
            maxLength={200}
            onChange={(e) => set({ name: e.target.value })}
          />
        </Field>
        <Field
          label={t('Arbeitsbereich')}
          hint={group ? t('Die Hosts der Gruppe ziehen mit um.') : undefined}
        >
          <WorkspaceSelect value={form.workspace} onChange={(w) => set({ workspace: w })} />
        </Field>
      </div>
      {rdpSpace && (
        <>
          <Field
            label={t('Anmeldung für alle Hosts der Gruppe')}
            hint={t('Ein Host mit eigener Anmeldung nimmt seine.')}
          >
            <RecordSelect
              records={ofKind(ctx.records, 'identity')}
              value={form.identityId}
              none={t('Keine')}
              onChange={(v) => set({ identityId: v })}
            />
          </Field>
          <DrivesFields
            drives={form.drives}
            inherit={false}
            onChange={(drives) => set({ drives })}
          />
        </>
      )}
    </EditorModal>
  );
}

// ── Identity ───────────────────────────────────────────────

type Secret = { mode: 'keep' } | { mode: 'value'; value: string };

export function IdentityEditor({
  ctx,
  identity,
  onClose,
  onSaved,
}: {
  ctx: SuiteCtx;
  identity: SuiteRecord | null;
  onClose: () => void;
  onSaved: (id: string) => void;
}) {
  useLanguage();
  const rdpSpace = ctx.space === 'rdp';
  const d = identity?.data ?? null;
  const secretId = ref(d, 'password_secret_id');
  const hasSecret = Boolean(secretId && ctx.index.get(secretId)?.kind === 'secret');
  const { form, set, dirty } = useForm(() => ({
    label: str(d, 'label'),
    username: str(d, 'username'),
    domain: str(d, 'domain'),
    authType: str(d, 'auth_type') || 'password',
    keyId: ref(d, 'key_id') ?? '',
    password: (hasSecret ? { mode: 'keep' } : { mode: 'value', value: '' }) as Secret,
  }));
  const [busy, setBusy] = useState(false);
  const [generating, setGenerating] = useState(false);
  const keys = ofKind(ctx.records, 'key');
  const usesKey = form.authType === 'key' || form.authType === 'cert';

  const reveal = async () => {
    if (!secretId) return;
    try {
      set({ password: { mode: 'value', value: await suiteReveal(ctx.space, secretId) } });
    } catch (e) {
      toastError(e);
    }
  };

  const save = async () => {
    const id = identity?.id ?? newId();
    const ops: SuiteOp[] = [];
    const patch: Json = {
      label: form.label.trim() || form.username.trim(),
      username: form.username.trim(),
      auth_type: form.authType,
      // Only a key login picks a key; any other leaves the pointer as it was.
      key_id: usesKey ? form.keyId || null : ref(d, 'key_id'),
    };
    if (rdpSpace && (form.domain.trim() || d?.domain !== undefined))
      patch.domain = form.domain.trim();
    if (form.password.mode === 'value') {
      const value = form.password.value;
      if (value) {
        if (hasSecret && secretId) {
          ops.push({ op: 'secret', id: secretId, text: value, seq: ctx.index.get(secretId)?.seq });
        } else {
          const fresh = newId();
          ops.push({ op: 'secret', id: fresh, text: value });
          patch.password_secret_id = fresh;
        }
      } else if (secretId) {
        patch.password_secret_id = null;
        if (hasSecret) ops.push({ op: 'delete', id: secretId });
      } else if (!identity) {
        patch.password_secret_id = null;
      }
    }
    // The identity first: the secret it lets go of is deleted after.
    ops.unshift({ op: 'put', id, kind: 'identity', seq: identity?.seq, patch });
    setBusy(true);
    const ok = await ctx.save(ops, identity ? t('Gespeichert ✧') : t('Anmeldung angelegt ✧'));
    setBusy(false);
    if (ok) onSaved(id);
    else onClose();
  };

  const pw = form.password;
  return (
    <>
      <EditorModal
        title={identity ? t('Anmeldung bearbeiten') : t('Neue Anmeldung')}
        dirty={dirty}
        busy={busy}
        canSave={Boolean(form.username.trim() || form.label.trim())}
        onClose={onClose}
        onSubmit={() => void save()}
      >
        <div className="editor-row">
          <Field label={t('Bezeichnung')}>
            <input
              type="text"
              value={form.label}
              autoFocus
              maxLength={200}
              onChange={(e) => set({ label: e.target.value })}
            />
          </Field>
          <Field label={t('Benutzername')}>
            <input
              type="text"
              value={form.username}
              autoCapitalize="off"
              spellCheck={false}
              onChange={(e) => set({ username: e.target.value })}
            />
          </Field>
          {rdpSpace && (
            <Field label={t('Domäne')} hint={t('Leer für ein lokales Konto.')}>
              <input
                type="text"
                value={form.domain}
                autoCapitalize="off"
                onChange={(e) => set({ domain: e.target.value })}
              />
            </Field>
          )}
        </div>
        <div className="editor-row">
          <Field label={t('Anmelden mit')}>
            <select value={form.authType} onChange={(e) => set({ authType: e.target.value })}>
              {[
                ...AUTH_TYPES,
                ...((AUTH_TYPES as readonly string[]).includes(form.authType)
                  ? []
                  : [form.authType]),
              ].map((a) => (
                <option key={a} value={a}>
                  {authLabel(a)}
                </option>
              ))}
            </select>
          </Field>
          {usesKey && (
            <Field label={t('Schlüssel')}>
              <RecordSelect
                records={keys}
                value={form.keyId}
                none={t('Keiner')}
                onChange={(v) => set({ keyId: v })}
              />
            </Field>
          )}
        </div>
        <div className="field">
          <span className="field-label-row">
            <span>{t('Passwort')}</span>
            <span className="field-actions">
              {pw.mode === 'keep' && (
                <button
                  type="button"
                  className="icon-button"
                  onClick={() => void reveal()}
                  title={t('Zeigen')}
                  aria-label={t('{label} zeigen', { label: t('Passwort') })}
                >
                  <Icon name="eye" size={15} />
                </button>
              )}
              <button
                type="button"
                className="icon-button"
                title={t('Passwort erzeugen')}
                aria-label={t('Passwort erzeugen')}
                onClick={() => setGenerating(true)}
              >
                <Icon name="dice" size={15} />
              </button>
              {pw.mode === 'value' && hasSecret && (
                <button
                  type="button"
                  className="icon-button"
                  onClick={() => set({ password: { mode: 'keep' } })}
                  title={t('Unverändert lassen')}
                  aria-label={t('{label} unverändert lassen', { label: t('Passwort') })}
                >
                  <Icon name="history" size={15} />
                </button>
              )}
            </span>
          </span>
          <input
            type="text"
            className="mono"
            value={pw.mode === 'value' ? pw.value : ''}
            placeholder={pw.mode === 'keep' ? '••••••••••••' : undefined}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => {
              if (e.target.value === '' && pw.mode === 'keep') return;
              set({ password: { mode: 'value', value: e.target.value } });
            }}
          />
          {pw.mode === 'keep' ? (
            <small className="field-hint">{t('Bleibt, wie es ist.')}</small>
          ) : hasSecret && pw.value === '' ? (
            <small className="field-hint" data-tone="warn">
              {t('Wird beim Speichern entfernt.')}
            </small>
          ) : null}
        </div>
      </EditorModal>
      {generating && (
        <GeneratorDialog
          onClose={() => setGenerating(false)}
          onUse={(value) => {
            set({ password: { mode: 'value', value } });
            setGenerating(false);
          }}
        />
      )}
    </>
  );
}

// ── Key ────────────────────────────────────────────────────

/** Renaming a key; its halves never change. */
export function KeyEditor({
  ctx,
  record,
  onClose,
  onSaved,
}: {
  ctx: SuiteCtx;
  record: SuiteRecord;
  onClose: () => void;
  onSaved: (id: string) => void;
}) {
  useLanguage();
  const { form, set, dirty } = useForm(() => ({ label: str(record.data, 'label') }));
  const [busy, setBusy] = useState(false);
  const save = async () => {
    setBusy(true);
    const ok = await ctx.save(
      [
        {
          op: 'put',
          id: record.id,
          kind: 'key',
          seq: record.seq,
          patch: { label: form.label.trim() },
        },
      ],
      t('Gespeichert ✧'),
    );
    setBusy(false);
    if (ok) onSaved(record.id);
    else onClose();
  };
  return (
    <EditorModal
      title={t('Schlüssel bearbeiten')}
      dirty={dirty}
      busy={busy}
      canSave={Boolean(form.label.trim())}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <Field label={t('Bezeichnung')}>
        <input
          type="text"
          value={form.label}
          autoFocus
          onChange={(e) => set({ label: e.target.value })}
        />
      </Field>
    </EditorModal>
  );
}

/** A new SSH key: made here (Ed25519), or an existing private key taken in. */
export function NewKeyDialog({
  ctx,
  onClose,
  onSaved,
}: {
  ctx: SuiteCtx;
  onClose: () => void;
  onSaved: (id: string) => void;
}) {
  useLanguage();
  const [mode, setMode] = useState<'generate' | 'import'>('generate');
  const { form, set, dirty } = useForm(() => ({
    label: '',
    comment: '',
    passphrase: '',
    repeat: '',
    privateKey: '',
  }));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const problem = (): string | null => {
    if (!form.label.trim()) return t('Die Bezeichnung fehlt.');
    if (mode === 'generate' && form.passphrase !== form.repeat)
      return t('Die Passphrasen stimmen nicht überein.');
    if (mode === 'import' && !form.privateKey.trim()) return t('Der private Schlüssel fehlt.');
    return null;
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const saved =
        mode === 'generate'
          ? await suiteGenerateKey(
              ctx.space,
              form.label.trim(),
              form.comment.trim(),
              form.passphrase || null,
            )
          : await suiteImportKey(
              ctx.space,
              form.label.trim(),
              form.privateKey,
              form.passphrase || null,
            );
      if (ctx.took(saved) && saved.id) onSaved(saved.id);
      else onClose();
    } catch (e) {
      const kind = failure(e).kind;
      if (kind === 'key-format' || kind === 'key-passphrase') setError(errorText(e));
      else toastError(e);
    } finally {
      setBusy(false);
    }
  };

  const pick = async (file: File | undefined) => {
    if (!file) return;
    if (file.size > 64 * 1024) {
      setError(t('Das ist kein privater Schlüssel (zu groß).'));
      return;
    }
    set({
      privateKey: await file.text(),
      label: form.label || file.name.replace(/\.(pem|key|ppk)$/i, ''),
    });
  };

  return (
    <EditorModal
      title={t('Neuer Schlüssel')}
      dirty={dirty}
      busy={busy}
      canSave={problem() === null}
      error={error ?? (dirty ? problem() : null)}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <div className="segmented" role="radiogroup" aria-label={t('Schlüssel')}>
        <button
          type="button"
          role="radio"
          aria-checked={mode === 'generate'}
          onClick={() => setMode('generate')}
        >
          {t('Neu erzeugen')}
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={mode === 'import'}
          onClick={() => setMode('import')}
        >
          {t('Importieren')}
        </button>
      </div>
      <Field label={t('Bezeichnung')}>
        <input
          type="text"
          value={form.label}
          autoFocus
          maxLength={200}
          onChange={(e) => set({ label: e.target.value })}
        />
      </Field>
      {mode === 'generate' ? (
        <>
          <Field
            label={t('Kommentar')}
            hint={t('Steht hinten im öffentlichen Schlüssel, oft benutzer@rechner.')}
          >
            <input
              type="text"
              value={form.comment}
              autoCapitalize="off"
              spellCheck={false}
              onChange={(e) => set({ comment: e.target.value })}
            />
          </Field>
          <div className="editor-row">
            <Field label={t('Passphrase (optional)')}>
              <input
                type="password"
                value={form.passphrase}
                autoComplete="new-password"
                onChange={(e) => set({ passphrase: e.target.value })}
              />
            </Field>
            <Field label={t('Passphrase wiederholen')}>
              <input
                type="password"
                value={form.repeat}
                autoComplete="new-password"
                onChange={(e) => set({ repeat: e.target.value })}
              />
            </Field>
          </div>
          <p className="field-hint">
            {busy && form.passphrase
              ? t('Verschlüsselt den Schlüssel mit der Passphrase … das dauert einen Moment.')
              : t(
                  'Ed25519, wie ssh-keygen ihn macht. Der private Schlüssel bleibt verschlüsselt in deinem Tresor.',
                )}
          </p>
        </>
      ) : (
        <>
          <div className="field">
            <span className="field-label-row">
              <span>{t('Privater Schlüssel')}</span>
              <span className="field-actions">
                <button type="button" className="quiet" onClick={() => fileRef.current?.click()}>
                  <Icon name="upload" size={14} />
                  {t('Datei wählen …')}
                </button>
              </span>
            </span>
            <textarea
              className="mono"
              rows={6}
              value={form.privateKey}
              spellCheck={false}
              placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
              onChange={(e) => set({ privateKey: e.target.value })}
            />
            <input
              ref={fileRef}
              type="file"
              hidden
              onChange={(e) => void pick(e.target.files?.[0])}
            />
          </div>
          <Field label={t('Passphrase, falls der Schlüssel eine hat')}>
            <input
              type="password"
              value={form.passphrase}
              autoComplete="off"
              onChange={(e) => set({ passphrase: e.target.value })}
            />
          </Field>
        </>
      )}
    </EditorModal>
  );
}

// ── Snippet, port forward ──────────────────────────────────

export function SnippetEditor({
  ctx,
  snippet,
  onClose,
  onSaved,
}: {
  ctx: SuiteCtx;
  snippet: SuiteRecord | null;
  onClose: () => void;
  onSaved: (id: string) => void;
}) {
  useLanguage();
  const d = snippet?.data ?? null;
  const { form, set, dirty } = useForm(() => ({
    label: str(d, 'label'),
    groupPath: str(d, 'group_path'),
    body: str(d, 'body'),
  }));
  const [busy, setBusy] = useState(false);
  const save = async () => {
    const id = snippet?.id ?? newId();
    setBusy(true);
    const ok = await ctx.save(
      [
        {
          op: 'put',
          id,
          kind: 'snippet',
          seq: snippet?.seq,
          patch: {
            label: form.label.trim(),
            body: form.body,
            group_path: form.groupPath.trim() || null,
          },
        },
      ],
      snippet ? t('Gespeichert ✧') : t('Snippet angelegt ✧'),
    );
    setBusy(false);
    if (ok) onSaved(id);
    else onClose();
  };
  return (
    <EditorModal
      title={snippet ? t('Snippet bearbeiten') : t('Neues Snippet')}
      dirty={dirty}
      busy={busy}
      canSave={Boolean(form.label.trim())}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <div className="editor-row">
        <Field label={t('Bezeichnung')}>
          <input
            type="text"
            value={form.label}
            autoFocus
            onChange={(e) => set({ label: e.target.value })}
          />
        </Field>
        <Field label={t('Ordner')}>
          <input
            type="text"
            value={form.groupPath}
            onChange={(e) => set({ groupPath: e.target.value })}
          />
        </Field>
      </div>
      <Field label={t('Befehl')}>
        <textarea
          className="mono"
          rows={6}
          value={form.body}
          spellCheck={false}
          onChange={(e) => set({ body: e.target.value })}
        />
      </Field>
    </EditorModal>
  );
}

export function TunnelEditor({
  ctx,
  hostId,
  tunnel,
  onClose,
}: {
  ctx: SuiteCtx;
  hostId: string;
  tunnel: SuiteRecord | null;
  onClose: () => void;
}) {
  useLanguage();
  const d = tunnel?.data ?? null;
  const { form, set, dirty } = useForm(() => ({
    name: str(d, 'name'),
    kind: str(d, 'kind') || 'local',
    bindAddress: tunnel ? str(d, 'bind_address') : '127.0.0.1',
    bindPort: tunnel ? String(num(d, 'bind_port')) : '',
    targetHost: tunnel ? str(d, 'target_host') : 'localhost',
    targetPort: tunnel ? String(num(d, 'target_port')) : '',
    autostart: bool(d, 'autostart'),
  }));
  const [busy, setBusy] = useState(false);
  const valid =
    port(form.bindPort) !== null &&
    port(form.targetPort) !== null &&
    Boolean(form.targetHost.trim());
  const save = async () => {
    setBusy(true);
    await ctx.save(
      [
        {
          op: 'put',
          id: tunnel?.id ?? newId(),
          kind: 'port_forward',
          seq: tunnel?.seq,
          patch: {
            host_id: hostId,
            name: form.name.trim(),
            kind: form.kind,
            bind_address: form.bindAddress.trim(),
            bind_port: port(form.bindPort),
            target_host: form.targetHost.trim(),
            target_port: port(form.targetPort),
            autostart: form.autostart,
          },
        },
      ],
      t('Gespeichert ✧'),
    );
    setBusy(false);
    onClose();
  };
  const local = form.kind !== 'remote';
  return (
    <EditorModal
      title={tunnel ? t('Weiterleitung bearbeiten') : t('Neue Weiterleitung')}
      dirty={dirty}
      busy={busy}
      canSave={valid}
      error={
        dirty && !valid ? t('Ports sind 1 bis 65535, und das Ziel braucht eine Adresse.') : null
      }
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <div className="editor-row">
        <Field label={t('Name')}>
          <input
            type="text"
            value={form.name}
            autoFocus
            onChange={(e) => set({ name: e.target.value })}
          />
        </Field>
        <Field label={t('Richtung')}>
          <select value={form.kind} onChange={(e) => set({ kind: e.target.value })}>
            <option value="local">{t('Lokal (-L)')}</option>
            <option value="remote">{t('Entfernt (-R)')}</option>
            {!['local', 'remote'].includes(form.kind) && (
              <option value={form.kind}>{form.kind}</option>
            )}
          </select>
        </Field>
      </div>
      <div className="editor-row">
        <Field label={local ? t('Lauscht hier auf') : t('Lauscht auf dem Server auf')}>
          <input
            type="text"
            className="mono"
            value={form.bindAddress}
            spellCheck={false}
            onChange={(e) => set({ bindAddress: e.target.value })}
          />
        </Field>
        <Field label={t('Port')}>
          <input
            type="text"
            inputMode="numeric"
            className="mono narrow"
            value={form.bindPort}
            onChange={(e) => set({ bindPort: e.target.value })}
          />
        </Field>
      </div>
      <div className="editor-row">
        <Field label={local ? t('Ziel, vom Server aus') : t('Ziel, von hier aus')}>
          <input
            type="text"
            className="mono"
            value={form.targetHost}
            spellCheck={false}
            onChange={(e) => set({ targetHost: e.target.value })}
          />
        </Field>
        <Field label={t('Port')}>
          <input
            type="text"
            inputMode="numeric"
            className="mono narrow"
            value={form.targetPort}
            onChange={(e) => set({ targetPort: e.target.value })}
          />
        </Field>
      </div>
      <Check
        label={t('Mit jedem Terminal zu diesem Host starten')}
        checked={form.autostart}
        onChange={(v) => set({ autostart: v })}
      />
    </EditorModal>
  );
}

/** Which editor opens for a record of `kind`. */
export type EditorTarget =
  | {
      kind: Exclude<SuiteKind, 'secret' | 'known_host' | 'port_forward'>;
      record: SuiteRecord | null;
    }
  | { kind: 'port_forward'; record: SuiteRecord | null; hostId: string };
