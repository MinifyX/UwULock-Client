/**
 * Moving a vault in from Bitwarden or Vaultwarden: the calls into Rust and
 * what comes back. The source account is logged in inside the move dialog
 * and never kept; see `src-tauri/src/moving.rs`.
 */

import { invoke } from '@tauri-apps/api/core';
import type { ServerKind, TwoFactorMethod } from './api';

export type MoveTarget = { uwulock: boolean; families: boolean; email: string; server: string };

export type MoveCount = { total: number; moved: number; todo: number };

export type MoveNoticeCode =
  | 'trash'
  | 'broken'
  | 'read-only'
  | 'org-members'
  | 'orgs-as-folders'
  | 'send-password'
  | 'send-emails'
  | 'send-file-locked'
  | 'send-expired'
  | 'send-file-counted'
  | 'too-large';

export type MovePreview = {
  source: string;
  sourceEmail: string;
  target: string;
  targetEmail: string;
  families: boolean;
  folders: MoveCount;
  organizations: MoveCount;
  collections: MoveCount;
  items: MoveCount;
  attachments: MoveCount;
  sends: MoveCount;
  fileBytes: number;
  notices: { code: MoveNoticeCode; count: number }[];
};

export type MoveStep =
  | { step: 'done'; preview: MovePreview }
  | { step: 'two-factor'; methods: TwoFactorMethod[]; message: string | null }
  | { step: 'new-device' };

export type MoveKind = 'folder' | 'organization' | 'collection' | 'item' | 'attachment' | 'send';

export type MoveProgress = { done: number; total: number; kind: MoveKind };

export type MoveTally = {
  folders: number;
  organizations: number;
  collections: number;
  items: number;
  attachments: number;
  sends: number;
};

export type MoveFinished = {
  summary: {
    moved: MoveTally;
    failed: { kind: MoveKind; message: string }[];
    orgsAsFolders: number;
  };
  cancelled: boolean;
};

export const moveTarget = () => invoke<MoveTarget>('move_target');
export const moveLogin = (
  server: { kind: ServerKind; url?: string },
  email: string,
  password: string,
) => invoke<MoveStep>('move_login', { server, email, password });
export const moveLoginTwoFactor = (provider: number, code: string) =>
  invoke<MoveStep>('move_login_two_factor', { provider, code });
export const moveLoginNewDevice = (code: string) =>
  invoke<MoveStep>('move_login_new_device', { code });
export const moveLoginSendEmail = () => invoke<void>('move_login_send_email');
export const moveStart = () => invoke<void>('move_start');
export const moveCancel = () => invoke<void>('move_cancel');
export const moveClose = () => invoke<void>('move_close');
