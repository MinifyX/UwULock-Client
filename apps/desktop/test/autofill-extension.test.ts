// The AutoFill extension itself is Swift and lives with the native app (apps/apple/CredentialProvider,
// tested there). What is left here is the page's side of its protocol — node --test.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

test('the protocol is copied as plain text, not as an expiring local-only secret', () => {
  const page = readFileSync(new URL('../src/components/AutofillLog.tsx', import.meta.url), 'utf8');
  assert.match(page, /copyText\(/);
  assert.doesNotMatch(page, /copyGenerated|copySecret/);
});
