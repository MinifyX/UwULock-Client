/**
 * The extension in German or English, the way the desktop app does it.
 *
 * German is the source language: every string is written in German where it is used and
 * wrapped in `t()`, and the English catalogue in `src/i18n/en/` maps each German string to its
 * English one. `pnpm lint` (scripts/check-i18n.mjs) lists every string without an English
 * entry. Placeholders are `{name}`, filled from `vars`; strings kept in constants are marked
 * with `N_()` and translated with `t()` where they are shown.
 *
 * The popup and the prompt window follow the setting; content scripts get the language with
 * the page's info from the background.
 */

import { EN } from '../i18n/en';

export type Language = 'de' | 'en';
export type Vars = Record<string, string | number>;

let current: Language = systemLanguage();

/** "System" is German when the browser prefers German, English otherwise. */
export function systemLanguage(): Language {
  const preferred =
    (typeof navigator !== 'undefined' && (navigator.languages?.[0] ?? navigator.language)) || '';
  return preferred.toLowerCase().startsWith('de') ? 'de' : 'en';
}

export function resolveLanguage(setting: 'system' | Language): Language {
  return setting === 'system' ? systemLanguage() : setting;
}

export function setLanguage(language: Language) {
  current = language;
}

export function language(): Language {
  return current;
}

function fill(template: string, vars?: Vars): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in vars ? String(vars[name]) : whole,
  );
}

export function translate(lang: Language, text: string, vars?: Vars): string {
  return fill(lang === 'en' ? (EN[text] ?? text) : text, vars);
}

/** `text` (German) in the current language, placeholders filled. */
export function t(text: string, vars?: Vars): string {
  return translate(current, text, vars);
}

/** Marks a German string kept in a constant for translation where it is shown. */
export function N_(text: string): string {
  return text;
}

/** For dates and numbers. */
export function locale(lang: Language = current): string {
  return lang === 'de' ? 'de-DE' : 'en-GB';
}
