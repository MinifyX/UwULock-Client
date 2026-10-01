// The review's card logic (src/lib/review.ts), run by Node itself:
//
//   node --test apps/desktop/test/
//
// Node strips the types; no bundler, no test framework to install.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  aboutThePassword,
  clampIndex,
  dragStarts,
  isIgnored,
  keyStep,
  parseLater,
  progress,
  skipCard,
  step,
  swipeStep,
  SWIPE,
  withoutLater,
} from '../src/lib/review.ts';

const cards = ['a', 'b', 'c', 'd'].map((id) => ({ finding: { id } }));

test('moving through the stack stops at both ends', () => {
  assert.equal(step(0, 1, 4), 1);
  assert.equal(step(3, 1, 4), 3);
  assert.equal(step(0, -1, 4), 0);
  assert.equal(step(2, -5, 4), 0);
  assert.equal(clampIndex(7, 0), 0, 'an empty stack has index 0');
});

test('arrow keys page, other keys do nothing', () => {
  assert.equal(keyStep('ArrowRight'), 1);
  assert.equal(keyStep('ArrowLeft'), -1);
  assert.equal(keyStep('ArrowUp'), 0);
  assert.equal(keyStep('Enter'), 0);
});

test('a drag is sideways and long enough; left is the next card', () => {
  assert.equal(dragStarts(5, 0), false, 'a tap that wobbles');
  assert.equal(dragStarts(30, 40), false, 'scrolling the page');
  assert.equal(dragStarts(-30, 10), true);
  assert.equal(swipeStep(-SWIPE), 1);
  assert.equal(swipeStep(-SWIPE + 1), 0);
  assert.equal(swipeStep(SWIPE + 40), -1);
  assert.equal(swipeStep(0), 0);
});

test('later takes the card out and shows the next one', () => {
  const middle = skipCard(cards, 1);
  assert.deepEqual(
    middle.cards.map((c) => c.finding.id),
    ['a', 'c', 'd'],
  );
  assert.equal(middle.index, 1, 'c moves into b’s place');
  assert.equal(middle.skipped, 'b');
  const last = skipCard(cards, 3);
  assert.equal(last.index, 2, 'at the end the one before is shown');
  const none = skipCard([], 0);
  assert.equal(none.skipped, null);
  assert.deepEqual(
    withoutLater(cards, new Set(['a', 'd'])).map((c) => c.finding.id),
    ['b', 'c'],
  );
});

test('progress counts from one', () => {
  assert.deepEqual(progress(2, 12), { n: 3, total: 12 });
  assert.deepEqual(progress(12, 12), { n: 12, total: 12 });
  assert.deepEqual(progress(0, 0), { n: 0, total: 0 });
});

test('ignored problems are per login and kind', () => {
  const list = [{ itemId: 'a', kind: 'weak' as const, since: '2026-10-01T00:00:00Z' }];
  assert.equal(isIgnored(list, 'a', 'weak'), true);
  assert.equal(isIgnored(list, 'a', 'reused'), false);
  assert.equal(isIgnored(list, 'b', 'weak'), false);
  assert.equal(isIgnored(null, 'a', 'weak'), false);
  assert.equal(aboutThePassword('weak'), true);
  assert.equal(aboutThePassword('twofa'), false);
  assert.equal(aboutThePassword('unsecured'), false);
});

test('the session’s later list survives garbage', () => {
  assert.deepEqual([...parseLater('["a","b",3]')], ['a', 'b']);
  assert.equal(parseLater('{').size, 0);
  assert.equal(parseLater(null).size, 0);
});
