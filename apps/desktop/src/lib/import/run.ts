/**
 * The import, into the vault: what the screens (components/ImportDialog.tsx,
 * mobile/pages/Import.tsx) and the system's hand-over (Apple's Credential
 * Exchange) call. Reading a file is `index.ts`; here the result goes to Rust
 * (`src-tauri/src/importing.rs`), which seals the items under the user key and
 * sends them to the server in parts, then syncs.
 *
 * ── For the native side (Credential Exchange and the like) ──────────────────
 *
 *   importCollected(collected, options) → Promise<ImportOutcome>
 *
 * `collected` is Bitwarden's unencrypted JSON export (`BitwardenExport`,
 * types.ts): `{ encrypted: false, folders: [{ id, name }], items: ExportItem[] }`.
 * An item has `type` (1 login, 2 note, 3 card, 4 identity, 5 SSH key), `name`,
 * `notes`, `favorite`, `reprompt` (0), `folderId` (an id from `folders`, or
 * null), `fields` ({ name, value, type: 0 text, 1 hidden, 2 boolean }) and the
 * part of its type: `login` ({ username, password, totp — an otpauth:// URI or
 * the bare key —, uris: [{ uri, match: null }], fido2Credentials?:
 * ExportPasskey[] — plain, base64url }), `secureNote: { type: 0 }`, `card`,
 * `identity`, `sshKey`. The easiest way to build it is `new Collector()`
 * (collect.ts): `login(name)`, `uris()`, `field()`, `add(item, folderName)`,
 * then `collector.result().data`; it keeps every value the item has no place
 * for as a custom field, as all importers do.
 *
 * `options.onProgress({ done, total })` follows the upload. The outcome says
 * how many came in, which items stayed out and why, and — when the import
 * stopped part way — the error (what came in before stays). It throws (a
 * `{ kind, message }` from Rust, `errorText` words it) only when nothing could
 * start: the vault is locked, or what was handed over isn't an export.
 *
 * To show the same preview first: `parsedFromCollected(collected)` gives the
 * `Parsed` that `ImportPreview` (components/ImportDialog.tsx) takes.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { failure, type Failure } from '../api';
import { errorText } from '../errors';
import { t } from '../i18n';
import { ImportError } from './bytes.ts';
import { setTranslator } from './i18n.ts';
import type { BitwardenExport, ImportFile, Parsed, Source } from './types.ts';

// The module's words in the app's language from here on.
setTranslator(t);

export type ImportProgress = { done: number; total: number };

/** `invalid`: Rust can't save it (an SSH key without all its parts, …); `too-long`: its notes. */
export type SkippedItem = { name: string; reason: 'invalid' | 'too-long' };

export type ImportOutcome = {
  imported: number;
  foldersCreated: number;
  skipped: SkippedItem[];
  /** Set when the import stopped part way. */
  error: Failure | null;
};

export type ImportOptions = { onProgress?: (progress: ImportProgress) => void };

async function submit(
  format: 'json' | 'csv',
  text: string,
  options: ImportOptions,
): Promise<ImportOutcome> {
  const stop = options.onProgress
    ? await listen<ImportProgress>('import-progress', ({ payload }) =>
        options.onProgress?.(payload),
      )
    : null;
  try {
    return await invoke<ImportOutcome>('import_vault', { format, text });
  } finally {
    stop?.();
  }
}

/** Items handed over as Bitwarden's unencrypted JSON export, into the vault on screen. */
export function importCollected(
  collected: BitwardenExport,
  options: ImportOptions = {},
): Promise<ImportOutcome> {
  return submit('json', JSON.stringify(collected), options);
}

/** A file read by `readImport`, into the vault on screen. */
export function importParsed(parsed: Parsed, options: ImportOptions = {}): Promise<ImportOutcome> {
  return submit(parsed.submit.format, parsed.submit.text, options);
}

/** What `ImportPreview` shows for items that didn't come from a file. */
export function parsedFromCollected(
  collected: BitwardenExport,
  meta: { source?: Source; format?: string; warnings?: string[] } = {},
): Parsed {
  return {
    source: meta.source ?? 'credential-exchange',
    format: meta.format ?? 'CXF',
    data: collected,
    warnings: meta.warnings ?? [],
    submit: { format: 'json', text: JSON.stringify(collected) },
  };
}

/** Bitwarden's password-protected export, opened with its password: a file to read like any other. */
export async function openProtectedExport(file: ImportFile, password: string): Promise<ImportFile> {
  const text = await invoke<string>('import_open_bitwarden', {
    text: new TextDecoder().decode(file.bytes),
    password,
  });
  return { name: file.name, bytes: new TextEncoder().encode(text) };
}

/** An import's error in words: the module's own, or one from Rust. */
export function importErrorText(error: unknown): string {
  if (error instanceof ImportError) return error.message;
  switch (failure(error).kind) {
    case 'import-password':
      return t('Das Passwort der Datei stimmt nicht.');
    case 'import-account-bound':
      return t(
        'Dieser Export ist mit dem Konto verschlüsselt, das ihn erstellt hat. Exportiere noch einmal als JSON – unverschlüsselt oder „Passwortgeschützt“ – und importiere diese Datei.',
      );
    case 'import-unsupported':
      return t('Diese Datei verlangt etwas, das UwULock nicht kann: {reason}', {
        reason: failure(error).message,
      });
    case 'import-file':
      return t(
        'Die Datei ließ sich nicht lesen: Sie ist beschädigt oder anders aufgebaut als erwartet.',
      );
  }
  return errorText(error);
}

/** Why an item stayed out, in words. */
export function skippedText(reason: SkippedItem['reason']): string {
  return reason === 'too-long'
    ? t('Die Notiz ist länger, als der Server annimmt.')
    : t('Der Eintrag ist unvollständig (etwa ein SSH-Schlüssel ohne alle Teile).');
}
