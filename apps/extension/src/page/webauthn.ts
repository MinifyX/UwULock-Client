/**
 * In the page itself (the MAIN world), on secure pages only: `navigator.credentials.create()`
 * and `get()` for passkeys go to UwULock first.
 *
 * This script can't talk to the extension; it posts the request to the content script
 * (content/bridge.ts) in the same window, which asks the background. Whatever the page does
 * here, it could do by calling WebAuthn itself: the background decides from the frame's real
 * origin, not from anything this script says. When UwULock doesn't answer — cancelled, no
 * passkey, not ES256, conditional mediation — the browser's own authenticator takes over,
 * with the page's original options.
 */

import type { PasskeyAnswer, PasskeyCredential } from '../shared/protocol';

(() => {
  const credentials = navigator.credentials as CredentialsContainer | undefined;
  if (!window.isSecureContext || !credentials || typeof PublicKeyCredential === 'undefined') return;

  const CHANNEL = 'uwulock-webauthn';
  const originalCreate = credentials.create.bind(credentials);
  const originalGet = credentials.get.bind(credentials);
  const waiting = new Map<string, (answer: PasskeyAnswer) => void>();

  window.addEventListener('message', (event: MessageEvent) => {
    const data = event.data as {
      channel?: string;
      direction?: string;
      requestId?: string;
      answer?: PasskeyAnswer;
    };
    if (event.source !== window || data?.channel !== CHANNEL || data.direction !== 'to-page')
      return;
    const resolve = data.requestId ? waiting.get(data.requestId) : undefined;
    if (!resolve || !data.answer) return;
    waiting.delete(data.requestId!);
    resolve(data.answer);
  });

  function toB64url(value: BufferSource | undefined | null): string {
    if (!value) return '';
    const bytes =
      value instanceof ArrayBuffer
        ? new Uint8Array(value)
        : new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
    let binary = '';
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  }

  function fromB64url(text: string): ArrayBuffer {
    const plain = text.replace(/-/g, '+').replace(/_/g, '/');
    const binary = atob(plain + '='.repeat((4 - (plain.length % 4)) % 4));
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes.buffer;
  }

  function ask(
    kind: 'create' | 'get',
    options: unknown,
    signal?: AbortSignal | null,
  ): Promise<PasskeyAnswer> {
    const requestId = crypto.randomUUID();
    return new Promise<PasskeyAnswer>((resolve) => {
      waiting.set(requestId, resolve);
      signal?.addEventListener('abort', () => {
        window.postMessage(
          { channel: CHANNEL, direction: 'to-content', kind: 'abort', requestId },
          window.location.origin,
        );
        waiting.delete(requestId);
        resolve({ kind: 'error', name: 'AbortError', message: 'The operation was aborted.' });
      });
      window.postMessage(
        { channel: CHANNEL, direction: 'to-content', kind, requestId, options },
        window.location.origin,
      );
    });
  }

  function extensions(value: AuthenticationExtensionsClientInputs | undefined) {
    // Only what can travel as JSON; `credProps` is the one UwULock answers.
    return value && 'credProps' in value ? { credProps: Boolean(value.credProps) } : {};
  }

  function serializeCreate(options: PublicKeyCredentialCreationOptions) {
    return {
      rp: { id: options.rp.id, name: options.rp.name },
      user: {
        id: toB64url(options.user.id),
        name: options.user.name,
        displayName: options.user.displayName,
      },
      challenge: toB64url(options.challenge),
      pubKeyCredParams: options.pubKeyCredParams.map((p) => ({ type: p.type, alg: p.alg })),
      excludeCredentials: (options.excludeCredentials ?? []).map((c) => ({
        id: toB64url(c.id),
        type: c.type,
      })),
      authenticatorSelection: options.authenticatorSelection
        ? {
            residentKey: options.authenticatorSelection.residentKey,
            requireResidentKey: options.authenticatorSelection.requireResidentKey,
            userVerification: options.authenticatorSelection.userVerification,
            authenticatorAttachment: options.authenticatorSelection.authenticatorAttachment,
          }
        : undefined,
      attestation: options.attestation,
      timeout: options.timeout,
      extensions: extensions(options.extensions),
    };
  }

  function serializeGet(options: PublicKeyCredentialRequestOptions, mediation: string | undefined) {
    return {
      rpId: options.rpId,
      challenge: toB64url(options.challenge),
      allowCredentials: (options.allowCredentials ?? []).map((c) => ({
        id: toB64url(c.id),
        type: c.type,
      })),
      userVerification: options.userVerification,
      timeout: options.timeout,
      mediation,
      extensions: extensions(options.extensions),
    };
  }

  /** A credential the page can't tell from the browser's own: the right prototypes, the methods sites call. */
  function credentialOf(answer: PasskeyCredential): PublicKeyCredential {
    const clientDataJSON = fromB64url(answer.clientDataJSON);
    const authenticatorData = fromB64url(answer.authenticatorData);
    let response: AuthenticatorResponse;
    if (answer.attestationObject) {
      const attestation = {
        clientDataJSON,
        attestationObject: fromB64url(answer.attestationObject),
        getAuthenticatorData: () => authenticatorData,
        getPublicKey: () => (answer.publicKey ? fromB64url(answer.publicKey) : null),
        getPublicKeyAlgorithm: () => answer.publicKeyAlgorithm ?? -7,
        getTransports: () => answer.transports ?? ['internal', 'hybrid'],
      };
      Object.setPrototypeOf(attestation, AuthenticatorAttestationResponse.prototype);
      response = attestation as unknown as AuthenticatorResponse;
    } else {
      const assertion = {
        clientDataJSON,
        authenticatorData,
        signature: fromB64url(answer.signature ?? ''),
        userHandle: answer.userHandle ? fromB64url(answer.userHandle) : null,
      };
      Object.setPrototypeOf(assertion, AuthenticatorAssertionResponse.prototype);
      response = assertion as unknown as AuthenticatorResponse;
    }
    const json = () => ({
      id: answer.id,
      rawId: answer.rawId,
      type: answer.type,
      authenticatorAttachment: answer.authenticatorAttachment,
      clientExtensionResults: answer.clientExtensionResults,
      response: answer.attestationObject
        ? {
            clientDataJSON: answer.clientDataJSON,
            attestationObject: answer.attestationObject,
            authenticatorData: answer.authenticatorData,
            publicKey: answer.publicKey,
            publicKeyAlgorithm: answer.publicKeyAlgorithm,
            transports: answer.transports,
          }
        : {
            clientDataJSON: answer.clientDataJSON,
            authenticatorData: answer.authenticatorData,
            signature: answer.signature,
            userHandle: answer.userHandle ?? undefined,
          },
    });
    const credential = {
      id: answer.id,
      rawId: fromB64url(answer.rawId),
      type: answer.type,
      authenticatorAttachment: answer.authenticatorAttachment,
      response,
      getClientExtensionResults: () => answer.clientExtensionResults,
      toJSON: json,
    };
    Object.setPrototypeOf(credential, PublicKeyCredential.prototype);
    return credential as unknown as PublicKeyCredential;
  }

  function settle(
    answer: PasskeyAnswer,
    fallback: () => Promise<Credential | null>,
  ): Promise<Credential | null> {
    if (answer.kind === 'credential') return Promise.resolve(credentialOf(answer.credential));
    if (answer.kind === 'error')
      return Promise.reject(new DOMException(answer.message, answer.name));
    return fallback();
  }

  credentials.create = async function create(options?: CredentialCreationOptions) {
    if (!options?.publicKey) return originalCreate(options);
    let serialized: unknown;
    try {
      serialized = serializeCreate(options.publicKey);
    } catch {
      return originalCreate(options);
    }
    return settle(await ask('create', serialized, options.signal), () => originalCreate(options));
  };

  credentials.get = async function get(options?: CredentialRequestOptions) {
    if (!options?.publicKey) return originalGet(options);
    let serialized: unknown;
    try {
      serialized = serializeGet(options.publicKey, options.mediation);
    } catch {
      return originalGet(options);
    }
    return settle(await ask('get', serialized, options.signal), () => originalGet(options));
  };
})();
