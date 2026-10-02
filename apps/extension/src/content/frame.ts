/**
 * What the background asks a frame about its own document (R4-2): the document's real origin
 * and its ancestors' origins, read here in the content script's isolated world, where the page
 * can't change what these return.
 *
 * `window.origin` is the document's origin: `"null"` for a sandboxed document (an iframe with
 * `sandbox` and no `allow-same-origin`, or a page served with `Content-Security-Policy:
 * sandbox`), even though its address — and `location.origin` — is the site's. Content scripts
 * are injected into such documents by their address, so the background can't go by the address
 * alone.
 *
 * Ancestors: `location.ancestorOrigins` where the browser has it (Chromium), which names every
 * ancestor's origin, opaque ones as `"null"`. Besides, `parents` walks `window.parent` up to the
 * top and reads each one's `origin` — a frame of another origin throws, and counts as
 * `"null"`.
 */

import { ext } from '../shared/browser';

export const FRAME_DOCUMENT = 'bg:frame-document';

export type FrameDocument = {
  origin: string;
  /** `location.ancestorOrigins`, nearest first; `null` where the browser has none. */
  ancestors: string[] | null;
  /** The origins up the `window.parent` chain, nearest first; `"null"` where unreadable. */
  parents: string[] | null;
};

export function frameDocument(): FrameDocument {
  const origin = String(window.origin);
  let ancestors: string[] | null = null;
  try {
    const list = (location as Location & { ancestorOrigins?: DOMStringList }).ancestorOrigins;
    if (list) ancestors = Array.from(list);
  } catch {
    ancestors = null;
  }
  let parents: string[] | null = [];
  try {
    let current: Window = window;
    for (let depth = 0; depth < 64 && current.parent !== current; depth++) {
      current = current.parent;
      let parentOrigin = 'null';
      try {
        parentOrigin = String(current.origin);
      } catch {
        // Another origin.
      }
      parents.push(parentOrigin);
    }
  } catch {
    parents = null;
  }
  return { origin, ancestors, parents };
}

/** Answers the background's `bg:frame-document`; true when it was that question. */
export function answerFrameDocument(
  message: unknown,
  sender: chrome.runtime.MessageSender,
  respond: (answer: unknown) => void,
): boolean {
  if (sender.id !== ext.runtime.id || sender.tab !== undefined) return false;
  if (!message || (message as { type?: unknown }).type !== FRAME_DOCUMENT) return false;
  respond(frameDocument());
  return true;
}
