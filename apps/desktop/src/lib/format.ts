import { locale, t } from './i18n';

/** "gerade eben", "vor 5 Minuten", "vor 3 Stunden", or the date. */
export function ago(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return t('noch nie');
  const seconds = Math.max(0, Date.now() / 1000 - unixSeconds);
  if (seconds < 60) return t('gerade eben');
  if (seconds < 3600) return t('vor {n} Min.', { n: Math.round(seconds / 60) });
  if (seconds < 86400) return t('vor {n} Std.', { n: Math.round(seconds / 3600) });
  return new Date(unixSeconds * 1000).toLocaleDateString(locale());
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

/** The toast after copying: what was copied and when it leaves the clipboard. */
export function copiedText(field: string, seconds: number): string {
  const what =
    field === 'username'
      ? t('Benutzername kopiert')
      : field === 'password'
        ? t('Passwort kopiert')
        : field === 'totp'
          ? t('Code kopiert')
          : t('Kopiert');
  return seconds > 0 ? t('{what} ✧ – wird nach {n} s geleert', { what, n: seconds }) : `${what} ✧`;
}
