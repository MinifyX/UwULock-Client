import { describe, expect, it } from './expect.ts';
import { readCredentialExchange } from '../src/lib/import/cxf.ts';
import type { ExportItem } from '../src/lib/import/types.ts';

/** What the iOS plugin hands over: Apple's ASExportedCredentialData, JSON-encoded. */
const exchange = {
  version: { major: 1, minor: 0 },
  exporterRpId: 'apple.com',
  exporterDisplayName: 'Passwörter',
  timestamp: 1790000000,
  accounts: [
    {
      id: 'YWNjb3VudA',
      username: '',
      email: 'nyu@example.com',
      collections: [
        {
          id: 'c1',
          title: 'Arbeit',
          items: [{ item: 'i1' }],
          subCollections: [{ id: 'c2', title: 'Server', items: [{ item: 'i3' }] }],
        },
      ],
      items: [
        {
          id: 'i1',
          creationAt: 1700000000,
          title: 'Example',
          favorite: true,
          scope: {
            urls: ['https://login.example.com/'],
            androidApps: [{ bundleId: 'com.example.app' }],
          },
          credentials: [
            {
              type: 'basic-auth',
              username: { fieldType: 'string', value: 'nyu' },
              password: { fieldType: 'concealed-string', value: 'correct horse' },
            },
            {
              type: 'passkey',
              credentialId: 'AQIDBA',
              rpId: 'Example.com',
              username: 'nyu',
              userDisplayName: 'Nyu',
              userHandle: 'dXNlcg',
              key: 'MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg',
            },
            { type: 'totp', secret: 'JBSWY3DPEHPK3PXP', algorithm: 'sha1', digits: 6, period: 30 },
            { type: 'note', content: { fieldType: 'string', value: 'Erste Notiz' } },
          ],
        },
        {
          id: 'i2',
          title: 'Bank',
          credentials: [
            {
              type: 'credit-card',
              number: '4111111111111111',
              fullName: 'Nyu Uwu',
              expiryDate: '2029-07',
              verificationNumber: '123',
            },
            { type: 'note', content: 'Karte im Schrank' },
          ],
        },
        {
          id: 'i3',
          title: 'Zuhause',
          credentials: [
            {
              type: 'wifi',
              ssid: 'uwu-net',
              passphrase: 'correct; horse',
              networkSecurityType: 'wpa2-personal',
              hidden: false,
            },
          ],
        },
        {
          id: 'i4',
          title: 'Zwei Passkeys',
          credentials: [
            {
              type: 'passkey',
              credentialId: 'AQ',
              rpId: 'example.net',
              username: 'a',
              userHandle: 'YQ',
              key: 'AAEC',
            },
            {
              type: 'passkey',
              credentialId: 'Ag',
              rpId: 'example.net',
              username: 'b',
              userHandle: 'Yg',
              key: 'AAED',
            },
            { type: 'passkey', credentialId: '', rpId: 'example.net', key: '' },
          ],
        },
        { id: 'i5', title: 'Nur Notiz', credentials: [{ type: 'note', content: 'Hallo' }] },
        {
          id: 'i6',
          title: 'Code mit 8 Stellen',
          credentials: [
            {
              type: 'totp',
              secret: 'JBSWY3DPEHPK3PXP',
              digits: 8,
              period: 30,
              algorithm: 'sha256',
              issuer: 'Example',
            },
          ],
        },
      ],
    },
  ],
};

const read = () => readCredentialExchange(JSON.stringify(exchange));
const named = (items: ExportItem[], name: string) => items.filter((i) => i.name === name);

describe('Credential Exchange (Apple Passwords, iOS 26)', () => {
  it('logins carry password, passkey, code, addresses and notes', () => {
    const { data, format } = read();
    expect(format).toBe('Passwörter (CXF)');
    const [login] = named(data.items, 'Example');
    expect(login!.favorite).toBe(true);
    expect(login!.notes).toBe('Erste Notiz');
    expect(login!.login!.username).toBe('nyu');
    expect(login!.login!.password).toBe('correct horse');
    expect(login!.login!.totp).toBe('JBSWY3DPEHPK3PXP');
    expect(login!.login!.uris.map((u) => u.uri)).toEqual([
      'https://login.example.com/',
      'androidapp://com.example.app',
    ]);
    const [passkey] = login!.login!.fido2Credentials!;
    expect(passkey).toMatchObject({
      credentialId: 'AQIDBA',
      rpId: 'example.com',
      userHandle: 'dXNlcg',
      userName: 'nyu',
      counter: '0',
      creationDate: new Date(1700000000 * 1000).toISOString(),
    });
    const folder = data.folders.find((f) => f.id === login!.folderId);
    expect(folder!.name).toBe('Arbeit');
  });

  it('cards, Wi-Fi networks and nested collections', () => {
    const { data } = read();
    const [card] = named(data.items, 'Bank');
    expect(card!.type).toBe(3);
    expect(card!.card).toMatchObject({
      number: '4111111111111111',
      expMonth: '7',
      expYear: '2029',
      code: '123',
      cardholderName: 'Nyu Uwu',
    });
    expect(card!.notes).toBe('Karte im Schrank');
    const [wifi] = named(data.items, 'Zuhause');
    expect(wifi!.type).toBe(2);
    expect(wifi!.fields.find((f) => f.name === 'SSID')!.value).toBe('uwu-net');
    expect(data.folders.find((f) => f.id === wifi!.folderId)!.name).toBe('Arbeit/Server');
  });

  it('one passkey per login, a broken one is warned about', () => {
    const { data, warnings } = read();
    const two = named(data.items, 'Zwei Passkeys');
    expect(two.map((i) => i.login!.fido2Credentials![0]!.userName)).toEqual(['a', 'b']);
    expect(two.map((i) => i.login!.uris.map((u) => u.uri))).toEqual([
      ['https://example.net'],
      ['https://example.net'],
    ]);
    expect(warnings.some((w) => w.includes('Passkey'))).toBe(true);
  });

  it('a note alone is a secure note; codes other than the default become otpauth', () => {
    const { data } = read();
    const [note] = named(data.items, 'Nur Notiz');
    expect(note!.type).toBe(2);
    expect(note!.notes).toBe('Hallo');
    const [code] = named(data.items, 'Code mit 8 Stellen');
    expect(code!.login!.totp).toBe(
      'otpauth://totp/Example?secret=JBSWY3DPEHPK3PXP&algorithm=SHA256&digits=8&period=30&issuer=Example',
    );
  });

  it("reads Apple's base64 secrets as base64, even when they look like base32", () => {
    const one = (secret: string, extra: Record<string, unknown> = {}) =>
      readCredentialExchange(
        JSON.stringify({
          accounts: [
            {
              id: 'a',
              items: [
                {
                  id: 'c',
                  title: 'Code',
                  credentials: [{ type: 'totp', secret, ...extra }],
                },
              ],
            },
          ],
        }),
      ).data.items[0]!.login!.totp;
    expect(one('AevVv6mTfWdROw==')).toBe('AHV5LP5JSN6WOUJ3');
    expect(one('JBSWY3DPEHPK3PXP')).toBe('JBSWY3DPEHPK3PXP');
    // Nonsense digits and periods fall back to the defaults.
    expect(one('JBSWY3DPEHPK3PXP', { digits: 'x', period: -5 })).toBe('JBSWY3DPEHPK3PXP');
  });

  it('a passkey before the password still makes one login', () => {
    const { data } = readCredentialExchange(
      JSON.stringify({
        accounts: [
          {
            id: 'a',
            items: [
              {
                id: 'p',
                title: 'Passkey zuerst',
                credentials: [
                  {
                    type: 'passkey',
                    credentialId: 'AQIDBA',
                    rpId: 'example.com',
                    username: 'nyu',
                    userHandle: 'dXNlcg',
                    key: 'MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg',
                  },
                  { type: 'basic-auth', username: 'nyu', password: 'geheim' },
                ],
              },
            ],
          },
        ],
      }),
    );
    expect(data.items.length).toBe(1);
    expect(data.items[0]!.login!.password).toBe('geheim');
    expect(data.items[0]!.login!.fido2Credentials!.length).toBe(1);
  });

  it('refuses what is not CXF', () => {
    expect(() => readCredentialExchange('nope')).toThrow();
    expect(() => readCredentialExchange('{"items":[]}')).toThrow();
  });
});
