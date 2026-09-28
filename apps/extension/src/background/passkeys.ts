/**
 * UwULock as the page's passkey provider.
 *
 * The script in the page (page/webauthn.ts) hands `navigator.credentials.create()` and `get()`
 * to the content script, which asks here. The background checks the relying party against the
 * frame's own origin (WebAuthn's rules, shared/rpid.ts), opens a small window of its own to ask
 * the person — create a passkey in the vault, or sign in with one — and answers with the
 * credential. Passkeys are kept in Bitwarden's format (`login.fido2Credentials`), so the ones
 * Bitwarden's apps made work here and the other way round.
 *
 * Whenever UwULock can't or shouldn't answer — logged out, cancelled, nothing for the site, a
 * cross-origin frame, an algorithm other than ES256, a conditional (autofill) request — the
 * page falls back to the browser's own authenticator.
 */

import { ext } from '../shared/browser';
import type {
  PageItem,
  PasskeyAnswer,
  PasskeyCreateOptions,
  PasskeyCredential,
  PasskeyDecision,
  PasskeyGetOptions,
  PasskeyPrompt,
  VaultState,
} from '../shared/protocol';
import { checkRpId } from '../shared/rpid';
import { matchingLogins, tabUrl } from './autofill';
import * as session from './session';
import { settings } from './settings';
import * as vault from './vault';
import { callJson } from './wasm';

type Sender = chrome.runtime.MessageSender;

type Request =
  { kind: 'create'; options: PasskeyCreateOptions } | { kind: 'get'; options: PasskeyGetOptions };

type Waiting = {
  id: string;
  request: Request;
  origin: string;
  rpId: string;
  url: string;
  windowId: number | null;
  timer: ReturnType<typeof setTimeout>;
  resolve: (answer: PasskeyAnswer) => void;
};

const waiting = new Map<string, Waiting>();
/** Page request id (from the content script) → prompt id, for aborts. */
const byPageRequest = new Map<string, string>();

const FALLBACK: PasskeyAnswer = { kind: 'fallback' };

function error(
  name: Extract<PasskeyAnswer, { kind: 'error' }>['name'],
  message: string,
): PasskeyAnswer {
  return { kind: 'error', name, message };
}

function b64url(bytes: Uint8Array): string {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function normalizeB64url(text: string): string {
  return text.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function verification(value: string | undefined): 'required' | 'preferred' | 'discouraged' {
  return value === 'required' || value === 'discouraged' ? value : 'preferred';
}

/** The frame may ask: a top frame, or a frame of the same origin as its page. */
function frameOrigin(sender: Sender): { origin: string; url: string } | null {
  const url = sender.url;
  const tabId = sender.tab?.id;
  if (!url || tabId === undefined) return null;
  let origin: string;
  try {
    origin = new URL(url).origin;
  } catch {
    return null;
  }
  if (sender.frameId !== 0) {
    const top = tabUrl(tabId) ?? sender.tab?.url;
    if (!top || new URL(top).origin !== origin) return null;
  }
  return { origin, url };
}

async function vaultState(): Promise<VaultState> {
  return (await session.status()).state;
}

function clientData(type: 'webauthn.create' | 'webauthn.get', challenge: string, origin: string) {
  const json = JSON.stringify({
    type,
    challenge: normalizeB64url(challenge),
    origin,
    crossOrigin: false,
  });
  return new TextEncoder().encode(json);
}

async function sha256(bytes: Uint8Array): Promise<Uint8Array> {
  return new Uint8Array(await crypto.subtle.digest('SHA-256', bytes as BufferSource));
}

// ── The page asks ─────────────────────────────────────────

export async function create(
  sender: Sender,
  pageRequestId: string,
  options: PasskeyCreateOptions,
): Promise<PasskeyAnswer> {
  if (!(await settings()).passkeys) return FALLBACK;
  const frame = frameOrigin(sender);
  if (!frame) return FALLBACK;
  const rp = checkRpId(frame.origin, options.rp?.id);
  if (!rp.ok) return error('SecurityError', `The relying party is not this site (${rp.reason}).`);
  // Only ES256; a site that doesn't take it gets the browser's authenticator.
  if (!options.pubKeyCredParams?.some((p) => p.type === 'public-key' && p.alg === -7))
    return FALLBACK;
  if ((await vaultState()) === 'logged-out') return FALLBACK;
  return ask(pageRequestId, { kind: 'create', options }, frame, rp.rpId, options.timeout);
}

export async function get(
  sender: Sender,
  pageRequestId: string,
  options: PasskeyGetOptions,
): Promise<PasskeyAnswer> {
  if (!(await settings()).passkeys) return FALLBACK;
  // The browser's autofill of passkeys stays the browser's.
  if (options.mediation === 'conditional') return FALLBACK;
  const frame = frameOrigin(sender);
  if (!frame) return FALLBACK;
  const rp = checkRpId(frame.origin, options.rpId);
  if (!rp.ok) return error('SecurityError', `The relying party is not this site (${rp.reason}).`);
  const state = await vaultState();
  if (state === 'logged-out') return FALLBACK;
  // Unlocked and nothing for this site: no window at all.
  if (state === 'unlocked' && getChoices(rp.rpId, options).length === 0) return FALLBACK;
  return ask(pageRequestId, { kind: 'get', options }, frame, rp.rpId, options.timeout);
}

export function abort(pageRequestId: string) {
  const id = byPageRequest.get(pageRequestId);
  const found = id ? waiting.get(id) : undefined;
  if (found) finish(found, error('AbortError', 'The page aborted the request.'));
}

function ask(
  pageRequestId: string,
  request: Request,
  frame: { origin: string; url: string },
  rpId: string,
  timeout: number | undefined,
): Promise<PasskeyAnswer> {
  const id = crypto.randomUUID();
  const limit = Math.min(Math.max(timeout ?? 300_000, 30_000), 600_000);
  return new Promise<PasskeyAnswer>((resolve) => {
    const entry: Waiting = {
      id,
      request,
      origin: frame.origin,
      rpId,
      url: frame.url,
      windowId: null,
      timer: setTimeout(() => finish(entry, error('NotAllowedError', 'Timed out.')), limit),
      resolve,
    };
    waiting.set(id, entry);
    byPageRequest.set(pageRequestId, id);
    void (async () => {
      const window = await ext.windows
        .create({
          url: ext.runtime.getURL(`prompt.html?id=${encodeURIComponent(id)}`),
          type: 'popup',
          width: 400,
          height: 560,
          focused: true,
        })
        .catch(() => null);
      if (!window?.id) {
        finish(entry, FALLBACK);
        return;
      }
      entry.windowId = window.id;
    })();
  });
}

function finish(entry: Waiting, answer: PasskeyAnswer) {
  if (!waiting.has(entry.id)) return;
  waiting.delete(entry.id);
  for (const [page, id] of byPageRequest) if (id === entry.id) byPageRequest.delete(page);
  clearTimeout(entry.timer);
  if (entry.windowId !== null) void ext.windows.remove(entry.windowId).catch(() => undefined);
  entry.resolve(answer);
}

// Closing the window is cancelling: the browser's authenticator gets its turn.
ext.windows.onRemoved.addListener((windowId) => {
  for (const entry of waiting.values()) {
    if (entry.windowId === windowId) {
      entry.windowId = null;
      finish(entry, FALLBACK);
    }
  }
});

// ── The window asks ───────────────────────────────────────

function getChoices(rpId: string, options: PasskeyGetOptions) {
  const allowed = new Set((options.allowCredentials ?? []).map((c) => normalizeB64url(c.id)));
  const out: Extract<PasskeyPrompt, { kind: 'get' }>['choices'] = [];
  for (const entry of vault.autofillIndex()) {
    for (const passkey of entry.passkeys ?? []) {
      if (passkey.rpId !== rpId) continue;
      if (allowed.size > 0 ? !allowed.has(passkey.credentialId) : !passkey.discoverable) continue;
      out.push({
        itemId: entry.id,
        credentialId: passkey.credentialId,
        name: entry.name,
        userName: passkey.userName ?? passkey.userDisplayName,
        reprompt: entry.reprompt,
      });
    }
  }
  return out;
}

export async function prompt(id: string): Promise<PasskeyPrompt> {
  const entry = waiting.get(id);
  if (!entry) throw { kind: 'expired', message: 'This request is over.' };
  const state = await vaultState();
  if (entry.request.kind === 'create') {
    const options = entry.request.options;
    const excludedIds = new Set(
      (options.excludeCredentials ?? []).map((c) => normalizeB64url(c.id)),
    );
    const excluded = vault
      .autofillIndex()
      .some((e) =>
        (e.passkeys ?? []).some((p) => p.rpId === entry.rpId && excludedIds.has(p.credentialId)),
      );
    const candidates =
      state === 'unlocked'
        ? (await matchingLogins(entry.url)).map((e): PageItem & { hasPasskey: boolean } => ({
            id: e.id,
            kind: e.kind,
            name: e.name,
            subtitle: e.subtitle,
            favorite: e.favorite,
            hasTotp: e.hasTotp,
            reprompt: e.reprompt,
            hasPasskey: (e.passkeys ?? []).length > 0,
          }))
        : [];
    return {
      id,
      kind: 'create',
      origin: entry.origin,
      rpId: entry.rpId,
      rpName: options.rp?.name || entry.rpId,
      userName: options.user?.name || options.user?.displayName || '',
      candidates,
      excluded,
      userVerification: verification(options.authenticatorSelection?.userVerification),
      state,
    };
  }
  const options = entry.request.options;
  return {
    id,
    kind: 'get',
    origin: entry.origin,
    rpId: entry.rpId,
    choices: state === 'unlocked' ? getChoices(entry.rpId, options) : [],
    userVerification: verification(options.userVerification),
    state,
  };
}

type Created = {
  credentialId: string;
  attestationObject: string;
  authenticatorData: string;
  publicKey: string;
  publicKeyAlgorithm: number;
  transports: string[];
  cipher: Record<string, unknown>;
  itemId: string | null;
};

type Asserted = {
  credentialId: string;
  authenticatorData: string;
  signature: string;
  userHandle: string | null;
  cipher: Record<string, unknown> | null;
};

/** The master password, when the site or the item asks for it. Throws when it's wrong. */
async function verify(password: string | null, needed: boolean): Promise<boolean> {
  if (password) {
    await vault.checkPassword(password);
    return true;
  }
  if (needed) throw { kind: 'verify', message: 'Enter your master password.' };
  return false;
}

export async function decide(decision: PasskeyDecision): Promise<void> {
  const entry = waiting.get(decision.id);
  if (!entry) throw { kind: 'expired', message: 'This request is over.' };
  if (decision.choice === 'cancel' || decision.choice === 'browser') {
    finish(entry, FALLBACK);
    return;
  }
  await session.requireUnlocked();
  const current = await prompt(decision.id);

  if (
    decision.choice === 'create' &&
    entry.request.kind === 'create' &&
    current.kind === 'create'
  ) {
    if (current.excluded) {
      finish(
        entry,
        error('InvalidStateError', 'A passkey for this account is in the vault already.'),
      );
      return;
    }
    const target = decision.itemId
      ? current.candidates.find((c) => c.id === decision.itemId)
      : null;
    if (decision.itemId && !target) throw { kind: 'not-found', message: 'This login is gone.' };
    const verified = await verify(
      decision.password,
      current.userVerification === 'required' || Boolean(target?.reprompt),
    );
    if (target?.reprompt) await vault.verifyReprompt(target.id, decision.password ?? '');
    const options = entry.request.options;
    const created = await callJson<Created>((core) =>
      core.passkeyCreate(
        JSON.stringify({
          itemId: target?.id ?? null,
          name: options.rp?.name || entry.rpId,
          folderId: null,
          rpId: entry.rpId,
          rpName: options.rp?.name ?? null,
          userHandle: options.user?.id ? normalizeB64url(options.user.id) : null,
          userName: options.user?.name ?? null,
          userDisplayName: options.user?.displayName ?? null,
          discoverable:
            options.authenticatorSelection?.residentKey !== 'discouraged' ||
            Boolean(options.authenticatorSelection?.requireResidentKey),
          userVerified: verified,
          now: new Date().toISOString(),
        }),
      ),
    );
    // Saved first: a passkey the site knows but the vault lost would lock somebody out.
    try {
      await vault.putCipher(target?.id ?? null, created.cipher);
    } catch (failure) {
      finish(entry, error('NotAllowedError', 'The passkey could not be saved.'));
      throw failure;
    }
    const data = clientData('webauthn.create', options.challenge, entry.origin);
    const credential: PasskeyCredential = {
      id: created.credentialId,
      rawId: created.credentialId,
      type: 'public-key',
      authenticatorAttachment: 'platform',
      clientDataJSON: b64url(data),
      attestationObject: created.attestationObject,
      authenticatorData: created.authenticatorData,
      publicKey: created.publicKey,
      publicKeyAlgorithm: created.publicKeyAlgorithm,
      transports: created.transports,
      clientExtensionResults: options.extensions?.credProps ? { credProps: { rk: true } } : {},
    };
    finish(entry, { kind: 'credential', credential });
    return;
  }

  if (decision.choice === 'use' && entry.request.kind === 'get' && current.kind === 'get') {
    const choice = current.choices.find(
      (c) => c.itemId === decision.itemId && c.credentialId === decision.credentialId,
    );
    if (!choice) throw { kind: 'not-found', message: 'This passkey is gone.' };
    const verified = await verify(
      decision.password,
      current.userVerification === 'required' || choice.reprompt,
    );
    if (choice.reprompt) await vault.verifyReprompt(choice.itemId, decision.password ?? '');
    const options = entry.request.options;
    const data = clientData('webauthn.get', options.challenge, entry.origin);
    const hash = b64url(await sha256(data));
    const asserted = await callJson<Asserted>((core) =>
      core.passkeyAssert(
        JSON.stringify({
          itemId: choice.itemId,
          credentialId: choice.credentialId,
          rpId: entry.rpId,
          clientDataHash: hash,
          userVerified: verified,
        }),
      ),
    );
    // A passkey that counts its uses (from elsewhere; UwULock's own stay at 0) is saved again.
    if (asserted.cipher)
      await vault.putCipher(choice.itemId, asserted.cipher).catch(() => undefined);
    const credential: PasskeyCredential = {
      id: asserted.credentialId,
      rawId: asserted.credentialId,
      type: 'public-key',
      authenticatorAttachment: 'platform',
      clientDataJSON: b64url(data),
      authenticatorData: asserted.authenticatorData,
      signature: asserted.signature,
      userHandle: asserted.userHandle,
      clientExtensionResults: {},
    };
    finish(entry, { kind: 'credential', credential });
    return;
  }
  throw { kind: 'invalid', message: 'That does not answer this request.' };
}
