import { describe, expect, it } from 'vitest';
import {
  domainOf,
  equivalentDomains,
  hostOf,
  isFillableUrl,
  isInsecureUrl,
  GLOBAL_DOMAINS,
  itemMatches,
  matchingDomains,
  MAX_REGEX_LENGTH,
  MAX_REGEX_TARGET,
  pageDomains,
  safeRegex,
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

  it("domain: Bitwarden's global groups come with the extension, the server only switches them off", () => {
    const groups = equivalentDomains({
      equivalentDomains: [['example.com', 'example.org']],
      globalEquivalentDomains: [
        // The server's copy of a group is not taken, whatever it lists.
        { type: 0, domains: ['google.com', 'example.com'], excluded: false },
        { type: 1, domains: ['apple.com', 'icloud.com'], excluded: true },
      ],
    });
    const google = matchingDomains('https://mail.google.com/', groups.global);
    expect(google.has('youtube.com')).toBe(true);
    expect(google.has('example.com')).toBe(false);
    const apple = matchingDomains('https://www.apple.com/', groups.global);
    expect(apple.has('icloud.com')).toBe(false);
    expect(groups.custom).toEqual([['example.com', 'example.org']]);
  });

  it("domain: the account's own groups only widen the wide set", () => {
    const groups = equivalentDomains({ equivalentDomains: [['example.com', 'example.org']] });
    const { strict, wide } = pageDomains(page, groups);
    expect(matches('https://login.example.org', 0, page, wide)).toBe(true);
    expect(matches('https://login.example.org', 0, page, strict)).toBe(false);
    const none = equivalentDomains(null);
    expect(none.custom).toEqual([]);
    expect(none.global.length).toBe(GLOBAL_DOMAINS.length);
  });

  it('host: the same host and port', () => {
    expect(matches('https://accounts.example.com:8443', 1)).toBe(true);
    expect(matches('https://accounts.example.com', 1)).toBe(false);
    expect(matches('https://www.example.com:8443', 1)).toBe(false);
  });

  it('starts with: the same origin first', () => {
    const bank = 'https://bank.example';
    expect(uriMatches({ uri: bank, match: 2 }, 'https://bank.example/login', domains)).toBe(true);
    expect(uriMatches({ uri: bank, match: 2 }, 'https://bank.example.evil.test/', domains)).toBe(
      false,
    );
    expect(uriMatches({ uri: bank, match: 2 }, 'https://bank.example:8443/', domains)).toBe(false);
    expect(uriMatches({ uri: bank, match: 2 }, 'https://bank.example@evil.test/', domains)).toBe(
      false,
    );
    expect(uriMatches({ uri: 'bank.example/', match: 2 }, 'https://bank.example/', domains)).toBe(
      false,
    );
  });

  it('regular expressions: top frame only, short, and nothing that backtracks for long', () => {
    const re = { uri: '^https://accounts\\.example\\.com', match: 4 };
    expect(uriMatches(re, page, domains, 0, { regex: true })).toBe(true);
    expect(uriMatches(re, page, domains, 0, { regex: false })).toBe(false);
    expect(uriMatches(re, `${page}${'a'.repeat(MAX_REGEX_TARGET)}`, domains)).toBe(false);
    expect(uriMatches({ uri: `${'a'.repeat(MAX_REGEX_LENGTH)}|x`, match: 4 }, page, domains)).toBe(
      false,
    );
    for (const slow of [
      '(a+)+$',
      '(a*)*b',
      '(a|aa)+$',
      '((ab)*c)+',
      '(x+x+)+y',
      '.*.*.*=',
      '(\\w{1,9})+z',
    ]) {
      expect(safeRegex(slow), slow).toBe(false);
    }
    for (const fine of [
      '^https://(www\\.)?example\\.com/.*$',
      '^https?://[a-z]+\\.example\\.com/(login|signin)',
      '\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}',
      '[(+*]+example\\(x\\)+',
    ]) {
      expect(safeRegex(fine), fine).toBe(true);
    }
    const started = performance.now();
    uriMatches({ uri: '(a+)+$', match: 4 }, `https://example.com/${'a'.repeat(40)}!`, domains);
    expect(performance.now() - started).toBeLessThan(50);
  });

  it('exact, regular expression, never', () => {
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
