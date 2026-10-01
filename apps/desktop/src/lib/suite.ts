/**
 * The calls into Rust for the sections "SSH (UwUSSH)" and "Remote Desktop
 * (UwURDP)" (`src-tauri/src/suite.rs`). Rust opens and seals the records,
 * keeps the secrets and checks every change; the page gets the records' JSON
 * and a secret only when someone asks to see it.
 */

import { invoke } from '@tauri-apps/api/core';
import type { SuiteOp, SuiteSpace, SuiteView } from './suiteModel';

export type SuiteSaved = {
  view: SuiteView;
  /** Records the server had newer copies of: taken over, nothing overwritten. */
  conflicts: string[];
  /** The key a generation or import made. */
  id?: string;
};

export const suiteView = (space: SuiteSpace) => invoke<SuiteView>('suite_view', { space });

export const suiteCreate = (space: SuiteSpace) => invoke<SuiteView>('suite_create', { space });

export const suiteSave = (space: SuiteSpace, ops: SuiteOp[]) =>
  invoke<SuiteSaved>('suite_save', { space, ops });

export const suiteReveal = (space: SuiteSpace, id: string) =>
  invoke<string>('suite_reveal', { space, id });

export const suiteCopy = (space: SuiteSpace, id: string) =>
  invoke<void>('suite_copy', { space, id });

export const suiteGenerateKey = (
  space: SuiteSpace,
  label: string,
  comment: string,
  passphrase: string | null,
) => invoke<SuiteSaved>('suite_generate_key', { space, label, comment, passphrase });

export const suiteImportKey = (
  space: SuiteSpace,
  label: string,
  privateKey: string,
  passphrase: string | null,
) => invoke<SuiteSaved>('suite_import_key', { space, label, privateKey, passphrase });

/** Saves a key's `public` line or `private` half as a file; answers where. */
export const suiteSaveKey = (space: SuiteSpace, id: string, half: 'public' | 'private') =>
  invoke<string>('suite_save_key', { space, id, half });

/** A `.rdp` file of a UwURDP host, without password and drives. */
export const suiteSaveRdp = (id: string) => invoke<string>('suite_save_rdp', { id });

/** `uwussh://connect/<id>` / `uwurdp://connect/<id>` (computers only). */
export const suiteOpenInApp = (space: SuiteSpace, id: string) =>
  invoke<void>('suite_open_in_app', { space, id });
