/**
 * The link for Mello (MelloAI-Desktop, docs/mello.md): a read-only endpoint
 * on 127.0.0.1 with counts only — locked or not, entries, weak and reused
 * passwords. Off by default; the token is what Mello needs to pair.
 */

import { invoke } from '@tauri-apps/api/core';

export type MelloLink = {
  enabled: boolean;
  /** Whether the endpoint is listening right now. */
  running: boolean;
  url: string;
  /** Only while the link is on. */
  token: string | null;
  /** Why it isn't listening, e.g. the port is taken. */
  problem: string | null;
};

export const melloLink = () => invoke<MelloLink>('mello_link');

export const setMelloLink = (enabled: boolean) => invoke<MelloLink>('set_mello_link', { enabled });

/** A new token; Mello has to be paired again. */
export const renewMelloLink = () => invoke<MelloLink>('mello_link_renew');
