/**
 * The English catalogue: German string → English string, one file per area of
 * the app. See `lib/i18n.ts`.
 */

import app from './app.json';
import editing from './editing.json';
import extras from './extras.json';
import health from './health.json';
import mobile from './mobile.json';
import moving from './moving.json';
import settings from './settings.json';
import vault from './vault.json';
import wifi from './wifi.json';

export const EN: Readonly<Record<string, string>> = {
  ...app,
  ...editing,
  ...extras,
  ...health,
  ...mobile,
  ...moving,
  ...settings,
  ...vault,
  ...wifi,
};
