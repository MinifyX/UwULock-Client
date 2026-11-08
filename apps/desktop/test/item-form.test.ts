// The editor's form and the draft it sends (src/lib/itemForm.ts), without
// Tauri: node --test apps/desktop/test/

import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { ItemDetail, ItemKind, ItemSummary } from '../src/lib/api.ts';
import { draftOf, formOf } from '../src/lib/itemForm.ts';

function summary(patch: Partial<ItemSummary> = {}): ItemSummary {
  return {
    id: 'i1',
    kind: 'login',
    name: 'Mail',
    subtitle: null,
    host: null,
    favorite: false,
    folderId: null,
    organizationId: null,
    collectionIds: [],
    deleted: false,
    reprompt: false,
    viewPassword: true,
    hasTotp: false,
    hasPassword: false,
    hasUsername: false,
    broken: false,
    revisionDate: '2026-11-01T00:00:00.000Z',
    ...patch,
  };
}

const unchanged = (kind: ItemKind, detail: Omit<ItemDetail, 'summary' | 'locked'>) => {
  const s = summary({ kind });
  return draftOf(formOf(s, { summary: s, locked: false, ...detail }), kind);
};

test('an untouched login sends no secrets back and keeps its revision and fields', () => {
  const draft = unchanged('login', {
    login: {
      username: 'nyu',
      hasPassword: true,
      hasTotp: true,
      passwordRevisionDate: null,
      uris: [{ uri: 'https://mail.example.com', match: null, host: null, openable: true }],
      passkeys: 0,
    },
    fields: [
      { index: 0, name: 'pin', kind: 'hidden', value: null, hasValue: true },
      { index: 1, name: 'user', kind: 'linked', value: null, hasValue: false },
    ],
  });
  assert.equal(draft.login?.password, null);
  assert.equal(draft.login?.totp, null);
  assert.equal(draft.login?.username, 'nyu');
  assert.equal(draft.revisionDate, '2026-11-01T00:00:00.000Z');
  assert.deepEqual(
    draft.fields?.map((f) => [f.from, f.value]),
    [
      [0, null],
      [1, null],
    ],
  );
});

test('an untouched card sends neither its number nor its code', () => {
  const draft = unchanged('card', {
    card: {
      cardholderName: 'Nyu',
      brand: 'Visa',
      numberEnding: '1234',
      expMonth: '1',
      expYear: '2030',
      hasCode: true,
    },
  });
  assert.equal(draft.card?.number, null);
  assert.equal(draft.card?.code, null);
  assert.equal(draft.card?.cardholderName, 'Nyu');
});

test('an untouched identity leaves the sensitive fields it has out', () => {
  const draft = unchanged('identity', {
    identity: [
      { name: 'firstName', sensitive: false, value: 'Nyu' },
      { name: 'ssn', sensitive: true, value: null },
      { name: 'passportNumber', sensitive: true, value: null },
      { name: 'licenseNumber', sensitive: true, value: null },
    ],
  });
  assert.equal(draft.identity?.firstName, 'Nyu');
  for (const key of ['ssn', 'passportNumber', 'licenseNumber'])
    assert.equal(key in (draft.identity ?? {}), false, key);
});
