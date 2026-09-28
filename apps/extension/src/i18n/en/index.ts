/**
 * The English catalogue: German string → English string, one file per part of the
 * extension. See `shared/i18n.ts`.
 */

import content from './content.json';
import popup from './popup.json';
import prompt from './prompt.json';
import background from './background.json';

export const EN: Readonly<Record<string, string>> = {
  ...background,
  ...content,
  ...popup,
  ...prompt,
};
