/** The extension's settings, with defaults; kept in `storage.local`. */

import type { Settings } from '../shared/protocol';
import { local, setLocal } from './store';

export const DEFAULT_SETTINGS: Settings = {
  language: 'system',
  theme: 'system',
  lockTimeout: 15,
  clipboardClear: 30,
  inlineMenu: true,
  savePrompt: true,
  copyTotp: true,
  passkeys: true,
  showIcons: true,
  neverSave: [],
  defaultMatch: 0,
  generator: {
    mode: 'password',
    password: {
      length: 20,
      lowercase: true,
      uppercase: true,
      digits: true,
      symbols: true,
      avoidAmbiguous: false,
    },
    passphrase: { words: 5, separator: '-', capitalize: true, includeNumber: true },
  },
};

export async function settings(): Promise<Settings> {
  const saved = (await local('settings')) ?? {};
  return {
    ...DEFAULT_SETTINGS,
    ...saved,
    generator: {
      ...DEFAULT_SETTINGS.generator,
      ...(saved as Partial<Settings>).generator,
    },
  };
}

const TIMEOUTS = new Set([0, 1, 5, 15, 30, 60, 240, -1]);

/** Only known keys, with values of the right kind: the popup is trusted, but not infallible. */
export async function updateSettings(patch: Partial<Settings>): Promise<Settings> {
  const next: Settings = { ...(await settings()) };
  if (patch.language && ['system', 'de', 'en'].includes(patch.language))
    next.language = patch.language;
  if (patch.theme && ['system', 'light', 'dark'].includes(patch.theme)) next.theme = patch.theme;
  if (patch.lockTimeout !== undefined && TIMEOUTS.has(patch.lockTimeout))
    next.lockTimeout = patch.lockTimeout;
  if (typeof patch.clipboardClear === 'number')
    next.clipboardClear = Math.max(0, Math.min(600, Math.round(patch.clipboardClear)));
  for (const key of ['inlineMenu', 'savePrompt', 'copyTotp', 'passkeys', 'showIcons'] as const) {
    if (typeof patch[key] === 'boolean') next[key] = patch[key];
  }
  if (Array.isArray(patch.neverSave))
    next.neverSave = patch.neverSave.filter((h) => typeof h === 'string').slice(0, 500);
  if (typeof patch.defaultMatch === 'number' && patch.defaultMatch >= 0 && patch.defaultMatch <= 5)
    next.defaultMatch = patch.defaultMatch;
  if (patch.generator) {
    const g = patch.generator;
    next.generator = {
      mode: g.mode === 'passphrase' ? 'passphrase' : 'password',
      password: { ...next.generator.password, ...g.password },
      passphrase: { ...next.generator.passphrase, ...g.passphrase },
    };
  }
  await setLocal('settings', next);
  return next;
}
