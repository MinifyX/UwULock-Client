/**
 * Between the page's WebAuthn calls (page/webauthn.ts, in the page's own world) and the
 * background: requests come in by `postMessage` from this very window, go to the background as
 * `content:passkey-*`, and the answer goes back the same way. One request at a time per frame;
 * anything malformed is dropped. The page learns nothing here it wouldn't get from WebAuthn
 * itself.
 */

import { ask } from '../shared/messages';
import type { PasskeyAnswer, PasskeyCreateOptions, PasskeyGetOptions } from '../shared/protocol';

(() => {
  const CHANNEL = 'uwulock-webauthn';
  let busy: string | null = null;

  function answer(requestId: string, value: PasskeyAnswer) {
    window.postMessage(
      { channel: CHANNEL, direction: 'to-page', requestId, answer: value },
      window.location.origin,
    );
  }

  window.addEventListener('message', (event: MessageEvent) => {
    if (event.source !== window) return;
    const data = event.data as {
      channel?: unknown;
      direction?: unknown;
      kind?: unknown;
      requestId?: unknown;
      options?: unknown;
    } | null;
    if (!data || data.channel !== CHANNEL || data.direction !== 'to-content') return;
    const requestId = data.requestId;
    if (typeof requestId !== 'string' || requestId.length > 64) return;

    if (data.kind === 'abort') {
      if (busy === requestId) {
        busy = null;
        void ask({ type: 'content:passkey-abort', requestId }).catch(() => undefined);
      }
      return;
    }
    if (
      (data.kind !== 'create' && data.kind !== 'get') ||
      typeof data.options !== 'object' ||
      !data.options
    )
      return;
    if (busy) {
      answer(requestId, {
        kind: 'error',
        name: 'NotAllowedError',
        message: 'Another request is running.',
      });
      return;
    }
    busy = requestId;
    const request =
      data.kind === 'create'
        ? ask<PasskeyAnswer>({
            type: 'content:passkey-create',
            requestId,
            options: data.options as PasskeyCreateOptions,
          })
        : ask<PasskeyAnswer>({
            type: 'content:passkey-get',
            requestId,
            options: data.options as PasskeyGetOptions,
          });
    request
      // The extension isn't there (updated, disabled): the browser does it.
      .catch((): PasskeyAnswer => ({ kind: 'fallback' }))
      .then((value) => {
        if (busy === requestId) busy = null;
        answer(requestId, value);
      });
  });
})();
