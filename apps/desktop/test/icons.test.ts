// The icon library's search without Tauri (src/lib/iconLibrary.ts), the
// web vault's, run by Node itself: node --test apps/desktop/test/

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { hostOf, localLabel, searchLibrary, suggestLibrary } from '../src/lib/iconLibrary.ts';
import type { IconLibrary } from '../src/lib/iconLibrary.ts';

const LIBRARY: IconLibrary = {
  updated: '2026-10-01T00:00:00Z',
  sources: [],
  icons: [
    { source: 'selfhst', id: 'nextcloud', name: 'Nextcloud', variants: ['default'], aliases: [] },
    {
      source: 'selfhst',
      id: 'nextcloud-talk',
      name: 'Nextcloud Talk',
      variants: ['default'],
      aliases: [],
    },
    {
      source: 'selfhst',
      id: 'jellyfin',
      name: 'Jellyfin',
      variants: ['default', 'light', 'dark'],
      aliases: [],
    },
    {
      source: 'dashboard-icons',
      id: 'home-assistant',
      name: 'Home Assistant',
      variants: ['default'],
      aliases: ['hass'],
    },
  ],
};

test('the library is searched as the web vault searches it', () => {
  assert.deepEqual(
    searchLibrary(LIBRARY, 'nextcloud').map((i) => i.id),
    ['nextcloud', 'nextcloud-talk'],
  );
  assert.deepEqual(
    searchLibrary(LIBRARY, 'HASS').map((i) => i.id),
    ['home-assistant'],
  );
  assert.deepEqual(searchLibrary(LIBRARY, '  '), []);
  assert.deepEqual(searchLibrary(LIBRARY, 'nextcloud', 1).length, 1);
});

test('a device in the home network gets suggestions by its name or the item’s', () => {
  assert.equal(localLabel('jellyfin.local'), 'jellyfin');
  assert.equal(localLabel('192.0.2.10'), null);
  assert.equal(localLabel('localhost'), null);
  assert.equal(localLabel('[fd00::1]'), null);
  assert.deepEqual(
    suggestLibrary(LIBRARY, 'jellyfin.local', 'Media').map((i) => i.id),
    ['jellyfin'],
  );
  assert.deepEqual(
    suggestLibrary(LIBRARY, 'home-assistant.lan', 'Haus').map((i) => i.id),
    ['home-assistant'],
  );
  assert.deepEqual(
    suggestLibrary(LIBRARY, '192.0.2.5', 'My Nextcloud').map((i) => i.id),
    ['nextcloud', 'nextcloud-talk'],
  );
});

test('the host of an address as somebody typed it', () => {
  assert.equal(hostOf('nas.local:5000/login'), 'nas.local');
  assert.equal(hostOf('https://Router.Lan/'), 'router.lan');
  assert.equal(hostOf('  '), null);
  assert.equal(hostOf('http://'), null);
});
