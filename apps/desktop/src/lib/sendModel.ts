/**
 * Sends without React or Tauri: the shape Rust hands them over in, the
 * status a list shows, the dates the editor offers, and the draft it saves —
 * the web vault's semantics (UwULock-Server's `SendsView.tsx`). Tested on its
 * own (`apps/desktop/test/sends.test.ts`, plain `node --test`), so no imports
 * with effects in here. The calls into Rust are in `sends.ts`.
 */

export type SendKind = 0 | 1;

/** Who may open a Send: 0 only `emails`, with a code by mail; 1 with the password; 2 anybody. */
export type SendAccess = 0 | 1 | 2;

export type Send = {
  id: string;
  kind: SendKind;
  name: string;
  /** Only for the owner. */
  notes: string | null;
  /** An entry Send's without its marker line. */
  text: string | null;
  hidden: boolean;
  fileName: string | null;
  size: number | null;
  maxAccessCount: number | null;
  accessCount: number;
  hasPassword: boolean;
  authType: SendAccess;
  emails: string[];
  disabled: boolean;
  hideEmail: boolean;
  revisionDate: string | null;
  expirationDate: string | null;
  deletionDate: string | null;
  /** Shared from an item as an entry: its text is not edited. */
  entry: boolean;
  link: string;
  /** The send domain its link uses; `null` the server's own address. */
  sendDomainId: string | null;
};

/** What the editor saves (uwulock-core's `SendDraft`). */
export type SendDraft = {
  kind: SendKind;
  name: string;
  notes: string | null;
  text: string | null;
  hidden: boolean;
  fileName: string | null;
  /** A new password; `null` keeps the one there is. */
  password: string | null;
  maxAccessCount: number | null;
  expirationDate: string | null;
  deletionDate: string;
  disabled: boolean;
  hideEmail: boolean;
  authType: SendAccess;
  emails: string[];
};

export type SendStatus =
  | { kind: 'disabled' }
  | { kind: 'expired' }
  | { kind: 'used-up' }
  | { kind: 'active'; until: string | null };

/** Whether a Send opens right now, and until when. */
export function sendStatus(send: Send, now = Date.now()): SendStatus {
  if (send.disabled) return { kind: 'disabled' };
  if (send.expirationDate && Date.parse(send.expirationDate) < now) return { kind: 'expired' };
  if (send.maxAccessCount && send.accessCount >= send.maxAccessCount) return { kind: 'used-up' };
  return { kind: 'active', until: send.expirationDate ?? send.deletionDate };
}

/** The days the editor offers for deleting and expiring (a Send lives at most 31). */
export const SEND_DAYS = [1, 2, 3, 7, 14, 30] as const;

const DAY = 86_400_000;

/** In `days` days, as the server takes it. */
export function inDays(days: number, now = Date.now()): string {
  return new Date(now + days * DAY).toISOString();
}

/** A date as the nearest of {@link SEND_DAYS} from now; none: 7. */
export function daysUntil(iso: string | null | undefined, now = Date.now()): number {
  if (!iso) return 7;
  const days = Math.round((Date.parse(iso) - now) / DAY);
  return SEND_DAYS.reduce<number>(
    (best, n) => (Math.abs(n - days) < Math.abs(best - days) ? n : best),
    7,
  );
}

/** Addresses as somebody types them: separated by commas, spaces or lines. */
export function splitAddresses(text: string): string[] {
  return text
    .split(/[\s,;]+/)
    .map((address) => address.trim().toLowerCase())
    .filter(Boolean);
}

/** The editor's fields, as it holds them. */
export type SendForm = {
  name: string;
  text: string;
  hidden: boolean;
  notes: string;
  /** The picked file's name, for a new file Send. */
  fileName: string | null;
  password: string;
  /** As typed; empty is unlimited. */
  maxAccess: string;
  /** One of {@link SEND_DAYS}. */
  deletionDays: number;
  /** 0: never (until it is deleted). */
  expiresDays: number;
  disabled: boolean;
  hideEmail: boolean;
  access: SendAccess;
  /** As typed. */
  emails: string;
};

/** The form for a Send, or a new one of `kind`. */
export function sendForm(send: Send | null, now = Date.now()): SendForm {
  return {
    name: send?.name ?? '',
    text: send?.text ?? '',
    hidden: send?.hidden ?? false,
    notes: send?.notes ?? '',
    fileName: null,
    password: '',
    maxAccess: send?.maxAccessCount ? String(send.maxAccessCount) : '',
    deletionDays: daysUntil(send?.deletionDate ?? null, now),
    expiresDays: send?.expirationDate ? daysUntil(send.expirationDate, now) : 0,
    disabled: send?.disabled ?? false,
    hideEmail: send?.hideEmail ?? false,
    access: send?.authType ?? 2,
    emails: send?.emails.join(', ') ?? '',
  };
}

/**
 * Whether the form can be saved: a name, the text (or for a new file Send
 * the file), an address when only addresses may open it, a password when a
 * password is asked for and the Send has none yet.
 */
export function sendFormReady(form: SendForm, send: Send | null, kind: SendKind): boolean {
  if (!form.name.trim()) return false;
  if (kind === 0 && !send?.entry && !form.text.trim()) return false;
  if (kind === 1 && !send && !form.fileName) return false;
  if (form.access === 0 && splitAddresses(form.emails).length === 0) return false;
  if (form.access === 1 && !form.password && !send?.hasPassword) return false;
  return true;
}

/** The draft Rust seals. Expiring never comes after deleting. */
export function sendDraft(
  form: SendForm,
  send: Send | null,
  kind: SendKind,
  now = Date.now(),
): SendDraft {
  const max = Number(form.maxAccess);
  return {
    kind,
    name: form.name.trim(),
    notes: form.notes.trim() || null,
    text: kind === 0 ? form.text : null,
    hidden: form.hidden,
    fileName: form.fileName ?? send?.fileName ?? null,
    password: form.access === 1 ? form.password || null : null,
    authType: form.access,
    emails: form.access === 0 ? splitAddresses(form.emails) : [],
    maxAccessCount: form.maxAccess.trim() && max > 0 ? Math.max(1, Math.floor(max)) : null,
    expirationDate: form.expiresDays
      ? inDays(Math.min(form.expiresDays, form.deletionDays), now)
      : null,
    deletionDate: inDays(form.deletionDays, now),
    disabled: form.disabled,
    hideEmail: form.hideEmail,
  };
}

/** Newest change first. */
export function sortSends(sends: Send[]): Send[] {
  return [...sends].sort((a, b) => (b.revisionDate ?? '').localeCompare(a.revisionDate ?? ''));
}
