/**
 * Telling the rest of the extension that something changed: the popup and prompt window ask
 * for the status again, content scripts for their page's items, and whoever registered here
 * (the context menu, the badge) updates itself.
 */

import { ext } from '../shared/browser';
import type { BackgroundMessage, StatusMessage } from '../shared/protocol';

type Listener = () => void | Promise<void>;
const listeners: Listener[] = [];

export function onChanged(listener: Listener) {
  listeners.push(listener);
}

let queued = false;

/** Something about the vault changed; several calls in a row notify once. */
export function changed() {
  if (queued) return;
  queued = true;
  queueMicrotask(() => {
    queued = false;
    void notify();
  });
}

async function notify() {
  for (const listener of listeners) {
    try {
      await listener();
    } catch {
      // One listener failing doesn't stop the others.
    }
  }
  const status: StatusMessage = { type: 'bg:status-changed' };
  ext.runtime.sendMessage(status).catch(() => undefined);
  // Every tab: those without the content script (browser pages) just don't answer.
  const tabs = await ext.tabs.query({}).catch(() => [] as chrome.tabs.Tab[]);
  const message: BackgroundMessage = { type: 'bg:vault-changed' };
  for (const tab of tabs) {
    if (tab.id !== undefined) ext.tabs.sendMessage(tab.id, message).catch(() => undefined);
  }
}
