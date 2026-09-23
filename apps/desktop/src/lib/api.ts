/**
 * The calls into Rust, typed. Everything secret stays there: the page gets
 * names, usernames and notes, and a password only when someone asks to see
 * it (`revealField`). Copying goes from Rust straight to the clipboard.
 */

import { invoke } from '@tauri-apps/api/core';

export type Failure = { kind: string; message: string };

/** Errors from Rust arrive as `{ kind, message }`; anything else becomes one. */
export function failure(error: unknown): Failure {
  if (typeof error === 'object' && error !== null && 'kind' in error) return error as Failure;
  return { kind: 'unknown', message: String(error) };
}

export type ServerKind = 'bitwarden-us' | 'bitwarden-eu' | 'self-hosted';

/** One account in the switcher. */
export type AccountBrief = {
  id: string;
  label: string;
  email: string;
  name: string | null;
  server: string;
  serverKind: ServerKind;
  unlocked: boolean;
  active: boolean;
  lastSync: number | null;
};

export type Status = {
  state: 'logged-out' | 'locked' | 'unlocked';
  accountId: string | null;
  label: string | null;
  email: string | null;
  name: string | null;
  server: string | null;
  serverKind: ServerKind | null;
  serverUrl: string | null;
  lastSync: number | null;
  syncing: boolean;
  syncError: string | null;
  sessionExpired: boolean;
  accounts: AccountBrief[];
};

export type TwoFactorMethod = {
  provider: number;
  kind: 'authenticator' | 'email' | 'yubikey' | 'duo' | 'webauthn' | 'u2f' | 'other';
  supported: boolean;
  hint: string | null;
};

export type LoginStep =
  | { step: 'done'; status: Status }
  | { step: 'two-factor'; methods: TwoFactorMethod[]; message: string | null }
  | { step: 'new-device' };

export type ItemKind = 'login' | 'note' | 'card' | 'identity' | 'ssh-key';

export type ItemSummary = {
  id: string;
  kind: ItemKind;
  name: string;
  subtitle: string | null;
  host: string | null;
  favorite: boolean;
  folderId: string | null;
  organizationId: string | null;
  collectionIds: string[];
  deleted: boolean;
  reprompt: boolean;
  hasTotp: boolean;
  hasPassword: boolean;
  hasUsername: boolean;
  broken: boolean;
  revisionDate: string | null;
};

export type Folder = { id: string; name: string };
export type Collection = { id: string; organizationId: string; name: string };
export type Organization = { id: string; name: string };
export type Overview = {
  folders: Folder[];
  collections: Collection[];
  organizations: Organization[];
  skipped: number;
};

export type FieldKind = 'text' | 'hidden' | 'boolean' | 'linked';

export type ItemDetail = {
  summary: ItemSummary;
  locked: boolean;
  notes?: string | null;
  login?: {
    username: string | null;
    hasPassword: boolean;
    hasTotp: boolean;
    passwordRevisionDate: string | null;
    uris: { uri: string; match: number | null; host: string | null; openable: boolean }[];
    passkeys: number;
  } | null;
  card?: {
    cardholderName: string | null;
    brand: string | null;
    numberEnding: string | null;
    expMonth: string | null;
    expYear: string | null;
    hasCode: boolean;
  } | null;
  identity?: { name: string; sensitive: boolean; value: string | null }[] | null;
  sshKey?: { publicKey: string | null; fingerprint: string | null; hasPrivateKey: boolean } | null;
  fields?: {
    index: number;
    name: string | null;
    kind: FieldKind;
    value: string | null;
    hasValue: boolean;
  }[];
  passwordHistory?: { index: number; lastUsed: string | null }[];
  attachments?: number;
  creationDate?: string | null;
};

export type TotpCode = { code: string; remaining: number; period: number };

/**
 * What the editor sends back. A secret it never had — a password nobody
 * looked at, a card number, a hidden field — stays `null`, and Rust keeps the
 * value the item already has; `''` clears it.
 */
export type Draft = {
  kind: ItemKind;
  name: string;
  notes?: string | null;
  favorite: boolean;
  reprompt: boolean;
  folderId: string | null;
  login?: {
    username?: string | null;
    password?: string | null;
    totp?: string | null;
    uris: { uri: string; match: number | null }[];
  };
  card?: {
    cardholderName?: string | null;
    brand?: string | null;
    number?: string | null;
    expMonth?: string | null;
    expYear?: string | null;
    code?: string | null;
  };
  /** By field name; a name that isn't in here keeps its value. */
  identity?: Record<string, string>;
  sshKey?: {
    privateKey?: string | null;
    publicKey?: string | null;
    fingerprint?: string | null;
  };
  fields: {
    name: string | null;
    kind: FieldKind;
    value?: string | null;
    /** Which field of the item this one was, for a value the editor never saw. */
    from: number | null;
  }[];
};

export type GeneratorOptions = {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  avoidAmbiguous: boolean;
};

export type UpdateInfo = { version: string; notes: string | null };
export type ProjectPage = 'source' | 'releases' | 'issues' | 'license' | 'suite';

export const vaultStatus = () => invoke<Status>('vault_status');
export const login = (
  server: { kind: ServerKind; url?: string },
  email: string,
  password: string,
) => invoke<LoginStep>('login', { server, email, password });
export const loginTwoFactor = (provider: number, code: string, remember: boolean) =>
  invoke<LoginStep>('login_two_factor', { provider, code, remember });
export const loginNewDevice = (code: string) => invoke<LoginStep>('login_new_device', { code });
export const loginSendEmail = () => invoke<void>('login_send_email');
export const loginCancel = () => invoke<void>('login_cancel');
export const unlock = (password: string) => invoke<Status>('unlock', { password });
export const lock = () => invoke<void>('lock');
/** Without an id: the account on screen. The others stay. */
export const logout = (id?: string) => invoke<Status>('logout', { id: id ?? null });
export const switchAccount = (id: string) => invoke<Status>('switch_account', { id });
export const renameAccount = (id: string, label: string) =>
  invoke<Status>('rename_account', { id, label });
export const touch = () => invoke<void>('touch');
export const setSecurity = (autoLockMinutes: number | null, clipboardSeconds: number | null) =>
  invoke<void>('set_security', { autoLockMinutes, clipboardSeconds });
export const syncNow = () => invoke<Status>('sync_now');
export const vaultOverview = () => invoke<Overview>('vault_overview');
export const vaultItems = () => invoke<ItemSummary[]>('vault_items');
export const vaultItem = (id: string) => invoke<ItemDetail>('vault_item', { id });
export const verifyReprompt = (id: string, password: string) =>
  invoke<void>('verify_reprompt', { id, password });
export const revealField = (id: string, field: string) =>
  invoke<string>('reveal_field', { id, field });
export const copyField = (id: string, field: string) => invoke<void>('copy_field', { id, field });
export const copyGenerated = (text: string) => invoke<void>('copy_generated', { text });
export const totpCode = (id: string) => invoke<TotpCode>('totp_code', { id });
export const generatePassword = (options: GeneratorOptions) =>
  invoke<{ password: string; bits: number }>('generate_password', { options });

/** Without an id: a new item. Returns the item's id. */
export const saveItem = (id: string | null, draft: Draft) =>
  invoke<string>('save_item', { id, draft });
export const setFavorite = (id: string, favorite: boolean) =>
  invoke<void>('set_favorite', { id, favorite });
export const setItemFolder = (id: string, folderId: string | null) =>
  invoke<void>('set_item_folder', { id, folderId });
export const deleteItem = (id: string, permanent: boolean) =>
  invoke<void>('delete_item', { id, permanent });
export const restoreItem = (id: string) => invoke<void>('restore_item', { id });
/** Without an id: a new folder. */
export const saveFolder = (id: string | null, name: string) =>
  invoke<string>('save_folder', { id, name });
export const deleteFolder = (id: string) => invoke<void>('delete_folder', { id });
export const openItemUri = (id: string, index: number) =>
  invoke<void>('open_item_uri', { id, index });
export const openWebVault = () => invoke<void>('open_web_vault');

export const setUpdateChannel = (channel: 'stable' | 'beta') =>
  invoke<void>('set_update_channel', { channel });
export const updateStatus = () => invoke<UpdateInfo | null>('update_status');
export const checkForUpdates = () => invoke<UpdateInfo | null>('check_for_updates');
export const installUpdate = () => invoke<void>('install_update');
export const openProjectPage = (page: ProjectPage) => invoke<void>('open_project_page', { page });
