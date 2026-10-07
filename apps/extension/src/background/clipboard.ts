/**
 * Copying, and taking it off the clipboard again after the configured time (30 seconds by
 * default). A Chromium service worker has no clipboard: it writes through an offscreen
 * document made for that. Firefox's background is a page and writes itself. Safari's is a page
 * too, but one that may write only right after a click in it: there the popup copies by itself
 * (popup/api.ts) and only the clearing is left here — which Safari may refuse as well, so a copy
 * from Safari can stay on the clipboard (docs/extension.md). The clearing runs on an alarm, so
 * it happens even when the worker was ended in between.
 */

import { ext } from '../shared/browser';
import { settings } from './settings';

const ALARM = 'clear-clipboard';
const OFFSCREEN = 'offscreen.html';

type Offscreen = {
  createDocument(options: { url: string; reasons: string[]; justification: string }): Promise<void>;
  hasDocument?: () => Promise<boolean>;
};

function offscreenApi(): Offscreen | undefined {
  return (ext as unknown as { offscreen?: Offscreen }).offscreen;
}

let creating: Promise<void> | null = null;

async function offscreenReady(api: Offscreen) {
  const contexts = await (
    ext.runtime as unknown as {
      getContexts?: (filter: { contextTypes: string[] }) => Promise<unknown[]>;
    }
  )
    .getContexts?.({ contextTypes: ['OFFSCREEN_DOCUMENT'] })
    .catch(() => []);
  if (contexts && contexts.length > 0) return;
  creating ??= api
    .createDocument({
      url: OFFSCREEN,
      reasons: ['CLIPBOARD'],
      justification: 'Copy a password, and clear it from the clipboard again.',
    })
    .catch((error: unknown) => {
      // Created meanwhile by another call: fine.
      if (!String(error).includes('Only a single offscreen')) throw error;
    })
    .finally(() => {
      creating = null;
    });
  await creating;
}

async function write(text: string) {
  const api = offscreenApi();
  if (api) {
    await offscreenReady(api);
    await ext.runtime.sendMessage({ target: 'offscreen', type: 'copy', text });
    return;
  }
  try {
    await navigator.clipboard.writeText(text);
  } catch (error) {
    // Safari's background page: the older way, which some versions still allow there.
    if (typeof document === 'undefined' || !legacyCopy(text)) throw error;
  }
}

function legacyCopy(text: string): boolean {
  const field = document.createElement('textarea');
  field.value = text;
  document.body.append(field);
  field.select();
  try {
    return document.execCommand('copy');
  } catch {
    return false;
  } finally {
    field.remove();
  }
}

/** Copy `text`; cleared again after the configured number of seconds. */
export async function copy(text: string): Promise<void> {
  await write(text);
  await clearLater();
}

/** The clearing alarm for what was just copied (here, or by Safari's popup). */
export async function clearLater(): Promise<void> {
  const { clipboardClear } = await settings();
  await ext.alarms.clear(ALARM);
  if (clipboardClear > 0) {
    await ext.alarms.create(ALARM, { when: Date.now() + clipboardClear * 1000 });
  }
}

/** Empty the clipboard now (the alarm, or locking). */
export async function clear(): Promise<void> {
  await ext.alarms.clear(ALARM);
  await write('').catch(() => undefined);
}

export function isClearAlarm(name: string): boolean {
  return name === ALARM;
}
