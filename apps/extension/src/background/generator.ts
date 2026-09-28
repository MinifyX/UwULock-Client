/**
 * The generator: passwords and passphrases from uwulock-core, with the settings the popup
 * saved. The last twenty results are kept until the browser closes, so a password generated
 * for a form that then failed isn't lost.
 */

import type { Generated, GeneratorSettings } from '../shared/protocol';
import { settings } from './settings';
import { session, setSession } from './store';
import { callJson } from './wasm';

const HISTORY = 20;

export async function generate(options?: GeneratorSettings): Promise<string> {
  return (await generateWithBits(options)).password;
}

export async function generateWithBits(
  options?: GeneratorSettings,
): Promise<{ password: string; bits: number }> {
  const chosen = options ?? (await settings()).generator;
  const result =
    chosen.mode === 'passphrase'
      ? await callJson<{ password: string; bits: number }>((core) =>
          core.passphrase(JSON.stringify(chosen.passphrase)),
        )
      : await callJson<{ password: string; bits: number }>((core) =>
          core.generate(JSON.stringify(chosen.password)),
        );
  const history = (await session('generated')) ?? [];
  await setSession(
    'generated',
    [{ password: result.password, date: Date.now() }, ...history].slice(0, HISTORY),
  );
  return result;
}

export async function history(): Promise<Generated[]> {
  return (await session('generated')) ?? [];
}

export async function clearHistory(): Promise<void> {
  await setSession('generated', []);
}
