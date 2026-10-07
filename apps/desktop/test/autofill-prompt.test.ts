// When the "Als Standard festlegen" card shows (src/lib/autofillPrompt.ts) — node --test.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  FRESH,
  hasCredentialManager,
  MAX_LATER,
  missing,
  promptLater,
  promptSeen,
  readPrompt,
  shouldPrompt,
  type ProviderView,
} from '../src/lib/autofillPrompt.ts';

const NOW = Date.parse('2026-10-07T12:00:00Z');
const DAY = 86_400_000;

function view(patch: Partial<ProviderView> = {}): ProviderView {
  return {
    platform: 'ios',
    supported: true,
    enabled: false,
    direct: true,
    autofill: null,
    list: false,
    ...patch,
  };
}

test('shows after the first unlock while UwULock is not the provider', () => {
  assert.equal(shouldPrompt(FRESH, view(), NOW), true);
  assert.equal(shouldPrompt(FRESH, view({ enabled: null }), NOW), true, 'unknown counts as off');
  assert.equal(shouldPrompt(FRESH, view({ enabled: true, list: true }), NOW), false);
  // Switched on in iOS, but UwULock's own list is off: nothing to fill yet.
  assert.equal(missing(view({ enabled: true, list: false })), 'credentials');
});

test('never on systems without a provider, nor unsigned builds', () => {
  assert.equal(shouldPrompt(FRESH, view({ platform: 'none' }), NOW), false);
  assert.equal(shouldPrompt(FRESH, view({ supported: false }), NOW), false);
  assert.equal(shouldPrompt(FRESH, null, NOW), false);
});

test('Android asks for the autofill service after Credential Manager', () => {
  const android = view({ platform: 'android', list: false });
  assert.equal(missing(android), 'credentials');
  assert.equal(missing({ ...android, enabled: true, autofill: false }), 'autofill');
  assert.equal(missing({ ...android, enabled: true, autofill: true }), null);
  assert.equal(missing({ ...android, enabled: true, autofill: null }), null);
});

test('"Später" waits a week, at most three times', () => {
  let state = promptLater(FRESH, NOW);
  assert.equal(shouldPrompt(state, view(), NOW + DAY), false);
  assert.equal(shouldPrompt(state, view(), NOW + 7 * DAY), true);
  for (let i = 1; i < MAX_LATER; i++) state = promptLater(state, NOW + 7 * i * DAY);
  assert.equal(state.later, MAX_LATER);
  assert.equal(shouldPrompt(state, view(), NOW + 365 * DAY), false);
});

test('once on, never again', () => {
  const on = promptSeen(FRESH, view({ enabled: true, list: true }));
  assert.equal(on.done, true);
  assert.equal(shouldPrompt(on, view(), NOW), false, 'even when switched off later');
  assert.equal(promptSeen(FRESH, view()).done, false);
  assert.equal(promptSeen(on, view({ enabled: true, list: true })), on, 'unchanged once done');
});

test('reads what the storage holds, whatever it is', () => {
  assert.deepEqual(readPrompt(null), FRESH);
  assert.deepEqual(readPrompt('not json'), FRESH);
  assert.deepEqual(readPrompt('{"later":-1,"until":"x","done":"yes"}'), FRESH);
  assert.deepEqual(readPrompt('{"later":2,"until":5,"done":true}'), {
    later: 2,
    until: 5,
    done: true,
  });
});

test('Android before 14: only the autofill service is asked for', () => {
  const old = view({ platform: 'android', enabled: null, autofill: false });
  assert.equal(missing(old), 'autofill');
  assert.equal(missing({ ...old, autofill: true }), null);
  assert.equal(hasCredentialManager(old), false);
  // Android 14+: Credential Manager first, then the autofill service.
  const current = view({ platform: 'android', enabled: false, autofill: false });
  assert.equal(hasCredentialManager(current), true);
  assert.equal(missing(current), 'credentials');
  assert.equal(missing({ ...current, enabled: true }), 'autofill');
});
