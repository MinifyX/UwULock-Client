/**
 * The interface font (Settings → Appearance → Font), as in UwUMail: UwU Sans
 * by default, Manrope (UwULock's font until 0.5), Rubik, DM Sans or the
 * system's. Kept on this device only — it is a setting of the app, not of the
 * account. The extension uses the same list (its own setting, per browser).
 *
 * The font reaches every style through `--uwu-font` on <html>; the web fonts
 * are declared in styles/fonts.css (UwU Sans) and by @fontsource (the others),
 * and the browser only fetches the one in use.
 */

export const FONT_CHOICES = ['uwu', 'manrope', 'rubik', 'dmsans', 'system'] as const;
export type FontChoice = (typeof FONT_CHOICES)[number];

const SYSTEM_STACK =
  'system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", "Noto Sans", Arial, sans-serif';

/** The family list for each choice; web fonts fall back to the system's. */
export const FONT_STACKS: Record<FontChoice, string> = {
  uwu: `"UwU Sans", ${SYSTEM_STACK}`,
  manrope: `"Manrope Variable", "Manrope", ${SYSTEM_STACK}`,
  rubik: `"Rubik Variable", ${SYSTEM_STACK}`,
  dmsans: `"DM Sans Variable", ${SYSTEM_STACK}`,
  system: SYSTEM_STACK,
};

/** The names the picker shows (font names stay untranslated; "system" is translated). */
export const FONT_NAMES: Record<Exclude<FontChoice, 'system'>, string> = {
  uwu: 'UwU Sans',
  manrope: 'Manrope',
  rubik: 'Rubik',
  dmsans: 'DM Sans',
};

/**
 * A little tighter than the fonts are set, for interface text: UwU Sans
 * (Atkinson Hyperlegible) is spaced generously for reading.
 */
export const FONT_TRACKING: Record<FontChoice, string> = {
  uwu: '-0.008em',
  manrope: '0em',
  rubik: '0em',
  dmsans: '-0.004em',
  system: '0em',
};

export const DEFAULT_FONT: FontChoice = 'uwu';

export function isFontChoice(value: unknown): value is FontChoice {
  return (FONT_CHOICES as readonly unknown[]).includes(value);
}

/** Puts the chosen font on the whole interface at once. */
export function applyFont(choice: FontChoice, root: HTMLElement = document.documentElement) {
  root.style.setProperty('--uwu-font', FONT_STACKS[choice]);
  root.style.setProperty('--uwu-tracking', FONT_TRACKING[choice]);
  root.dataset.font = choice;
}
