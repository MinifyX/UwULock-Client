/**
 * UwULock Server's extras, as the page sees them: which of them the server
 * offers, the marks on items (due reminders, masked addresses, own icons),
 * and the calls into Rust behind each. An account on Bitwarden or
 * Vaultwarden gets an empty status and sees none of it, except sharing an
 * item as a Send.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useSyncExternalStore } from 'react';
import type { Organization, Status } from './api';

export type SendDomain = { id: string; url: string };

export type UwuStatus = {
  uwu: boolean;
  features: string[];
  travel: { enabled: boolean; hiddenCount: number | null };
  unseen: { securityNotices: number; fileRequestSubmissions: number };
  organizations: Organization[];
  sendDomains: SendDomain[];
  reminders: Record<string, { due: string | null; everyMonths: number | null; isDue: boolean }>;
  masked: Record<string, { id: string; email: string }>;
  ownIcons: Record<string, string | null>;
  automaticIcons: boolean;
  limits: {
    maxFileBytes?: number | null;
    versionsPerItem?: number | null;
    versionDays?: number | null;
    fileRequestMaxFiles?: number | null;
    fileRequestMaxDays?: number | null;
  } | null;
  /** The extras key isn't the one this device took before: warn until seen. */
  extrasKeyChanged: boolean;
};

export const EMPTY_STATUS: UwuStatus = {
  uwu: false,
  features: [],
  travel: { enabled: false, hiddenCount: null },
  unseen: { securityNotices: 0, fileRequestSubmissions: 0 },
  organizations: [],
  sendDomains: [],
  reminders: {},
  masked: {},
  ownIcons: {},
  automaticIcons: false,
  limits: null,
  extrasKeyChanged: false,
};

export type Feature =
  | 'icons'
  | 'own-icons'
  | 'versions'
  | 'travel-mode'
  | 'reminders'
  | 'file-requests'
  | 'masked-addresses'
  | 'send-domains'
  | 'send-emails'
  | 'suite';

export const has = (status: UwuStatus, feature: Feature) => status.features.includes(feature);

// ── The status, shared by every component ─────────────────

let current: UwuStatus = EMPTY_STATUS;
const subscribers = new Set<() => void>();
let started = false;

function set(next: UwuStatus) {
  current = next;
  for (const notify of subscribers) notify();
}

export function refreshUwu() {
  void invoke<UwuStatus>('uwu_status')
    .then(set)
    .catch(() => set(EMPTY_STATUS));
}

function start() {
  if (started) return;
  started = true;
  refreshUwu();
  void listen<UwuStatus>('uwu-changed', ({ payload }) => set(payload));
  // Locking, unlocking and switching accounts change whose status it is.
  // Locked, the page keeps no icon: own icons were encrypted.
  void listen<Status>('vault-status', ({ payload }) => {
    if (payload.state !== 'unlocked') forgetIcons();
    refreshUwu();
  });
}

/** The extras of the account on screen; draws again when they change. */
export function useUwu(): UwuStatus {
  start();
  return useSyncExternalStore(
    (notify) => {
      subscribers.add(notify);
      return () => subscribers.delete(notify);
    },
    () => current,
  );
}

// ── Icons ──────────────────────────────────────────────────

const icons = new Map<string, string>();
const iconSubscribers = new Set<() => void>();
let iconVersion = 0;

function forgetIcons() {
  if (!icons.size) return;
  icons.clear();
  iconVersion += 1;
  for (const notify of iconSubscribers) notify();
}

/**
 * Asks Rust for the icons of these items and keeps them. Items asked for
 * again (after a sync, or a new own icon) are answered from Rust's own cache.
 */
export async function loadIcons(ids: string[], automatic: boolean) {
  if (!ids.length) return;
  const found = await invoke<Record<string, string>>('item_icons', { ids, automatic }).catch(
    () => null,
  );
  if (!found) return;
  let changed = false;
  for (const id of ids) {
    const url = found[id];
    if (url && icons.get(id) !== url) {
      icons.set(id, url);
      changed = true;
    } else if (!url && icons.has(id)) {
      icons.delete(id);
      changed = true;
    }
  }
  if (changed) {
    iconVersion += 1;
    for (const notify of iconSubscribers) notify();
  }
}

/** The icon of an item as a `data:` URL, once it is known. */
export function useItemIcon(id: string): string | undefined {
  useSyncExternalStore(
    (notify) => {
      iconSubscribers.add(notify);
      return () => iconSubscribers.delete(notify);
    },
    () => iconVersion,
  );
  return icons.get(id);
}

export const setOwnIcon = (id: string, png: string) => invoke<void>('set_own_icon', { id, png });
export const fetchDeviceIcon = (id: string) => invoke<void>('fetch_device_icon', { id });
export const deleteOwnIcon = (id: string) => invoke<void>('delete_own_icon', { id });

/** Whether a host is on the local network: its icon comes from the device. */
export function isLocalHost(host: string | null | undefined): boolean {
  if (!host) return false;
  const h = host
    .toLowerCase()
    .replace(/^\[|\]$/g, '')
    .replace(/\.$/, '');
  if (/^\d+\.\d+\.\d+\.\d+$/.test(h)) {
    const [a, b] = h.split('.').map(Number) as [number, number];
    return (
      a === 10 ||
      a === 127 ||
      (a === 172 && b >= 16 && b < 32) ||
      (a === 192 && b === 168) ||
      (a === 169 && b === 254) ||
      (a === 100 && b >= 64 && b < 128)
    );
  }
  if (h.includes(':')) return /^(::1|f[cd]|fe[89ab])/.test(h);
  if (!h.includes('.')) return true;
  const last = h.slice(h.lastIndexOf('.') + 1);
  return [
    'local',
    'lan',
    'home',
    'internal',
    'intranet',
    'localhost',
    'localdomain',
    'corp',
    'private',
    'arpa',
  ].includes(last);
}

// ── Travel mode, links to the web vault ────────────────────

export const uwuTravel = () =>
  invoke<{ enabled: boolean; hiddenCount: number | null }>('uwu_travel');

export type WebVaultPlace = 'organization' | 'masked' | 'travel' | 'keys' | 'file-requests';
export const openWebVaultAt = (place: WebVaultPlace, id?: string) =>
  invoke<void>('open_web_vault_at', { place, id: id ?? null });

// ── Entry versions ─────────────────────────────────────────

export type Change = {
  field: string;
  label: string | null;
  kind: 'changed' | 'added' | 'removed';
  secret: boolean;
  before: string | null;
  after: string | null;
};

export type Version = {
  id: string;
  revisionDate: string | null;
  replacedDate: string | null;
  size: number;
  broken: boolean;
  changes: Change[];
};

export const itemVersions = (id: string) => invoke<Version[]>('item_versions', { id });
/** Without a version: the item as it is now, named the same way. */
export const revealVersionField = (id: string, versionId: string | null, field: string) =>
  invoke<string>('reveal_version_field', { id, versionId, field });
export const restoreVersion = (id: string, versionId: string) =>
  invoke<void>('restore_version', { id, versionId });
/** Without a version: all of them. */
export const deleteVersions = (id: string, versionId?: string) =>
  invoke<void>('delete_versions', { id, versionId: versionId ?? null });

// ── Reminders ──────────────────────────────────────────────

export const setReminder = (id: string, due: string | null, everyMonths: number | null) =>
  invoke<void>('set_reminder', { id, due, everyMonths });
export const deleteReminder = (id: string) => invoke<void>('delete_reminder', { id });

/** The person saw that the extras key changed: it is the one from now on. */
export const extrasKeySeen = () => invoke<void>('uwu_extras_key_seen');

// ── File requests ──────────────────────────────────────────

export type FileRequest = {
  id: string;
  label: string | null;
  title: string | null;
  note: string | null;
  owner: string | null;
  link: string | null;
  passwordSet: boolean;
  expirationDate: string | null;
  deletionDate: string | null;
  maxSubmissions: number | null;
  submissionCount: number;
  maxFiles: number;
  maxFileBytes: number | null;
  textAllowed: boolean;
  sendDomainId: string | null;
  disabled: boolean;
  unseen: number;
  bytes: number;
  /** Its details encrypt for a key that isn't the account's own: no link. */
  foreignKey: boolean;
};

export type FileRequestInput = {
  label: string;
  title: string;
  note: string | null;
  owner: string | null;
  /** On a change, `null` keeps the date. */
  expiresInDays: number | null;
  maxSubmissions: number | null;
  maxFiles: number;
  maxFileMib: number | null;
  textAllowed: boolean;
  password: string | null;
  sendDomainId: string | null;
  disabled: boolean;
};

export type Submission = {
  id: string;
  creationDate: string | null;
  seen: boolean;
  text: string | null;
  senderName: string | null;
  senderEmail: string | null;
  /** `risky`: a type that runs when opened (a program, a script, a macro document). */
  files: { id: string; name: string | null; size: number; risky: boolean }[];
  broken: boolean;
};

export const fileRequests = () => invoke<FileRequest[]>('file_requests');
export const createFileRequest = (input: FileRequestInput) =>
  invoke<FileRequest>('create_file_request', { input });
export const updateFileRequest = (
  id: string,
  input: FileRequestInput,
  newLink: boolean,
  removePassword: boolean,
) => invoke<FileRequest>('update_file_request', { id, input, newLink, removePassword });
export const deleteFileRequest = (id: string) => invoke<void>('delete_file_request', { id });
export const fileRequestSubmissions = (requestId: string) =>
  invoke<Submission[]>('file_request_submissions', { requestId });
/** Saves into the Downloads folder; returns the path. */
export const saveSubmissionFile = (
  requestId: string,
  submissionId: string,
  fileId: string,
  allowRisky: boolean,
) => invoke<string>('save_submission_file', { requestId, submissionId, fileId, allowRisky });
export const markSubmissionSeen = (requestId: string, submissionId: string) =>
  invoke<void>('mark_submission_seen', { requestId, submissionId });
export const deleteSubmission = (requestId: string, submissionId: string) =>
  invoke<void>('delete_submission', { requestId, submissionId });
export const takeOverSubmission = (
  requestId: string,
  submissionId: string,
  name: string,
  senderLabel: string,
) => invoke<string>('take_over_submission', { requestId, submissionId, name, senderLabel });

// ── Masked addresses ───────────────────────────────────────

export type MaskedConnection = {
  connected: boolean;
  server: string | null;
  username: string | null;
  domains: string[] | null;
  defaultDomain: string | null;
  status: 'ok' | 'revoked' | 'unreachable' | null;
};

export type MaskedAddress = {
  id: string;
  email: string;
  state: string;
  forDomain: string | null;
  description: string | null;
  createdAt: string | null;
  lastMessageAt: string | null;
  cipherId: string | null;
  itemName?: string | null;
};

export const maskedConnection = () => invoke<MaskedConnection>('masked_connection');
export const maskedAddresses = () => invoke<MaskedAddress[]>('masked_addresses');
export const createMaskedAddress = (
  forDomain: string | null,
  description: string | null,
  cipherId: string | null,
) => invoke<MaskedAddress>('create_masked_address', { forDomain, description, cipherId });
export const setMaskedState = (id: string, state: 'enabled' | 'disabled') =>
  invoke<MaskedAddress>('update_masked_address', { id, addressState: state, link: null });
/** Links the address to an item. */
export const linkMaskedAddress = (id: string, cipherId: string) =>
  invoke<MaskedAddress>('update_masked_address', { id, addressState: null, link: cipherId });
export const deleteMaskedAddress = (id: string) => invoke<void>('delete_masked_address', { id });

// ── Sharing as a Send ──────────────────────────────────────

export type SendOptions = {
  emails: boolean;
  domains: SendDomain[];
  defaultDomainId: string | null;
};

export type ShareInput = {
  fields: [string, string][];
  deletionDays: number;
  maxAccess: number | null;
  password: string | null;
  emails: string[];
  sendDomainId: string | null;
  hideText: boolean;
  /**
   * An entry Send: UwULock's Send page shows it as an entry. Only then may
   * `fields` name `totp` (live codes, never the key in the readable text).
   */
  entry?: boolean;
};

export const sendOptions = () => invoke<SendOptions>('send_options');
export const shareAsSend = (id: string, input: ShareInput) =>
  invoke<{ id: string; link: string; deletionDate: string }>('share_as_send', { id, input });
