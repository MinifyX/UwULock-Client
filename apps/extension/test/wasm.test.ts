// @vitest-environment node
/**
 * The background's WebAssembly (crates/uwulock-wasm), called the way the background calls it:
 * a vault encrypted here with Node's own crypto, the way Bitwarden's apps encrypt, opens there;
 * Bitwarden's known answers hold; a passkey made there signs what WebCrypto verifies.
 *
 * Needs the module built (`pnpm wasm`); CI builds it before the tests.
 */

import {
  createCipheriv,
  createDecipheriv,
  createHmac,
  hkdfSync,
  pbkdf2Sync,
  randomBytes,
  webcrypto,
} from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';

const pkg = new URL('../src/wasm/pkg/', import.meta.url);
const built = existsSync(new URL('core_bg.wasm', pkg));

type Core = typeof import('../src/wasm/pkg/core.js');
let core: Core;

const EMAIL = 'nyu@example.com';
const PASSWORD = 'correct horse battery staple';
const KDF = JSON.stringify({ kdf: 0, kdfIterations: 5000 });

function b64url(bytes: Uint8Array | ArrayBuffer): string {
  return Buffer.from(bytes as ArrayBuffer).toString('base64url');
}

/** `2.iv|data|mac`: AES-256-CBC with HMAC-SHA256, as every Bitwarden value is stored. */
function encrypt(plain: Buffer, key: Buffer): string {
  const iv = randomBytes(16);
  const cipher = createCipheriv('aes-256-cbc', key.subarray(0, 32), iv);
  const data = Buffer.concat([cipher.update(plain), cipher.final()]);
  const mac = createHmac('sha256', key.subarray(32))
    .update(Buffer.concat([iv, data]))
    .digest();
  return `2.${iv.toString('base64')}|${data.toString('base64')}|${mac.toString('base64')}`;
}

/** An account and a vault with one login, made without the module. */
function account() {
  const master = pbkdf2Sync(PASSWORD, EMAIL, 5000, 32, 'sha256');
  const expand = (info: string) =>
    createHmac('sha256', master)
      .update(Buffer.concat([Buffer.from(info), Buffer.from([1])]))
      .digest();
  const stretched = Buffer.concat([expand('enc'), expand('mac')]);
  const userKey = randomBytes(64);
  const protectedKey = encrypt(userKey, stretched);
  const value = (text: string) => encrypt(Buffer.from(text), userKey);
  const sync = {
    profile: { id: 'user-1', email: EMAIL, name: 'Nyu', key: protectedKey, organizations: [] },
    folders: [],
    collections: [],
    ciphers: [
      {
        id: 'login-1',
        type: 1,
        name: value('Example'),
        favorite: false,
        reprompt: 0,
        revisionDate: '2026-09-01T00:00:00Z',
        login: {
          username: value('nyu'),
          password: value('hunter2'),
          uris: [{ uri: value('https://login.example.com/'), match: null }],
        },
      },
    ],
  };
  return {
    protectedKey,
    sync,
    hash: pbkdf2Sync(master, PASSWORD, 1, 32, 'sha256').toString('base64'),
  };
}

/** DER ECDSA to the r‖s WebCrypto verifies. */
function rawSignature(der: Uint8Array): Uint8Array {
  let at = 2;
  const part = () => {
    at++;
    const length = der[at++]!;
    let value = der.slice(at, at + length);
    at += length;
    while (value.length > 32 && value[0] === 0) value = value.slice(1);
    const out = new Uint8Array(32);
    out.set(value, 32 - value.length);
    return out;
  };
  const raw = new Uint8Array(64);
  raw.set(part());
  raw.set(part(), 32);
  return raw;
}

describe.skipIf(!built)('the WebAssembly', () => {
  beforeAll(async () => {
    core = await import('../src/wasm/pkg/core.js');
    core.initSync({ module: readFileSync(new URL('core_bg.wasm', pkg)) });
  });

  it('hashes the master password as Bitwarden does', () => {
    const hash = core.deriveLogin(
      'test@bitwarden.com',
      'asdfasdf',
      JSON.stringify({ kdf: 0, kdfIterations: 100_000 }),
    );
    expect(hash).toBe('wmyadRMyBZOH7P/a/ucTCbSghKgdzDpPqUnu/DAVtSw=');
    core.lock();
  });

  it('opens a vault encrypted elsewhere, and a wrong password does not', () => {
    const { protectedKey, sync, hash } = account();
    expect(() => core.unlockWithPassword(EMAIL, KDF, protectedKey, 'wrong')).toThrow(
      /wrong-password/,
    );
    expect(core.unlockWithPassword(EMAIL, KDF, protectedKey, PASSWORD)).toBe(hash);
    core.open(JSON.stringify(sync));
    const [entry] = JSON.parse(core.autofillIndex()) as { id: string; uris: { uri: string }[] }[];
    expect(entry?.uris[0]?.uri).toBe('https://login.example.com/');
    expect(JSON.parse(core.fillValues('login-1', Date.now() / 1000))).toMatchObject({
      kind: 'login',
      username: 'nyu',
      password: 'hunter2',
    });
    // Restored from the key alone, as after the service worker was ended.
    const key = core.userKey();
    core.lock();
    expect(core.isUnlocked()).toBe(false);
    core.unlockWithKey(EMAIL, KDF, protectedKey, key);
    core.open(JSON.stringify(sync));
    expect(core.reveal('login-1', 'password', 0)).toBe('hunter2');
    core.lock();
  });

  it('shares a login as a Send that opens elsewhere, without its authenticator key', () => {
    const { protectedKey, sync } = account();
    core.unlockWithPassword(EMAIL, KDF, protectedKey, PASSWORD);
    core.open(JSON.stringify(sync));
    expect(JSON.parse(core.shareableFields('login-1'))).toEqual([
      { name: 'username' },
      { name: 'password' },
      { name: 'uri:0' },
    ]);
    const request = JSON.parse(
      core.sealShare(
        'login-1',
        JSON.stringify({
          fields: [
            ['username', 'Username'],
            ['totp', 'Code'],
          ],
          deletionDate: '2026-09-29T12:00:00.000Z',
          maxAccessCount: 1,
        }),
      ),
    );
    const decrypt = (value: string, key: Buffer) => {
      const [iv, data] = value
        .slice(2)
        .split('|')
        .map((part) => Buffer.from(part, 'base64'));
      const decipher = createDecipheriv('aes-256-cbc', key.subarray(0, 32), iv!);
      return Buffer.concat([decipher.update(data!), decipher.final()]);
    };
    const seed = decrypt(request.key, Buffer.from(core.userKey(), 'base64'));
    // Bitwarden's derive_shareable_key: HKDF-SHA256, salt "bitwarden-send", info "send".
    const sendKey = Buffer.from(hkdfSync('sha256', seed, 'bitwarden-send', 'send', 64));
    expect(decrypt(request.text.text, sendKey).toString()).toBe('Example\nUsername: nyu');
    expect(core.sendLink(request.key, 'acc', 'https://lock.example.com', false)).toBe(
      `https://lock.example.com/#/send/acc/${b64url(seed)}`,
    );
    core.lock();
  });

  it('unlocks with a PIN, and not with a wrong one', () => {
    const { protectedKey } = account();
    core.unlockWithPassword(EMAIL, KDF, protectedKey, PASSWORD);
    const wrapped = core.pinProtect('4711');
    core.lock();
    expect(() => core.unlockWithPin(EMAIL, KDF, protectedKey, '1234', wrapped)).toThrow();
    core.unlockWithPin(EMAIL, KDF, protectedKey, '4711', wrapped);
    expect(core.isUnlocked()).toBe(true);
    core.lock();
  });

  it('makes a passkey whose signatures WebCrypto verifies', async () => {
    const { protectedKey, sync } = account();
    core.unlockWithPassword(EMAIL, KDF, protectedKey, PASSWORD);
    core.open(JSON.stringify(sync));
    const created = JSON.parse(
      core.passkeyCreate(
        JSON.stringify({
          itemId: 'login-1',
          name: 'Example',
          folderId: null,
          rpId: 'example.com',
          rpName: 'Example',
          userHandle: b64url(new Uint8Array([1, 2, 3, 4])),
          userName: 'nyu',
          userDisplayName: 'Nyu',
          discoverable: true,
          userVerified: false,
          now: new Date().toISOString(),
        }),
      ),
    ) as {
      credentialId: string;
      publicKey: string;
      authenticatorData: string;
      cipher: { login: { fido2Credentials: unknown[] } };
    };
    expect(created.cipher.login.fido2Credentials).toHaveLength(1);
    const created_auth = Buffer.from(created.authenticatorData, 'base64url');
    const rpIdHash = Buffer.from(
      await webcrypto.subtle.digest('SHA-256', Buffer.from('example.com')),
    );
    expect(created_auth.subarray(0, 32).equals(rpIdHash)).toBe(true);
    expect(created_auth[32]! & 0x41).toBe(0x41); // user present, attested credential data

    // The saved item comes back from the server with its passkey: open that, and sign.
    const saved = structuredClone(sync);
    saved.ciphers[0]!.login = {
      ...saved.ciphers[0]!.login,
      ...created.cipher.login,
    } as (typeof saved.ciphers)[0]['login'];
    core.open(JSON.stringify(saved));
    const clientDataHash = new Uint8Array(
      await webcrypto.subtle.digest('SHA-256', Buffer.from('{"type":"webauthn.get"}')),
    );
    const asserted = JSON.parse(
      core.passkeyAssert(
        JSON.stringify({
          itemId: 'login-1',
          credentialId: created.credentialId,
          rpId: 'example.com',
          clientDataHash: b64url(clientDataHash),
          userVerified: true,
        }),
      ),
    ) as { authenticatorData: string; signature: string; userHandle: string };
    expect(asserted.userHandle).toBe(b64url(new Uint8Array([1, 2, 3, 4])));
    const auth = Buffer.from(asserted.authenticatorData, 'base64url');
    expect(auth[32]! & 0x05).toBe(0x05); // user present and verified
    const key = await webcrypto.subtle.importKey(
      'spki',
      Buffer.from(created.publicKey, 'base64url'),
      { name: 'ECDSA', namedCurve: 'P-256' },
      false,
      ['verify'],
    );
    const signed = Buffer.concat([auth, clientDataHash]);
    const signature = rawSignature(Buffer.from(asserted.signature, 'base64url'));
    expect(
      await webcrypto.subtle.verify({ name: 'ECDSA', hash: 'SHA-256' }, key, signature, signed),
    ).toBe(true);
    // Another site's rpId gets nothing.
    expect(() =>
      core.passkeyAssert(
        JSON.stringify({
          itemId: 'login-1',
          credentialId: created.credentialId,
          rpId: 'example.org',
          clientDataHash: b64url(clientDataHash),
          userVerified: false,
        }),
      ),
    ).toThrow();
    core.lock();
  });
});
