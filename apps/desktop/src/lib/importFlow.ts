/**
 * The import's steps, shared by the desktop dialog (components/ImportDialog.tsx)
 * and the phone's page (mobile/pages/Import.tsx): pick the app (or let UwULock
 * tell), pick the file, give a KeePass file or a password-protected Bitwarden
 * export its password, look at what will come in, import, see what came in.
 * What the file holds stays in memory only until the import or the cancel.
 */

import { useCallback, useRef, useState } from 'react';
import {
  isProtectedExport,
  needsPassword,
  readImport,
  type ImportFile,
  type Parsed,
  type Source,
} from './import/index.ts';
import { appKdf } from './import/kdf.ts';
import { checkFileSize } from './import/limits.ts';
import {
  importErrorText,
  importParsed,
  openProtectedExport,
  type ImportOutcome,
  type ImportProgress,
} from './import/run.ts';

/**
 * The file picker's filter on a computer. A phone gets none: iOS greys out files whose type it
 * doesn't know (.kdbx, .1pux), Android's pickers are just as picky.
 */
export const IMPORT_ACCEPT =
  '.json,.csv,.kdbx,.1pux,.zip,.xml,.pgp,application/json,text/csv,application/zip';

export type ImportStep =
  | { name: 'pick' }
  | { name: 'reading' }
  | { name: 'password'; file: ImportFile; kind: 'keepass' | 'bitwarden' }
  | { name: 'preview'; parsed: Parsed }
  | { name: 'running'; parsed: Parsed; progress: ImportProgress | null }
  // Done: only the outcome; the file's plaintext isn't kept once it is in the vault.
  | { name: 'done'; outcome: ImportOutcome };

/** Lets the page draw ("Öffnet …") before a key derivation holds anything up. */
const nextFrame = () =>
  new Promise<void>((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));

export function useImportFlow(initial?: Parsed) {
  const [source, setSource] = useState<Source | 'auto'>('auto');
  const [step, setStep] = useState<ImportStep>(
    initial ? { name: 'preview', parsed: initial } : { name: 'pick' },
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // A cancel while a file is being read: what it reads is dropped.
  const generation = useRef(0);

  const reset = useCallback(() => {
    generation.current += 1;
    setStep({ name: 'pick' });
    setError(null);
    setBusy(false);
  }, []);

  const pick = async (chosen: File | null | undefined) => {
    if (!chosen) return;
    const run = ++generation.current;
    setError(null);
    try {
      // Before the file is read into memory at all.
      checkFileSize(chosen.size);
      setStep({ name: 'reading' });
      const file = { name: chosen.name, bytes: new Uint8Array(await chosen.arrayBuffer()) };
      if (run !== generation.current) return;
      if (needsPassword(file.bytes) && (source === 'auto' || source === 'keepass')) {
        setStep({ name: 'password', file, kind: 'keepass' });
      } else if (isProtectedExport(file.bytes) && (source === 'auto' || source === 'bitwarden')) {
        setStep({ name: 'password', file, kind: 'bitwarden' });
      } else {
        const parsed = await readImport(file, source);
        if (run === generation.current) setStep({ name: 'preview', parsed });
      }
    } catch (e) {
      if (run !== generation.current) return;
      setError(importErrorText(e));
      setStep({ name: 'pick' });
    }
  };

  /** The password (and, for KeePass, the key file) of the file in the `password` step. */
  const unlockFile = async (password: string, keyFile: Uint8Array | null) => {
    if (step.name !== 'password' || busy) return;
    const { file, kind } = step;
    const run = generation.current;
    setBusy(true);
    setError(null);
    await nextFrame();
    try {
      const parsed =
        kind === 'keepass'
          ? await readImport(file, 'keepass', {
              credentials: { password, keyFile },
              kdf: appKdf,
            })
          : await readImport(await openProtectedExport(file, password), 'bitwarden');
      if (run === generation.current) setStep({ name: 'preview', parsed });
    } catch (e) {
      if (run === generation.current) setError(importErrorText(e));
    } finally {
      if (run === generation.current) setBusy(false);
    }
  };

  const start = async () => {
    if (step.name !== 'preview') return;
    const { parsed } = step;
    setError(null);
    setStep({ name: 'running', parsed, progress: null });
    try {
      const outcome = await importParsed(parsed, {
        onProgress: (progress) => setStep((s) => (s.name === 'running' ? { ...s, progress } : s)),
      });
      setStep({ name: 'done', outcome });
    } catch (e) {
      // Nothing came in: back to the preview, with the reason.
      setError(importErrorText(e));
      setStep({ name: 'preview', parsed });
    }
  };

  return { source, setSource, step, error, busy, pick, unlockFile, start, reset };
}

export type ImportFlow = ReturnType<typeof useImportFlow>;
