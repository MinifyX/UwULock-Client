/**
 * UwULock as a passkey provider for the system (src-tauri/src/passkeys): the
 * settings, and the dialog's side of a request from a browser or app. Rust
 * keeps the request and the vault; the page only learns names.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { t } from './i18n';

export type PasskeyProviderSettings = {
  /** Linux: the virtual security key over /dev/uhid. */
  securityKey: boolean;
  /** Windows 11: the plugin passkey manager (experimental). */
  windowsPlugin: boolean;
  /** iOS and macOS: the AutoFill extension's sealed list. */
  appleExtension: boolean;
};

/**
 * Another program stands in for UwULock (docs/passkeys.md, "Another program
 * of yours"): `held` on Linux (it holds the one security key per user),
 * `registry` on Windows (the entry that starts UwULock for a request
 * pointed elsewhere; UwULock put it back).
 */
export type PasskeyProviderWarning = {
  kind: 'held' | 'registry';
  /** The program, as far as UwULock could tell. */
  holder: string | null;
};

export type PasskeyProviderStatus = {
  settings: PasskeyProviderSettings;
  platform: 'linux' | 'windows' | 'android' | 'apple' | 'none';
  active: boolean;
  problem: string | null;
  warning: PasskeyProviderWarning | null;
};

/** The warning in words, for the settings and the note. */
export function warningText(warning: PasskeyProviderWarning): string {
  const holder = warning.holder ?? t('ein unbekanntes Programm');
  return warning.kind === 'held'
    ? t(
        'Ein anderes Programm hält deinen UwULock-Sicherheitsschlüssel: {holder}. Browser reden gerade nicht mit UwULock. Kennst du das Programm nicht, beende es und prüfe deinen Rechner.',
        { holder },
      )
    : t(
        'Der Windows-Eintrag, der UwULock für Passkey-Anfragen startet, zeigte auf ein anderes Programm: {holder}. UwULock hat ihn zurückgesetzt. Kennst du das Programm nicht, prüfe deinen Rechner.',
        { holder },
      );
}

/** Calls `onWarning` whenever the warning comes or goes; returns the unlisten. */
export const onPasskeyProviderWarning = (
  onWarning: (warning: PasskeyProviderWarning | null) => void,
) =>
  listen<PasskeyProviderWarning | null>('passkey-provider-warning', ({ payload }) =>
    onWarning(payload),
  );

export type PasskeyRequest = {
  id: number;
  kind: 'create' | 'get' | 'select';
  /** Who asks: "Firefox", "Windows", a program's name; empty when unknown. */
  client: string;
  /** A browser UwULock knows, or a request Windows signed. */
  trusted: boolean;
  rpId: string | null;
  rpName: string | null;
  userName: string | null;
  userDisplayName: string | null;
  locked: boolean;
  /** The site wants the master password typed again. */
  verify: boolean;
  /** The vault already has a passkey the site knows. */
  excluded: boolean;
  logins: { itemId: string; name: string; userName: string | null; hasPasskey: boolean }[];
  passkeys: {
    itemId: string;
    itemName: string;
    credentialId: string;
    userName: string | null;
    userDisplayName: string | null;
  }[];
};

export const passkeyProviderStatus = () => invoke<PasskeyProviderStatus>('passkey_provider_status');

export const setPasskeyProvider = (settings: PasskeyProviderSettings) =>
  invoke<PasskeyProviderStatus>('set_passkey_provider', { settings });

export const passkeyRequest = () => invoke<PasskeyRequest | null>('passkey_request');

export const passkeyAnswer = (answer: {
  id: number;
  allow: boolean;
  itemId?: string | null;
  credentialId?: string | null;
  password?: string | null;
}) => invoke<void>('passkey_answer', answer);
