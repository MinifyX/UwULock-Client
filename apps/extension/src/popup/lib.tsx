/**
 * What the popup and the prompt window share: the status and settings as React state, errors
 * in words, toasts, formatting, and the small controls the desktop app has too (a password
 * field with an eye, the one-time code's ring).
 */

import {
  applyAppearance,
  applyUiFont,
  createToasts,
  ICONS,
  IconButton,
  isFontChoice,
  QUERIES,
  resolveAppearance,
  Toaster,
  UwuLabels,
} from '@uwusuite/design';
import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';
import { NyuStage } from '@desktop/components/nyu/stage';
import { ext } from '../shared/browser';
import { RequestFailed } from '../shared/messages';
import { locale, resolveLanguage, setLanguage, t } from '../shared/i18n';
import type { Settings, Status, StatusMessage } from '../shared/protocol';
import { uwuErrorText } from '../shared/uwu-errors';
import { getSettings, touch, vaultStatus } from './api';

// ── Status and settings ───────────────────────────────────

/** The background's status, again whenever it says something changed. */
export function useStatus(): [Status | null, () => Promise<void>] {
  const [status, setStatus] = useState<Status | null>(null);
  const refresh = useCallback(async () => {
    try {
      setStatus(await vaultStatus());
    } catch {
      // The background is starting; the next change asks again.
    }
  }, []);
  useEffect(() => {
    void refresh();
    const listener = (message: unknown) => {
      if ((message as StatusMessage | null)?.type === 'bg:status-changed') void refresh();
    };
    ext.runtime.onMessage.addListener(listener);
    // While the popup is open it counts as use, and keeps the background awake.
    const timer = window.setInterval(() => void touch().catch(() => undefined), 20_000);
    return () => {
      ext.runtime.onMessage.removeListener(listener);
      window.clearInterval(timer);
    };
  }, [refresh]);
  return [status, refresh];
}

/**
 * Whether the account's server is a UwULock Server that offers `feature` (as its
 * `/uwu/v1/info` lists it: `masked-addresses`, `icons`, `file-requests`, …). UwULock's own
 * extras show only then; for Vaultwarden and Bitwarden this is always false.
 */
export function uwuFeature(status: Status | null, feature: string): boolean {
  return Boolean(status?.uwu?.features.includes(feature));
}

let settingsCache: Settings | null = null;
const settingsListeners = new Set<() => void>();

/**
 * Language, theme, contrast, motion and font onto <html>, through @uwusuite/design: data-theme,
 * data-contrast, data-motion (applyAppearance) and the font (applyUiFont). A font stored before
 * the package's list counts as UwU Sans.
 */
function applyLook(settings: Settings) {
  setLanguage(resolveLanguage(settings.language));
  document.documentElement.lang = resolveLanguage(settings.language);
  applyAppearance(
    resolveAppearance({
      theme: settings.theme,
      contrast: settings.contrast ?? 'system',
      motion: settings.motion ?? 'system',
    }),
  );
  applyUiFont(isFontChoice(settings.font) ? settings.font : 'uwu');
}

/** "System" follows the system while the popup is open, too. */
let watching = false;
function watchSystem() {
  if (watching || typeof window.matchMedia !== 'function') return;
  watching = true;
  for (const query of Object.values(QUERIES)) {
    window.matchMedia(query).addEventListener('change', () => {
      if (settingsCache) applyLook(settingsCache);
    });
  }
}

export function publishSettings(next: Settings) {
  settingsCache = next;
  applyLook(next);
  watchSystem();
  for (const listener of settingsListeners) listener();
}

export async function loadSettings(): Promise<Settings> {
  const loaded = await getSettings();
  publishSettings(loaded);
  return loaded;
}

/** The settings; the component draws again when they (and so the language) change. */
export function useSettings(): Settings | null {
  return useSyncExternalStore(
    (listener) => {
      settingsListeners.add(listener);
      return () => settingsListeners.delete(listener);
    },
    () => settingsCache,
  );
}

// ── Errors in words ───────────────────────────────────────

export function errorText(error: unknown): string {
  const kind = error instanceof RequestFailed ? error.kind : 'unknown';
  const m = error instanceof Error ? error.message : String(error);
  const uwu = uwuErrorText(kind, m);
  if (uwu) return uwu;
  switch (kind) {
    case 'network':
      return t('Der Server ist nicht erreichbar.');
    case 'permission':
      return t(
        'UwULock darf diesen Server noch nicht erreichen. Erlaube es, wenn der Browser fragt.',
      );
    case 'refused':
      if (/wrong|incorrect|invalid_grant/i.test(m))
        return t('E-Mail-Adresse oder Master-Passwort stimmt nicht.');
      return t('Der Server hat die Anmeldung abgelehnt: {reason}', { reason: m });
    case 'weaker-kdf':
      return t(
        'Der Server verlangt für dieses Konto eine schwächere Schlüsselableitung als bei der letzten Anmeldung, deshalb hat UwULock nichts gesendet. Wenn du sie selbst gesenkt hast, melde das Konto hier ab (oder vergiss die gespeicherte Einstellung unten) und melde dich neu an.',
      );
    case 'wrong-password':
      return t('Das Master-Passwort ist falsch.');
    case 'account-changed':
      return t('Inzwischen ist ein anderes Konto geöffnet. Es wurde nichts gespeichert.');
    case 'pin-too-short':
      return t('Die PIN ist zu kurz.');
    case 'pin-cleared':
      return t(
        'Zu viele falsche PINs. Die PIN ist gelöscht, entsperre mit deinem Master-Passwort.',
      );
    case 'session-expired':
      return t('Die Sitzung ist abgelaufen. Bitte melde dich neu an.');
    case 'expired':
      return t('Das ist abgelaufen. Bitte versuche es noch einmal.');
    case 'conflict':
      return t(
        'Dieser Eintrag wurde woanders geändert. UwULock hat nichts überschrieben – bearbeite ihn noch einmal.',
      );
    case 'reprompt':
      return t('Dieser Eintrag fragt zuerst nach deinem Master-Passwort.');
    case 'verify':
      return t('Bitte gib dein Master-Passwort ein.');
    case 'server':
      return t('Der Server hat mit einem Fehler geantwortet: {reason}', { reason: m });
    case 'unsupported':
      return t('Das kann UwULock noch nicht: {reason}', { reason: m });
    case 'crypto':
      return t('Etwas ließ sich nicht entschlüsseln: {reason}', { reason: m });
    case 'locked':
      return t('Der Tresor ist gesperrt.');
    case 'not-found':
      return t('Das gibt es in diesem Eintrag nicht (mehr).');
    case 'unavailable':
      return t('UwULock antwortet gerade nicht. Schließe das Fenster und öffne es neu.');
    case 'invalid':
      if (m.includes('not an email')) return t('Das sieht nicht nach einer E-Mail-Adresse aus.');
      if (m.includes('PIN')) return t('Die PIN braucht mindestens vier Zeichen.');
      if (m.includes('out first')) return t('Melde dieses Konto zuerst ab.');
      return m;
    default:
      return m;
  }
}

/** A server address that can't be used, in words. */
export function serverUrlError(code: string): string {
  if (code === 'insecure')
    return t(
      'Der Server muss per https:// erreichbar sein. Unverschlüsseltes http:// geht nur zu diesem Rechner (localhost).',
    );
  if (code === 'empty') return t('Bitte gib die Adresse deines Servers ein.');
  return t('Das ist keine gültige Server-Adresse.');
}

// ── Toasts ────────────────────────────────────────────────

/** @uwusuite/design's toasts, one at a time and quicker: most answer a click just made. */
const toasts = createToasts({ infoMs: 2600, errorMs: 6000, max: 1 });

export function toast(text: string, tone: 'info' | 'error' = 'info') {
  toasts.show(text, { tone });
}

/**
 * A failure as a toast. An extra the server switched off meanwhile is nothing the person can fix:
 * a calm note, and the background has asked the server again, so it goes away.
 */
export function toastError(error: unknown) {
  const off = error instanceof RequestFailed && error.kind === 'uwu:feature_off';
  toast(errorText(error), off ? 'info' : 'error');
}

/** The toasts (above the tab bar when there is one) and Nyu's cameos. */
export function ToastView({ raised = false }: { raised?: boolean }) {
  useSettings();
  return (
    <>
      <div className={raised ? '[&>div]:bottom-16' : undefined}>
        <UwuLabels labels={locale().startsWith('en') ? 'en' : 'de'}>
          <Toaster store={toasts} />
        </UwuLabels>
      </div>
      <NyuStage />
    </>
  );
}

/**
 * A Segmented control across the whole width, its choices sharing it (on the button elements,
 * so they win over the package's own padding).
 */
export const WIDE_SEGMENTED =
  'w-full [&>button]:min-w-0 [&>button]:flex-auto [&>button]:truncate [&>button]:px-2';

// ── Formatting ────────────────────────────────────────────

/** "gerade eben", "vor 5 Min.", or the date. Milliseconds since 1970. */
export function ago(ms: number | null | undefined): string {
  if (!ms) return t('noch nie');
  const seconds = Math.max(0, (Date.now() - ms) / 1000);
  if (seconds < 60) return t('gerade eben');
  if (seconds < 3600) return t('vor {n} Min.', { n: Math.round(seconds / 60) });
  if (seconds < 86400) return t('vor {n} Std.', { n: Math.round(seconds / 3600) });
  return new Date(ms).toLocaleDateString(locale());
}

/** An ISO date from the server, as a local date and time. */
export function when(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return date.toLocaleString(locale(), { dateStyle: 'medium', timeStyle: 'short' });
}

/** A TOTP code in two halves, "123 456", as authenticator apps show it. */
export function spacedCode(code: string): string {
  if (code.length < 6 || !/^\d+$/.test(code)) return code;
  const half = Math.ceil(code.length / 2);
  return `${code.slice(0, half)} ${code.slice(half)}`;
}

/** Splits a password into runs of letters, digits and symbols, for colouring. */
export function charClasses(text: string): { kind: 'letter' | 'digit' | 'symbol'; text: string }[] {
  const runs: { kind: 'letter' | 'digit' | 'symbol'; text: string }[] = [];
  for (const char of text) {
    const kind = /\d/.test(char) ? 'digit' : /\p{L}/u.test(char) ? 'letter' : 'symbol';
    const last = runs[runs.length - 1];
    if (last && last.kind === kind) last.text += char;
    else runs.push({ kind, text: char });
  }
  return runs;
}

/** A password with digits pink and symbols violet, so `l1I|` can be told apart. */
export function Colored({ text }: { text: string }) {
  return (
    <span className="colored mono">
      {charClasses(text).map((run, index) => (
        <span key={index} data-class={run.kind}>
          {run.text}
        </span>
      ))}
    </span>
  );
}

/** The toast after copying: what was copied and when it leaves the clipboard. */
export function copiedText(field: string, seconds: number): string {
  const what =
    field === 'username'
      ? t('Benutzername kopiert')
      : field === 'password'
        ? t('Passwort kopiert')
        : field === 'totp' || field === 'totp-next'
          ? t('Code kopiert')
          : t('Kopiert');
  return seconds > 0 ? t('{what} ✧ – wird nach {n} s geleert', { what, n: seconds }) : `${what} ✧`;
}

// ── Controls ──────────────────────────────────────────────

type PasswordInputProps = {
  value: string;
  onChange: (value: string) => void;
  autoFocus?: boolean;
  disabled?: boolean;
  autoComplete?: string;
  id?: string;
  label?: string;
  placeholder?: string;
  inputMode?: 'text' | 'numeric';
};

/** A password field with an eye to peek, and a hint when Caps Lock is on. */
export function PasswordInput({
  value,
  onChange,
  autoFocus,
  disabled,
  autoComplete = 'current-password',
  id,
  label,
  placeholder,
  inputMode,
}: PasswordInputProps) {
  const [visible, setVisible] = useState(false);
  const [caps, setCaps] = useState(false);
  return (
    <span className="password-input">
      <input
        id={id}
        type={visible ? 'text' : 'password'}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => setCaps(e.getModifierState('CapsLock'))}
        onKeyUp={(e) => setCaps(e.getModifierState('CapsLock'))}
        onBlur={() => setCaps(false)}
        autoFocus={autoFocus}
        disabled={disabled}
        autoComplete={autoComplete}
        spellCheck={false}
        aria-label={label}
        placeholder={placeholder}
        inputMode={inputMode}
      />
      <IconButton
        icon={visible ? ICONS.hide : ICONS.show}
        label={visible ? t('Passwort verbergen') : t('Passwort zeigen')}
        size="sm"
        onClick={() => setVisible(!visible)}
        aria-pressed={visible}
        tabIndex={-1}
      />
      {caps && <small className="caps-hint">{t('Feststelltaste ist an')}</small>}
    </span>
  );
}

/** The one-time code's countdown: a ring that empties, amber for the last five seconds. */
export function TotpRing({ remaining, period }: { remaining: number; period: number }) {
  const radius = 9;
  const circumference = 2 * Math.PI * radius;
  const left = Math.max(0, Math.min(1, remaining / period));
  return (
    <svg className="totp-ring" width="24" height="24" viewBox="0 0 24 24" aria-hidden>
      <circle className="totp-track" cx="12" cy="12" r={radius} fill="none" strokeWidth="3" />
      <circle
        className="totp-left"
        cx="12"
        cy="12"
        r={radius}
        fill="none"
        strokeWidth="3"
        strokeDasharray={circumference}
        strokeDashoffset={circumference * (1 - left)}
        transform="rotate(-90 12 12)"
      />
    </svg>
  );
}
