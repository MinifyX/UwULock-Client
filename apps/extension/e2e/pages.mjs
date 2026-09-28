// The web pages the end-to-end test fills and signs in to, served on 127.0.0.1 and opened
// as http://localhost (a secure context, so WebAuthn works without a certificate).

import { createServer } from 'node:http';

const LOGIN = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Sign in</title></head>
<body>
  <h1>Sign in to the test site</h1>
  <form method="post" action="/login">
    <label>Username <input id="username" name="username" autocomplete="username"></label>
    <label>Password <input id="password" name="password" type="password" autocomplete="current-password"></label>
    <button id="submit" type="submit">Sign in</button>
  </form>
</body></html>`;

const WELCOME = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Welcome</title></head>
<body><h1 id="welcome">Welcome back</h1><input id="search" type="search" placeholder="Search"></body></html>`;

// Registers a passkey, then signs in with it and checks the signature with WebCrypto — what a
// relying party's server would do.
const WEBAUTHN = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Passkeys</title></head>
<body>
  <h1>Passkeys</h1>
  <button id="register">Register</button>
  <button id="signin">Sign in</button>
  <p id="result">idle</p>
<script>
const out = (text) => { document.getElementById('result').textContent = text; };
const random = (n) => crypto.getRandomValues(new Uint8Array(n));
const b64 = (buf) => btoa(String.fromCharCode(...new Uint8Array(buf)));
let stored = null;

function rawSignature(der) {
  // DER ECDSA (SEQUENCE { INTEGER r, INTEGER s }) to the 64 bytes WebCrypto verifies.
  const bytes = new Uint8Array(der);
  let at = 2;
  const part = () => {
    at++; const len = bytes[at++]; let value = bytes.slice(at, at + len); at += len;
    while (value.length > 32 && value[0] === 0) value = value.slice(1);
    const out = new Uint8Array(32); out.set(value, 32 - value.length); return out;
  };
  const r = part(); const s = part();
  const raw = new Uint8Array(64); raw.set(r); raw.set(s, 32); return raw;
}

document.getElementById('register').onclick = async () => {
  try {
    const challenge = random(32);
    const credential = await navigator.credentials.create({ publicKey: {
      rp: { id: 'localhost', name: 'Passkey test' },
      user: { id: random(16), name: 'nyu@example.com', displayName: 'Nyu' },
      challenge,
      pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
      authenticatorSelection: { residentKey: 'required', userVerification: 'preferred' },
      timeout: 60000,
    }});
    const data = JSON.parse(new TextDecoder().decode(credential.response.clientDataJSON));
    if (data.type !== 'webauthn.create' || data.origin !== location.origin) throw new Error('client data');
    if (!(credential instanceof PublicKeyCredential)) throw new Error('not a PublicKeyCredential');
    const auth = new Uint8Array(credential.response.getAuthenticatorData());
    if ((auth[32] & 0x41) !== 0x41) throw new Error('flags ' + auth[32]);
    stored = { id: credential.rawId, key: credential.response.getPublicKey() };
    out('registered ' + credential.id);
  } catch (e) { out('error ' + e.name + ': ' + e.message); }
};

document.getElementById('signin').onclick = async () => {
  try {
    const challenge = random(32);
    const credential = await navigator.credentials.get({ publicKey: {
      rpId: 'localhost', challenge, userVerification: 'preferred', timeout: 60000,
    }});
    if (b64(credential.rawId) !== b64(stored.id)) throw new Error('another credential');
    const clientData = credential.response.clientDataJSON;
    const data = JSON.parse(new TextDecoder().decode(clientData));
    const expected = b64(challenge).replace(/\\+/g, '-').replace(/\\//g, '_').replace(/=+$/, '');
    if (data.type !== 'webauthn.get' || data.challenge !== expected) throw new Error('client data');
    const key = await crypto.subtle.importKey('spki', stored.key, { name: 'ECDSA', namedCurve: 'P-256' }, false, ['verify']);
    const hash = await crypto.subtle.digest('SHA-256', clientData);
    const auth = new Uint8Array(credential.response.authenticatorData);
    const signed = new Uint8Array(auth.length + 32); signed.set(auth); signed.set(new Uint8Array(hash), auth.length);
    const ok = await crypto.subtle.verify({ name: 'ECDSA', hash: 'SHA-256' }, key, rawSignature(credential.response.signature), signed);
    out(ok ? 'verified' : 'bad signature');
  } catch (e) { out('error ' + e.name + ': ' + e.message); }
};
</script>
</body></html>`;

/** Starts the pages on a free port; resolves to `{ url, close }`. */
export function startPages() {
  const server = createServer((request, response) => {
    const path = new URL(request.url ?? '/', 'http://localhost').pathname;
    if (request.method === 'POST' && path === '/login') {
      request.resume();
      request.on('end', () => {
        response.writeHead(303, { Location: '/welcome' });
        response.end();
      });
      return;
    }
    const page = { '/login': LOGIN, '/welcome': WELCOME, '/webauthn': WEBAUTHN }[path];
    if (!page) {
      response.writeHead(404);
      response.end();
      return;
    }
    response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    response.end(page);
  });
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      resolve({ url: `http://localhost:${port}`, close: () => server.close() });
    });
  });
}
