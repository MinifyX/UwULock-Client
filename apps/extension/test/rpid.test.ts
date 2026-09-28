import { describe, expect, it } from 'vitest';
import { checkRpId } from '../src/shared/rpid';

const ok = (origin: string, rpId?: string) => checkRpId(origin, rpId).ok;

describe('rpId', () => {
  it('is the host, or a registrable domain above it', () => {
    expect(checkRpId('https://login.example.com', undefined)).toEqual({
      ok: true,
      rpId: 'login.example.com',
    });
    expect(ok('https://login.example.com', 'example.com')).toBe(true);
    expect(ok('https://login.example.com', 'login.example.com')).toBe(true);
  });

  it('is never another site, a public suffix or a sub-domain below the page', () => {
    expect(ok('https://login.example.com', 'example.org')).toBe(false);
    expect(ok('https://login.example.com', 'com')).toBe(false);
    expect(ok('https://example.com', 'login.example.com')).toBe(false);
    expect(ok('https://a.github.io', 'github.io')).toBe(false);
    expect(ok('https://evilexample.com', 'example.com')).toBe(false);
  });

  it('needs a secure origin', () => {
    expect(ok('http://example.com')).toBe(false);
    expect(ok('http://localhost:8080')).toBe(true);
    expect(ok('https://192.0.2.1')).toBe(false);
    expect(ok('file:///etc/passwd')).toBe(false);
  });
});
