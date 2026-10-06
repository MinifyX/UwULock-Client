/**
 * How the window looks, from the settings: theme, contrast and motion through
 * @uwusuite/design (useAppearance puts them on <html>; /boot.js does it before
 * the first paint), the font through applyUiFont, the language on <html lang>.
 *
 * Phones draw their status and navigation bars around the page, so Rust is
 * told whether the page is dark and colours the bars to match.
 */

import { applyUiFont, useAppearance } from '@uwusuite/design';
import { useEffect } from 'react';
import { setAppearance } from './api';
import { language } from './i18n';
import { isMobile, platform } from './platform';
import { getSettings, type Settings } from './settings';

/** Before the first render: platform, font and language (the theme is /boot.js's). */
export function prepareDocument() {
  const root = document.documentElement;
  root.dataset.platform = isMobile() ? platform() : 'desktop';
  const settings = getSettings();
  applyUiFont(settings.font);
  root.lang = language(settings);
}

/** Keeps <html> in step with the settings and the system. */
export function useAppAppearance(settings: Settings) {
  const resolved = useAppearance({
    theme: settings.theme,
    contrast: settings.contrast,
    motion: settings.motion,
  });
  const lang = language(settings);

  useEffect(() => {
    applyUiFont(settings.font);
  }, [settings.font]);

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  const dark = resolved.theme === 'dark';
  useEffect(() => {
    if (isMobile()) void setAppearance(dark).catch(() => undefined);
  }, [dark]);

  return resolved;
}
