// Sends without Tauri (src/lib/sendModel.ts): status, dates, the draft —
// run by Node itself: node --test apps/desktop/test/

import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  daysUntil,
  inDays,
  sendDraft,
  sendForm,
  sendFormReady,
  sendStatus,
  sortSends,
  splitAddresses,
  type Send,
} from '../src/lib/sendModel.ts';

const NOW = Date.parse('2026-10-06T12:00:00Z');
const DAY = 86_400_000;

function send(patch: Partial<Send> = {}): Send {
  return {
    id: 's1',
    kind: 0,
    name: 'Wi-Fi',
    notes: null,
    text: 'hunter2',
    hidden: false,
    fileName: null,
    size: null,
    maxAccessCount: null,
    accessCount: 0,
    hasPassword: false,
    authType: 2,
    emails: [],
    disabled: false,
    hideEmail: false,
    revisionDate: '2026-10-05T12:00:00Z',
    expirationDate: null,
    deletionDate: new Date(NOW + 7 * DAY).toISOString(),
    entry: false,
    link: 'https://lock.example.com/#/send/abc/key',
    sendDomainId: null,
    ...patch,
  };
}

test('a Send is switched off, expired, used up, or open until a date', () => {
  assert.equal(sendStatus(send({ disabled: true }), NOW).kind, 'disabled');
  assert.equal(
    sendStatus(send({ expirationDate: new Date(NOW - 1000).toISOString() }), NOW).kind,
    'expired',
  );
  assert.equal(sendStatus(send({ maxAccessCount: 2, accessCount: 2 }), NOW).kind, 'used-up');
  const open = sendStatus(send({ expirationDate: new Date(NOW + DAY).toISOString() }), NOW);
  assert.deepEqual(open, { kind: 'active', until: new Date(NOW + DAY).toISOString() });
  // Without an expiry it lasts until it is deleted.
  assert.deepEqual(sendStatus(send(), NOW), { kind: 'active', until: send().deletionDate });
});

test('dates come back as the nearest offered number of days', () => {
  assert.equal(daysUntil(null, NOW), 7);
  assert.equal(daysUntil(new Date(NOW + 13 * DAY).toISOString(), NOW), 14);
  assert.equal(daysUntil(new Date(NOW + 2 * DAY + 3_600_000).toISOString(), NOW), 2);
  assert.equal(daysUntil(new Date(NOW + 40 * DAY).toISOString(), NOW), 30);
  assert.equal(inDays(1, NOW), '2026-10-07T12:00:00.000Z');
});

test('addresses are split on commas, spaces and lines, in lower case', () => {
  assert.deepEqual(splitAddresses(' A@Example.com, b@example.org\nc@example.net;; '), [
    'a@example.com',
    'b@example.org',
    'c@example.net',
  ]);
});

test('the draft is the web vault’s: expiry never after deletion, a password only when asked for', () => {
  const form = {
    ...sendForm(null, NOW),
    name: ' Plan ',
    text: 'x',
    deletionDays: 3,
    expiresDays: 7,
  };
  const draft = sendDraft({ ...form, password: 'pw', maxAccess: '0' }, null, 0, NOW);
  assert.equal(draft.name, 'Plan');
  assert.equal(draft.deletionDate, inDays(3, NOW));
  assert.equal(draft.expirationDate, inDays(3, NOW));
  assert.equal(draft.password, null);
  assert.equal(draft.maxAccessCount, null);
  assert.equal(draft.authType, 2);
  const locked = sendDraft({ ...form, access: 1, password: 'pw', maxAccess: '4' }, null, 0, NOW);
  assert.equal(locked.password, 'pw');
  assert.equal(locked.maxAccessCount, 4);
  const mailed = sendDraft({ ...form, access: 0, emails: 'A@example.com' }, null, 0, NOW);
  assert.deepEqual(mailed.emails, ['a@example.com']);
  assert.equal(sendDraft({ ...form, expiresDays: 0 }, null, 0, NOW).expirationDate, null);
});

test('what a Send needs before it is saved', () => {
  const empty = sendForm(null, NOW);
  assert.equal(sendFormReady({ ...empty, name: 'x' }, null, 0), false);
  assert.equal(sendFormReady({ ...empty, name: 'x', text: 't' }, null, 0), true);
  // A file Send needs its file; a changed one keeps it.
  assert.equal(sendFormReady({ ...empty, name: 'x' }, null, 1), false);
  assert.equal(sendFormReady({ ...empty, name: 'x', fileName: 'a.pdf' }, null, 1), true);
  assert.equal(sendFormReady({ ...empty, name: 'x' }, send({ kind: 1 }), 1), true);
  // A password: a new one, or the one it has.
  const locked = { ...empty, name: 'x', text: 't', access: 1 as const };
  assert.equal(sendFormReady(locked, null, 0), false);
  assert.equal(sendFormReady(locked, send({ hasPassword: true }), 0), true);
  assert.equal(sendFormReady({ ...locked, access: 0, emails: ' , ' }, null, 0), false);
  // An entry Send's text isn't edited.
  assert.equal(sendFormReady({ ...empty, name: 'x' }, send({ entry: true }), 0), true);
});

test('a Send’s form starts from what it has', () => {
  const form = sendForm(
    send({ maxAccessCount: 3, authType: 0, emails: ['a@example.com', 'b@example.org'] }),
    NOW,
  );
  assert.equal(form.maxAccess, '3');
  assert.equal(form.access, 0);
  assert.equal(form.emails, 'a@example.com, b@example.org');
  assert.equal(form.deletionDays, 7);
  assert.equal(form.expiresDays, 0);
  assert.deepEqual(
    sortSends([send({ id: 'old', revisionDate: '2026-01-01' }), send({ id: 'new' })]).map(
      (s) => s.id,
    ),
    ['new', 'old'],
  );
});
