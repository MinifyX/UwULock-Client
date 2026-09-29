/**
 * UwULock Server's extras: the extras key, masked addresses, sharing an item as a Send, and
 * the owner's file requests (read only). All of it runs here in the background: the popup and
 * the content scripts never talk to the server and never see a key.
 *
 * - The **extras key** (contract §3) is only opened here, never made or wrapped again: the
 *   web vault and the desktop app do that. Without it — none made yet, or lost in a key
 *   rotation — what needs it (own icons of personal items, file requests) stays off.
 * - **Masked addresses** (§13.3) are made for the site of the tab, which is taken from the
 *   sender (a content script's tab) or the active tab (the popup), never from a message.
 * - **Sends** are Bitwarden's, so sharing works with Vaultwarden and Bitwarden too; only the
 *   link on a send domain (§14.2) needs UwULock Server.
 */

import type {
  FileRequestEntry,
  FileRequests,
  MaskedAddress,
  MaskedConnection,
  SendDomain,
  ShareableField,
  ShareOptions,
  SharedSend,
} from '../shared/protocol';
import * as clipboard from './clipboard';
import { ApiError, request, uwu } from './http';
import { endpoints } from './server';
import type { Account } from './store';
import { hasFeature } from './uwu';
import * as vault from './vault';
import { call, callJson } from './wasm';

type Json = Record<string, unknown>;

function isObject(value: unknown): value is Json {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

const text = (value: unknown): string | null =>
  typeof value === 'string' && value !== '' ? value : null;

function lowerKeys(value: unknown): Json {
  if (!isObject(value)) return {};
  return Object.fromEntries(Object.entries(value).map(([k, v]) => [k.toLowerCase(), v]));
}

function featureOff(): never {
  throw { kind: 'uwu:feature_off', message: 'The server does not offer this.' };
}

// ── The extras key ────────────────────────────────────────

export type ExtrasState = 'open' | 'none' | 'lost';

let extras: {
  accountId: string;
  closings: number;
  generation: number;
  state: Promise<ExtrasState>;
  settled?: ExtrasState;
} | null = null;

async function openExtras(account: Account): Promise<ExtrasState> {
  let keys: unknown;
  try {
    keys = await uwu(account, '/keys');
  } catch (error) {
    // Switched off, or a server that doesn't know it yet: nothing to open.
    if (error instanceof ApiError && error.status === 404) return 'none';
    throw error;
  }
  try {
    const answer = await callJson<{ state: ExtrasState }>((core) =>
      core.openExtras(JSON.stringify(keys)),
    );
    return answer.state;
  } catch (error) {
    // Neither wrap opens with this account's keys: for this client that is lost.
    if ((error as { kind?: string }).kind === 'crypto') return 'lost';
    throw error;
  }
}

/**
 * The extras key, opened in the WebAssembly module once per unlocked vault. A failure to reach
 * the server is tried again on the next call; so is "none" or "lost" after the next sync — the
 * web vault or the desktop app may have made the key meanwhile.
 */
export function extrasKey(account: Account): Promise<ExtrasState> {
  if (!hasFeature(account, 'own-icons') && !hasFeature(account, 'file-requests'))
    return Promise.resolve('none');
  if (
    extras &&
    extras.accountId === account.id &&
    extras.closings === vault.closedCount() &&
    (extras.settled === 'open' || extras.generation === vault.generation())
  )
    return extras.state;
  const state = openExtras(account);
  const entry: NonNullable<typeof extras> = {
    accountId: account.id,
    closings: vault.closedCount(),
    generation: vault.generation(),
    state,
  };
  extras = entry;
  state.then(
    (found) => {
      entry.settled = found;
    },
    () => {
      if (extras === entry) extras = null;
    },
  );
  return state;
}

// ── Masked addresses ──────────────────────────────────────

/** The site an address is for: the page's origin, or nothing for anything but a web page. */
export function forDomain(url: string | null | undefined): string {
  if (!url) return '';
  try {
    const parsed = new URL(url);
    return parsed.protocol === 'https:' || parsed.protocol === 'http:' ? parsed.origin : '';
  } catch {
    return '';
  }
}

/** The web vault's page where an account connects to UwUMail. */
export function maskedSettingsUrl(account: Account): string {
  return `${endpoints(account.server).web}/#/settings/masked`;
}

export async function maskedConnection(account: Account): Promise<MaskedConnection> {
  if (!hasFeature(account, 'masked-addresses')) featureOff();
  const found = await uwu<Json>(account, '/masked/connection');
  return {
    connected: found.connected === true,
    status: text(found.status),
    server: text(found.server),
    username: text(found.username),
    defaultDomain: text(found.defaultDomain),
    settingsUrl: maskedSettingsUrl(account),
  };
}

/** A new masked address at the account's UwUMail, for the page at `url`. */
export async function createMasked(
  account: Account,
  url: string | null,
  cipherId: string | null,
): Promise<MaskedAddress> {
  if (!hasFeature(account, 'masked-addresses')) featureOff();
  const created = await uwu<Json>(account, '/masked/addresses', {
    body: {
      forDomain: forDomain(url),
      description: '',
      domain: null,
      emailPrefix: null,
      cipherId: cipherId && /^[0-9a-f-]{36}$/i.test(cipherId) ? cipherId : null,
    },
  });
  const email = text(created.email);
  if (!email || !email.includes('@'))
    throw { kind: 'server', message: 'The server answered without an address.' };
  return { id: String(created.id ?? ''), email, forDomain: text(created.forDomain) };
}

// ── Sharing an item as a Send ─────────────────────────────

export const shareFields = (id: string) =>
  callJson<ShareableField[]>((core) => core.shareableFields(id));

const HOUR = 3_600_000;

function checkOptions(options: ShareOptions): ShareOptions {
  const fields = Array.isArray(options?.fields) ? options.fields : [];
  const clean = fields
    .filter(
      (pair): pair is [string, string] =>
        Array.isArray(pair) && typeof pair[0] === 'string' && typeof pair[1] === 'string',
    )
    .slice(0, 200);
  if (!clean.length) throw { kind: 'invalid', message: 'Choose at least one value to share.' };
  const hours = Number(options.deletionHours);
  const count = options.maxAccessCount;
  return {
    fields: clean,
    // Bitwarden allows at most 31 days.
    deletionHours: Number.isFinite(hours) ? Math.min(Math.max(Math.round(hours), 1), 31 * 24) : 24,
    maxAccessCount:
      typeof count === 'number' && Number.isFinite(count)
        ? Math.min(Math.max(Math.round(count), 1), 1_000_000)
        : null,
    password: typeof options.password === 'string' && options.password ? options.password : null,
  };
}

/** The account's default send domain, if the server has it (§14.2); else the main host. */
async function sendDomain(account: Account): Promise<SendDomain | null> {
  const domains = account.uwu?.sendDomains ?? [];
  if (!domains.length) return null;
  try {
    const found = await uwu<Json>(account, '/account');
    return domains.find((domain) => domain.id === found.sendDomainId) ?? null;
  } catch {
    return null;
  }
}

/**
 * Shares chosen values of an item as a new text Send (Bitwarden's `POST /api/sends`), and
 * answers its link: on the account's send domain if it has one, else the web vault's.
 */
export async function shareItem(
  account: Account,
  id: string,
  options: ShareOptions,
): Promise<SharedSend> {
  const checked = checkOptions(options);
  const deletionDate = new Date(Date.now() + checked.deletionHours * HOUR).toISOString();
  const body = await callJson<Json>((core) =>
    core.sealShare(
      id,
      JSON.stringify({
        fields: checked.fields,
        deletionDate,
        maxAccessCount: checked.maxAccessCount,
        password: checked.password,
      }),
    ),
  );
  const created = lowerKeys(await request(account, '/api/sends', { body }));
  const accessId = text(created.accessid);
  const key = text(created.key) ?? text(body.key);
  if (!accessId || !key) throw { kind: 'server', message: 'The server answered without a link.' };
  const domain = await sendDomain(account);
  const base = domain?.url ?? endpoints(account.server).web;
  const link = await call((core) => core.sendLink(key, accessId, base, domain !== null));
  return {
    id: String(created.id ?? ''),
    link,
    deletionDate: text(created.deletiondate) ?? deletionDate,
    onSendDomain: domain !== null,
  };
}

// ── File requests ─────────────────────────────────────────

/** The requests as the server sent them, for their links; forgotten when the vault closes. */
let requests = { closings: -1, byId: new Map<string, Json>() };

async function listRequests(account: Account): Promise<Json[]> {
  const all: Json[] = [];
  let token: string | null = null;
  for (let page = 0; page < 20; page += 1) {
    const query: string = token ? `?continuationToken=${encodeURIComponent(token)}` : '';
    const answer = await uwu<unknown>(account, `/file-requests${query}`);
    const data = Array.isArray(answer) ? answer : isObject(answer) ? answer.data : [];
    if (Array.isArray(data))
      all.push(...data.filter((r): r is Json => isObject(r) && !!text(r.id)));
    token = isObject(answer) ? text(answer.continuationToken) : null;
    if (!token) break;
  }
  return all;
}

function entry(found: Json, label: string | null, web: string, now: number): FileRequestEntry {
  const id = String(found.id);
  const expirationDate = text(found.expirationDate);
  const count = (value: unknown) => (typeof value === 'number' && value >= 0 ? value : 0);
  return {
    id,
    label,
    expirationDate,
    submissionCount: count(found.submissionCount),
    maxSubmissions: typeof found.maxSubmissions === 'number' ? found.maxSubmissions : null,
    unseen: count(found.unseen),
    disabled: found.disabled === true,
    expired: expirationDate !== null && Date.parse(expirationDate) <= now,
    manageUrl: `${web}/#/file-requests/${encodeURIComponent(id)}`,
  };
}

export async function fileRequests(account: Account): Promise<FileRequests> {
  if (!hasFeature(account, 'file-requests')) featureOff();
  const web = endpoints(account.server).web;
  const webUrl = `${web}/#/file-requests`;
  const state = await extrasKey(account);
  if (state !== 'open') return { state, requests: [], webUrl };
  const found = await listRequests(account);
  requests = {
    closings: vault.closedCount(),
    byId: new Map(found.map((r) => [String(r.id), r])),
  };
  const labels = await callJson<{ id: string; label: string | null }[]>((core) =>
    core.fileRequestLabels(JSON.stringify(found)),
  );
  const byId = new Map(labels.map((l) => [l.id, l.label]));
  const now = Date.now();
  return {
    state: 'open',
    webUrl,
    requests: found.map((r) => entry(r, byId.get(String(r.id)) ?? null, web, now)),
  };
}

/** Copies a file request's link (§11.1): on its send domain if it names one, else the main host. */
export async function copyFileRequestLink(account: Account, id: string): Promise<void> {
  if (!hasFeature(account, 'file-requests')) featureOff();
  if ((await extrasKey(account)) !== 'open')
    throw { kind: 'uwu:no_extras_key', message: 'The extras key is not there.' };
  const known = requests.closings === vault.closedCount() ? requests.byId.get(id) : undefined;
  const found = known ?? (await uwu<Json>(account, `/file-requests/${encodeURIComponent(id)}`));
  const domain = (account.uwu?.sendDomains ?? []).find((d) => d.id === found.sendDomainId) ?? null;
  const base = domain?.url ?? endpoints(account.server).web;
  const link = await call((core) =>
    core.fileRequestLink(JSON.stringify(found), base, domain !== null),
  );
  await clipboard.copy(link);
}
