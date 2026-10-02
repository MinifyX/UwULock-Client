/**
 * The popup's calls into the background, typed — the same calls the desktop app's page makes
 * into Rust. Everything secret stays in the background: the popup gets names, usernames and
 * notes, and a password only when someone asks to see it (`revealField`). Copying goes from
 * the background straight to the clipboard.
 */

import { ext } from '../shared/browser';
import { ask } from '../shared/messages';
import type {
  Draft,
  FileRequests,
  MaskedAddress,
  MaskedConnection,
  ShareableField,
  SharedSend,
  ShareOptions,
  Generated,
  GeneratorSettings,
  ItemDetail,
  ItemSummary,
  LoginStep,
  Overview,
  PasskeyDecision,
  PasskeyInfo,
  PasskeyPrompt,
  PendingSave,
  SaveAnswer,
  ServerChoice,
  Settings,
  Status,
  TabItems,
  TotpCode,
} from '../shared/protocol';
import { endpoints } from '../background/server';

export { RequestFailed } from '../shared/messages';

export const vaultStatus = () => ask<Status>({ type: 'status' });

/**
 * Logging in needs the permission to reach the server. The browser asks the person, and only
 * during a click: so this is the first thing the button does, before anything is awaited.
 */
export function requestServerPermission(server: ServerChoice): Promise<boolean> {
  let origins: string[];
  try {
    origins = endpoints(server).origins;
  } catch {
    return Promise.resolve(false);
  }
  return ext.permissions.request({ origins }).catch(() => false);
}

export const login = (server: ServerChoice, email: string, password: string) =>
  ask<LoginStep>({ type: 'login', server, email, password });
export const loginTwoFactor = (provider: number, code: string, remember: boolean) =>
  ask<LoginStep>({ type: 'login-two-factor', provider, code, remember });
export const loginWebAuthn = (remember: boolean) => ask<void>({ type: 'login-webauthn', remember });
export const loginNewDevice = (code: string) => ask<LoginStep>({ type: 'login-new-device', code });
export const loginSendEmail = () => ask<void>({ type: 'login-send-email' });
export const loginCancel = () => ask<void>({ type: 'login-cancel' });
export const forgetKdf = (server: ServerChoice, email: string) =>
  ask<void>({ type: 'forget-kdf', server, email });
export const unlock = (password: string) => ask<Status>({ type: 'unlock', password });
export const unlockWithPin = (pin: string) => ask<Status>({ type: 'unlock-pin', pin });
export const setPin = (pin: string | null, afterRestart: boolean) =>
  ask<Status>({ type: 'set-pin', pin, afterRestart });
export const lock = () => ask<Status>({ type: 'lock' });
export const logout = (id?: string) => ask<Status>({ type: 'logout', id });
export const switchAccount = (id: string) => ask<Status>({ type: 'switch-account', id });
export const touch = () => ask<void>({ type: 'touch' });
export const syncNow = () => ask<Status>({ type: 'sync' });

export const vaultOverview = () => ask<Overview>({ type: 'overview' });
export const vaultItems = () => ask<ItemSummary[]>({ type: 'items' });
export const vaultItem = (id: string) => ask<ItemDetail>({ type: 'item', id });
export const revealField = (id: string, field: string) =>
  ask<string>({ type: 'reveal', id, field });
export const copyField = (id: string, field: string) => ask<void>({ type: 'copy', id, field });
export const copyText = (text: string) => ask<void>({ type: 'copy-text', text });
export const totpCode = (id: string) => ask<TotpCode>({ type: 'totp', id });
export const verifyReprompt = (id: string, password: string) =>
  ask<void>({ type: 'verify-reprompt', id, password });
export const saveItem = (id: string | null, draft: Draft) =>
  ask<string>({ type: 'save-item', id, draft });
export const setFavorite = (id: string, favorite: boolean) =>
  ask<void>({ type: 'set-favorite', id, favorite });
export const deleteItem = (id: string, permanent: boolean) =>
  ask<void>({ type: 'delete-item', id, permanent });
export const restoreItem = (id: string) => ask<void>({ type: 'restore-item', id });
export const saveFolder = (id: string | null, name: string) =>
  ask<string>({ type: 'save-folder', id, name });
export const openItemUri = (id: string, index: number) =>
  ask<void>({ type: 'open-uri', id, index });
export const itemPasskeys = (id: string) => ask<PasskeyInfo[]>({ type: 'item-passkeys', id });
export const deletePasskey = (id: string, index: number, credentialId: string | null) =>
  ask<void>({ type: 'delete-passkey', id, index, credentialId });

/** `length` is raised to `required` when the minimums need more. */
export const generate = (settings: GeneratorSettings) =>
  ask<{ password: string; bits: number; length?: number; required?: number }>({
    type: 'generate',
    settings,
  });
export const generatorHistory = () => ask<Generated[]>({ type: 'generator-history' });
export const clearGeneratorHistory = () => ask<void>({ type: 'clear-generator-history' });

export const getSettings = () => ask<Settings>({ type: 'settings' });
export const setSettings = (patch: Partial<Settings>) =>
  ask<Settings>({ type: 'set-settings', patch });

export const tabItems = () => ask<TabItems>({ type: 'tab-items' });
export const fillTab = (id: string, confirmedInsecure = false, password?: string) =>
  ask<void>({ type: 'fill-tab', id, confirmedInsecure, ...(password ? { password } : {}) });
export const pendingSaves = () => ask<PendingSave[]>({ type: 'pending-saves' });
export const answerPendingSave = (id: string, answer: SaveAnswer) =>
  ask<void>({ type: 'answer-pending-save', id, answer });

export const passkeyPrompt = (id: string) => ask<PasskeyPrompt>({ type: 'passkey-prompt', id });
export const passkeyDecide = (decision: PasskeyDecision) =>
  ask<void>({ type: 'passkey-decide', decision });

// ── UwULock Server's extras ───────────────────────────────

export const itemIcons = (ids: string[]) => ask<Record<string, string>>({ type: 'icons', ids });
export const maskedConnection = () => ask<MaskedConnection>({ type: 'masked-connection' });
export const createMasked = (cipherId: string | null) =>
  ask<MaskedAddress>({ type: 'masked-create', cipherId });
export const shareFields = (id: string) => ask<ShareableField[]>({ type: 'share-fields', id });
export const shareItem = (id: string, options: ShareOptions) =>
  ask<SharedSend>({ type: 'share-item', id, options });
export const fileRequests = () => ask<FileRequests>({ type: 'file-requests' });
export const copyFileRequestLink = (id: string) =>
  ask<void>({ type: 'copy-file-request-link', id });
