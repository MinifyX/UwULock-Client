// @vitest-environment node
/**
 * Requests without a session (background/http.ts): a redirect is never followed, and no answer
 * is read past its size limit — neither one that says it is too large nor one that just keeps
 * coming.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { anonymous, ApiError, MAX_BODY_BYTES, readBody, TOO_LARGE } from '../src/background/http';

afterEach(() => vi.unstubAllGlobals());

/** A body that sends `chunks` pieces of `size` bytes, and notes whether it was cancelled. */
function streamed(chunks: number, size: number) {
  let sent = 0;
  const state = { cancelled: false };
  const body = new ReadableStream<Uint8Array>({
    pull(controller) {
      if (sent >= chunks) {
        controller.close();
        return;
      }
      sent += 1;
      controller.enqueue(new Uint8Array(size).fill(0x20));
    },
    cancel() {
      state.cancelled = true;
    },
  });
  return { body, state };
}

describe('anonymous requests', () => {
  it('ask fetch to fail on a redirect', async () => {
    const calls: RequestInit[] = [];
    vi.stubGlobal('fetch', async (_url: string, init: RequestInit) => {
      calls.push(init);
      return new Response('{"ok":true}', { status: 200 });
    });
    await expect(
      anonymous('https://vault.example.com/identity/accounts/prelogin'),
    ).resolves.toEqual({ ok: true });
    expect(calls[0]?.redirect).toBe('error');
    expect(calls[0]?.credentials).toBe('omit');
  });

  it('treat a redirect handed back anyway as an error', async () => {
    vi.stubGlobal(
      'fetch',
      async () =>
        new Response(null, { status: 307, headers: { Location: 'https://evil.example/' } }),
    );
    const error = await anonymous('https://vault.example.com/identity/connect/token').catch(
      (e: unknown) => e,
    );
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).status).toBe(0);
  });

  it('refuse an answer that grows past the limit', async () => {
    const { body, state } = streamed(10_000, 64 * 1024);
    vi.stubGlobal('fetch', async () => new Response(body, { status: 200 }));
    await expect(anonymous('https://vault.example.com/api/sync')).rejects.toEqual(TOO_LARGE);
    expect(state.cancelled).toBe(true);
  });

  it('take a larger limit where the caller gives one', async () => {
    const { body } = streamed(4, 1024);
    vi.stubGlobal('fetch', async () => new Response(body, { status: 200 }));
    await expect(
      anonymous('https://vault.example.com/api/sync', { maxBytes: 8 * 1024 }),
    ).resolves.toBe(' '.repeat(4096));
  });
});

describe('readBody', () => {
  it('refuses a declared length above the limit before reading', async () => {
    const { body, state } = streamed(1, 16);
    const response = new Response(body, {
      headers: { 'Content-Length': String(MAX_BODY_BYTES + 1) },
    });
    await expect(readBody(response, MAX_BODY_BYTES)).rejects.toEqual(TOO_LARGE);
    expect(state.cancelled).toBe(true);
  });

  it('reads an answer up to exactly the limit', async () => {
    const { body } = streamed(2, 512);
    expect((await readBody(new Response(body), 1024)).length).toBe(1024);
    const { body: over } = streamed(3, 512);
    await expect(readBody(new Response(over), 1024)).rejects.toEqual(TOO_LARGE);
  });
});
