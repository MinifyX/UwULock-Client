// The generator's minimums (src/lib/generator.ts) and the font list
// (src/lib/fonts.ts), run by Node itself: node --test apps/desktop/test/

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { DEFAULT_FONT, FONT_CHOICES, FONT_STACKS, isFontChoice } from '../src/lib/fonts.ts';
import {
  cleanMinimums,
  effectiveLength,
  MAX_LENGTH,
  minimumOf,
  required,
  withMinimum,
} from '../src/lib/generator.ts';

const all = {
  length: 20,
  lowercase: true,
  uppercase: true,
  digits: true,
  symbols: true,
};

test('each set that is on needs at least one', () => {
  assert.equal(required(all), 4);
  assert.equal(required({ ...all, digits: false, symbols: false }), 2);
  // None on: lower case alone, as uwulock-core does.
  assert.equal(
    required({ ...all, lowercase: false, uppercase: false, digits: false, symbols: false }),
    1,
  );
  assert.equal(minimumOf({ ...all, minNumber: 0 }, 'digits'), 1);
});

test('minimums of sets that are off do not count', () => {
  assert.equal(required({ ...all, digits: false, minNumber: 50 }), 3);
});

test('the length is raised to what the minimums need', () => {
  const options = { ...all, length: 8, minNumber: 6, minSpecial: 6 };
  assert.equal(required(options), 14);
  assert.equal(effectiveLength(options), 14);
  assert.equal(effectiveLength({ ...all, length: 30 }), 30);
});

test('a minimum is kept so all of them fit into 128 characters', () => {
  const options = { ...all, minLowercase: 10, minUppercase: 10, minNumber: 10 };
  const next = withMinimum(options, 'symbols', 500);
  assert.equal(next.minSpecial, MAX_LENGTH - 30);
  assert.equal(required(next), MAX_LENGTH);
  assert.equal(withMinimum(options, 'symbols', -3).minSpecial, 1);
  assert.equal(withMinimum(options, 'symbols', Number.NaN).minSpecial, 1);
  assert.equal(withMinimum(options, 'digits', 4.7).minNumber, 4);
});

test('stored minimums are checked', () => {
  assert.deepEqual(
    cleanMinimums({ minLowercase: 3, minUppercase: '5', minNumber: -1, minSpecial: 999 }),
    { minLowercase: 3, minNumber: 0, minSpecial: MAX_LENGTH },
  );
});

test('fonts: UwU Sans first, every choice with a stack', () => {
  assert.equal(DEFAULT_FONT, 'uwu');
  assert.equal(FONT_CHOICES[0], 'uwu');
  for (const choice of FONT_CHOICES) assert.ok(FONT_STACKS[choice].includes('sans-serif'));
  assert.ok(isFontChoice('rubik'));
  assert.ok(!isFontChoice('comic-sans'));
});
