import { describe, expect, it } from 'vitest';
import {
  domainOf,
  equivalentDomains,
  hostOf,
  isFillableUrl,
  isInsecureUrl,
  itemMatches,
  matchingDomains,
  uriMatches,
} from '../src/shared/uri';

const page = 'https://accounts.example.com:8443/login?next=/home';
const domains = matchingDomains(page, []);
const matches = (uri: string, match: number | null, target = page, set = domains) =>
  uriMatches({ uri, match }, target, set);

describe('domains', () => {
  it('reads the registrable domain', () => {
    expect(domainOf('https://a.b.example.co.uk/x')).toBe('example.co.uk');
    expect(domainOf('example.com')).toBe('example.com');
    expect(domainOf('http://192.0.2.10:8080/')).toBe('192.0.2.10');
    expect(domainOf('http://localhost:3000')).toBe('localhost');
    expect(domainOf('androidapp://com.example')).toBeNull();
    expect(hostOf('https://Example.com:8443/x')).toBe('example.com:8443');
  });

  it('keeps sites under a public suffix apart', () => {
    expect(domainOf('https://a.github.io')).toBe('a.github.io');
    const set = matchingDomains('https://a.github.io/', []);
    expect(matches('https://b.github.io', 0, 'https://a.github.io/', set)).toBe(false);
  });
});

describe('match detection', () => {
  it('domain: any host of the same site, ports ignored', () => {
    expect(matches('https://example.com', 0)).toBe(true);
    expect(matches('www.example.com', null)).toBe(true);
    expect(matches('https://example.net', 0)).toBe(false);
    expect(matches('https://notexample.com', 0)).toBe(false);
  });

  it('domain: equivalent domains from the server', () => {
    const groups = equivalentDomains({
      equivalentDomains: [['example.com', 'example.org']],
      globalEquivalentDomains: [
        { type: 1, domains: ['example.net', 'example.com'], excluded: true },
        { type: 2, domains: ['example.edu', 'example.com'], excluded: false },
      ],
    });
    const set = matchingDomains(page, groups);
    expect(matches('https://login.example.org', 0, page, set)).toBe(true);
    expect(matches('https://example.edu', 0, page, set)).toBe(true);
    expect(matches('https://example.net', 0, page, set)).toBe(false);
  });

  it('host: the same host and port', () => {
    expect(matches('https://accounts.example.com:8443', 1)).toBe(true);
    expect(matches('https://accounts.example.com', 1)).toBe(false);
    expect(matches('https://www.example.com:8443', 1)).toBe(false);
  });

  it('starts with, exact, regular expression, never', () => {
    expect(matches('https://accounts.example.com:8443/login', 2)).toBe(true);
    expect(matches('https://accounts.example.com:8443/logout', 2)).toBe(false);
    expect(matches(page, 3)).toBe(true);
    expect(matches('https://accounts.example.com:8443/login', 3)).toBe(false);
    expect(matches('^https://accounts\\.EXAMPLE\\.com', 4)).toBe(true);
    expect(matches('([', 4)).toBe(false);
    expect(matches('https://example.com', 5)).toBe(false);
  });

  it('the default follows the setting', () => {
    expect(uriMatches({ uri: 'https://example.com', match: null }, page, domains, 1)).toBe(false);
    expect(
      itemMatches(
        [
          { uri: 'x.test', match: 0 },
          { uri: page, match: 3 },
        ],
        page,
        domains,
      ),
    ).toBe(true);
  });
});

describe('pages', () => {
  it('fills only web pages, and knows plain http', () => {
    expect(isFillableUrl('https://example.com')).toBe(true);
    expect(isFillableUrl('chrome://settings')).toBe(false);
    expect(isFillableUrl('https://chromewebstore.google.com/detail/x')).toBe(false);
    expect(isInsecureUrl('http://example.com')).toBe(true);
    expect(isInsecureUrl('http://localhost:8080')).toBe(false);
    expect(isInsecureUrl('https://example.com')).toBe(false);
  });
});
