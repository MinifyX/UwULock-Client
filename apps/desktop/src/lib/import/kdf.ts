/**
 * KeePass's key derivations for the app: Argon2d, Argon2id and AES-KDF in Rust
 * (uwulock-core's `import`, the commands in `src-tauri/src/importing.rs`), off
 * the page's thread. With KeePassXC's defaults that takes a second or two.
 */

import { invoke } from '@tauri-apps/api/core';
import type { KdbxKdf } from './types.ts';

const bytes = (list: number[]) => new Uint8Array(list);

export const appKdf: KdbxKdf = {
  argon2: async (id, version, key, salt, memoryKiB, iterations, lanes) =>
    bytes(
      await invoke<number[]>('import_kdbx_argon2', {
        id,
        version,
        key: Array.from(key),
        salt: Array.from(salt),
        memoryKib: memoryKiB,
        iterations,
        lanes,
      }),
    ),
  aesKdf: async (key, seed, rounds) =>
    bytes(
      await invoke<number[]>('import_kdbx_aes_kdf', {
        key: Array.from(key),
        seed: Array.from(seed),
        rounds,
      }),
    ),
};
