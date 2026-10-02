/**
 * The password check (UwULock-Server's docs/uwu-api.md §15), as the page sees
 * it. Rust works it out (uwulock-core's `health`) and asks the server; the
 * page gets findings — never a password — and the cards of the review.
 */

import { invoke } from '@tauri-apps/api/core';
import { t } from './i18n';
import type { IgnoredEntry, ProblemKind } from './review';

export type { IgnoredEntry, ProblemKind } from './review';

export type BreachSwitches = {
  hibp: boolean;
  xonPasswords: boolean;
  siteBreaches: boolean;
  emailCheck: boolean;
  changePassword: boolean;
};

export type Finding = {
  id: string;
  name: string;
  subtitle: string | null;
  bits: number;
  weak: boolean;
  reused: number;
  unsecured: boolean;
  /** Times seen in a breach; `null` when that wasn't checked. */
  breached: number | null;
  breachSources: string[];
  host: string | null;
  uri: string | null;
  passwordChanged: string | null;
};

export type Report = {
  findings: Finding[];
  checked: number;
  breachesChecked: boolean;
  breachesIncomplete: boolean;
};

export type SiteBreach = {
  domain: string;
  title: string;
  date: string | null;
  added: string | null;
  records: number;
  passwords: boolean;
  dataClasses: string[];
  sources: Record<string, string>;
};

export type Source = {
  id?: string | null;
  name: string;
  url?: string | null;
  license?: string | null;
};

export type TwofaEntry = {
  domain: string;
  name: string;
  methods: string[];
  documentation: string | null;
};

export type MissingTwofa = { itemId: string; name: string; host: string; entry: TwofaEntry };

export type Problem =
  | { kind: 'breached'; count: number; sources: string[] }
  | { kind: 'siteBreach'; breach: SiteBreach }
  | { kind: 'reused'; others: number }
  | { kind: 'weak'; bits: number }
  | { kind: 'unsecured' }
  | { kind: 'twofa'; documentation: string | null };

export type Card = { finding: Finding; problems: Problem[] };

export type EmailOptIn = { optedIn: boolean; since: string | null };

export type HealthView = {
  /** A UwULock Server: there may be breach sources and the shared ignore list. */
  uwu: boolean;
  switches: BreachSwitches;
  report: Report;
  checkedAt: string | null;
  siteBreaches: Record<string, SiteBreach>;
  siteSources: Source[];
  sitesFailed: boolean;
  twofa: MissingTwofa[];
  twofaSource: Source | null;
  twofaFailed: boolean;
  /** `null` where the server keeps no ignore list. */
  ignored: IgnoredEntry[] | null;
  cards: Card[];
  emailOptIn: EmailOptIn | null;
};

export type EmailResult = {
  email: string;
  status: 'found' | 'clean' | 'later' | 'failed';
  breaches: string[];
};

/** `fresh`: ask the breach sources again; else the last answers are reused. */
export const healthReport = (fresh: boolean) => invoke<HealthView>('health_report', { fresh });

export const setIgnored = (itemId: string, kind: ProblemKind, ignored: boolean) =>
  invoke<IgnoredEntry[]>('health_ignore', { itemId, kind, ignored });

/** Opens the change-password page (or the login) in the system browser. */
export const openChangePage = (itemId: string) => invoke<void>('health_open_page', { itemId });

/** Saves `password` to the login; the old one goes into its history. */
export const saveNewPassword = (itemId: string, password: string) =>
  invoke<void>('health_save_password', { itemId, password });

/** `null` when the server doesn't offer the check of addresses. */
export const emailOptIn = () => invoke<EmailOptIn | null>('health_email_opt_in');

export const setEmailOptIn = (optedIn: boolean) =>
  invoke<EmailOptIn>('set_health_email_opt_in', { optedIn });

export const checkEmails = () =>
  invoke<{ results: EmailResult[]; retryAfter: number | null }>('health_check_emails');

// ── Words ──────────────────────────────────────────────────

export function problemTitle(kind: ProblemKind): string {
  switch (kind) {
    case 'breached':
      return t('Passwort in Datenlecks');
    case 'siteBreach':
      return t('Datenleck nach deiner letzten Passwortänderung');
    case 'reused':
      return t('Mehrfach benutzt');
    case 'weak':
      return t('Schwach');
    case 'unsecured':
      return t('Ohne https');
    case 'twofa':
      return t('2FA möglich, nicht eingerichtet');
  }
}

const SOURCE_NAMES: Record<string, string> = { hibp: 'Have I Been Pwned', xon: 'XposedOrNot' };

export const sourceNames = (sources: string[]) =>
  sources.map((source) => SOURCE_NAMES[source] ?? source).join(', ');

/** How far one breach source is; `waiting`: questions waiting for a busy server. */
export type SourceProgress = { done: number; total: number; waiting: number };

/** The `health-progress` event: in all, and per source for breached passwords. */
export type CheckProgress = {
  done: number;
  total: number;
  hibp?: SourceProgress | null;
  xon?: SourceProgress | null;
};

/**
 * What the check says while it asks: per source when the event has them
 * ("Have I Been Pwned 12 von 40 · XposedOrNot 3 von 40, wartet auf den Server"),
 * else in all (the addresses).
 */
export function progressText(progress: CheckProgress): string {
  const sources = (['hibp', 'xon'] as const).flatMap((key) => {
    const source = progress[key];
    if (!source) return [];
    const count = t('{name} {done} von {total}', {
      name: SOURCE_NAMES[key] ?? key,
      done: source.done,
      total: source.total,
    });
    return [source.waiting > 0 ? t('{count}, wartet auf den Server', { count }) : count];
  });
  if (!sources.length)
    return t('Fragt nach Datenlecks … {done} von {total}', {
      done: progress.done,
      total: progress.total,
    });
  return t('Fragt nach Datenlecks … {sources}', { sources: sources.join(' · ') });
}

export function breachText(breach: SiteBreach): string {
  return t('{site}, {date} – {sources}', {
    site: breach.title,
    date: breach.date ? new Date(`${breach.date}T00:00:00Z`).toLocaleDateString() : '?',
    sources: sourceNames(Object.keys(breach.sources)),
  });
}

export function problemDetail(problem: Problem): string {
  switch (problem.kind) {
    case 'breached':
      return [
        t('{n} Mal gesehen', { n: problem.count.toLocaleString() }),
        problem.sources.length ? sourceNames(problem.sources) : null,
      ]
        .filter(Boolean)
        .join(' · ');
    case 'siteBreach':
      return breachText(problem.breach);
    case 'reused':
      return t('noch {n} Mal im Tresor', { n: problem.others });
    case 'weak':
      return t('{bits} Bit', { bits: problem.bits });
    case 'unsecured':
      return t('Die Adresse beginnt mit http://');
    case 'twofa':
      return t('Die Website bietet Einmal-Codes aus einer Authenticator-App an.');
  }
}
