/**
 * How the window looks, from the settings: theme, contrast and motion through
 * @uwusuite/design (useAppearance puts them on <html>; /boot.js does it before
 * the first paint), the font through applyUiFont, the text sizes through
 * useTypeScale (the platform's scale and the system's text size, times
 * Darstellung → Textgröße), the language on <html lang>.
 *
 * Phones draw their status and navigation bars around the page, so Rust is
 * told whether the page is dark and colours the bars to match.
 */

import {
  applyType,
  applyUiFont,
  currentTypePlatform,
  readSystemType,
  resolveType,
  useAppearance,
  useTypeScale,
} from '@uwusuite/design';
import { useEffect } from 'react';
import { setAppearance } from './api';
import { language, t } from './i18n';
import { BUILD_PLATFORM, isMobile, platform } from './platform';
import { getSettings, type Settings, type TextSizeChoice } from './settings';

/** Which text scale applies: macOS, iOS (also the iOS build on a Mac), Android or desktop. */
export const typePlatform = () => currentTypePlatform(BUILD_PLATFORM);

/** Before the first render: platform, font, text sizes and language (the theme is /boot.js's). */
export function prepareDocument() {
  const root = document.documentElement;
  root.dataset.platform = isMobile() ? platform() : 'desktop';
  const settings = getSettings();
  applyUiFont(settings.font);
  const type = typePlatform();
  applyType(
    resolveType({
      platform: type,
      font: settings.font,
      textSize: settings.textSize,
      ...readSystemType(type),
    }),
  );
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
  useTypeScale({ platform: typePlatform(), font: settings.font, textSize: settings.textSize });

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

/** Darstellung → Textgröße: on top of the system's text size, which "System" follows alone. */
export function textSizeOptions(): { value: TextSizeChoice; label: string }[] {
  return [
    { value: 'smaller', label: t('Kleiner') },
    { value: 'system', label: t('System') },
    { value: 'larger', label: t('Größer') },
    { value: 'largest', label: t('Sehr groß') },
  ];
}
