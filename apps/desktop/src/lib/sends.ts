/**
 * The account's own Sends, the calls into Rust behind them (`sends.rs`):
 * list, save (text or file), open to anybody again, delete. Rust seals and
 * opens them; the page only holds what it shows. The model — status, dates,
 * the draft — is in `sendModel.ts`.
 *
 * Bitwarden and Vaultwarden have Sends; a UwULock Server offers them unless
 * its features leave `sends` out ({@link sendsAvailable}).
 */

import { invoke } from '@tauri-apps/api/core';
import type { SendDraft, Send } from './sendModel';
import { has, type UwuStatus } from './uwu';

export type { Send, SendDraft } from './sendModel';

/** Whether the server has Sends: every Bitwarden and Vaultwarden does. */
export const sendsAvailable = (status: UwuStatus) => !status.uwu || has(status, 'sends');

/** Newest change first. Fresh after every save: Rust syncs before it answers. */
export const sends = () => invoke<Send[]>('sends');

/**
 * Hands the file of a new file Send to Rust as raw bytes (no JSON on the
 * way): a file from the desktop's picker, or a phone's photo or document
 * picker. The next {@link saveSend} of a file Send takes it.
 */
export async function stageSendFile(file: Blob): Promise<void> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  await invoke<void>('stage_send_file', bytes);
}

/**
 * Saves a Send: a new one (`id` null) or a change. A new file Send needs
 * {@link stageSendFile} first. `domain`: where its link points — a send
 * domain's id, `null` the server's own address, `undefined` leaves it.
 * Answers its id.
 */
export const saveSend = (id: string | null, draft: SendDraft, domain?: string | null) =>
  invoke<string>('save_send', {
    id,
    draft,
    domain: domain ?? null,
    chooseDomain: domain !== undefined,
  });

/** Anybody with the link may open it again: no password, no addresses. */
export const removeSendAuth = (id: string) => invoke<void>('remove_send_auth', { id });

export const deleteSend = (id: string) => invoke<void>('delete_send', { id });
