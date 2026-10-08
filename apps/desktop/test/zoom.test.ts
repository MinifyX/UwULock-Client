// The window never zooms (src/lib/zoom.ts, index.html) — node --test.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { isZoomKey, isZoomWheel } from '../src/lib/zoom.ts';

const key = (key: string, mods: { ctrl?: boolean; meta?: boolean; alt?: boolean } = {}) => ({
  key,
  ctrlKey: mods.ctrl ?? false,
  metaKey: mods.meta ?? false,
  altKey: mods.alt ?? false,
});

test("a browser's zoom keys are caught, the app's own shortcuts are not", () => {
  for (const k of ['+', '-', '=', '0', 'Add', 'Subtract']) {
    assert.ok(isZoomKey(key(k, { ctrl: true })), `Ctrl+${k}`);
    assert.ok(isZoomKey(key(k, { meta: true })), `⌘${k}`);
  }
  for (const k of ['l', 'f', 'g', ',', 'n']) assert.ok(!isZoomKey(key(k, { ctrl: true })), k);
  assert.ok(!isZoomKey(key('+')), 'typing a plus');
  assert.ok(!isZoomKey(key('0', { ctrl: true, alt: true })), 'AltGr');
});

test('a pinch on a trackpad (Ctrl + wheel) is caught, scrolling is not', () => {
  assert.ok(isZoomWheel({ ctrlKey: true }));
  assert.ok(!isZoomWheel({ ctrlKey: false }));
});

test('the viewport forbids zooming and keeps the safe areas', () => {
  for (const [file, cover] of [
    ['../index.html', true],
    ['../../setup/index.html', false],
  ] as const) {
    const html = readFileSync(new URL(file, import.meta.url), 'utf8');
    const viewport = html.match(/<meta\s+name="viewport"\s+content="([^"]+)"/)?.[1] ?? '';
    assert.match(viewport, /maximum-scale=1(\.0)?\b/, file);
    assert.match(viewport, /user-scalable=no/, file);
    if (cover) assert.match(viewport, /viewport-fit=cover/, file);
  }
  const conf = JSON.parse(
    readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'),
  ) as { app: { windows: { zoomHotkeysEnabled?: boolean }[] } };
  assert.equal(conf.app.windows[0]?.zoomHotkeysEnabled, false);
});
