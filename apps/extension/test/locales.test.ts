// @vitest-environment node
/** Store limits for _locales: App Store Connect rejects Safari extensions whose description is longer than 112 characters. */

import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const dir = join(__dirname, '..', 'public', '_locales');

describe('_locales', () => {
  for (const locale of readdirSync(dir)) {
    it(`${locale}: description fits Safari (≤ 112) and Chrome (≤ 132)`, () => {
      const messages = JSON.parse(readFileSync(join(dir, locale, 'messages.json'), 'utf8'));
      const description: string = messages.extDescription.message;
      expect([...description].length).toBeLessThanOrEqual(112);
      expect(messages.extName.message.length).toBeLessThanOrEqual(45);
    });
  }
});
