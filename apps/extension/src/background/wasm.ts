/**
 * uwulock-core, compiled to WebAssembly (crates/uwulock-wasm): the keys, the crypto and the
 * opened vault live in its memory, in the background only. Loaded once per start of the
 * background; every call goes through here, which turns its JSON back into objects and its
 * errors into `{ kind, message }`.
 */

import init, * as wasm from '../wasm/pkg/core.js';
import { ext } from '../shared/browser';
import type { Failure } from '../shared/protocol';

let ready: Promise<unknown> | null = null;

/** Load the module; every call below waits for this first. Tests pass the bytes. */
export function load(bytes?: BufferSource): Promise<unknown> {
  ready ??= init({ module_or_path: bytes ?? ext.runtime.getURL('core.wasm') });
  return ready;
}

function failure(error: unknown): Failure {
  if (typeof error === 'string') {
    try {
      const parsed = JSON.parse(error) as Failure;
      if (parsed && typeof parsed.kind === 'string') return parsed;
    } catch {
      // Not ours: a plain message.
    }
    return { kind: 'crypto', message: error };
  }
  if (error instanceof Error) return { kind: 'crypto', message: error.message };
  return { kind: 'unknown', message: String(error) };
}

export type Core = typeof wasm;

/** Call into the module; throws a `Failure`. */
export async function call<T>(work: (core: Core) => T): Promise<T> {
  await load();
  try {
    return work(wasm);
  } catch (error) {
    throw failure(error);
  }
}

/** The same, for calls that answer JSON. */
export async function callJson<T>(work: (core: Core) => string): Promise<T> {
  return JSON.parse(await call(work)) as T;
}
