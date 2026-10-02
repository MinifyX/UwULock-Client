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
 *
 * Which origin asks is the document's, not its address's (R4-2, `frameDocument`): a sandboxed
 * document on the site's address has an opaque origin and gets nothing. A frame may ask only
 * when it and every frame above it are of one origin, which `location.ancestorOrigins` shows;
 * a browser without it (Firefox) only serves top frames. So `crossOrigin` is always false.
 *
 * One window per tab at a time; after the person cancelled, that tab and origin get the
 * browser's authenticator for a while (R4-5).
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
import { hostnameOf } from '../shared/uri';
import { passkeyLoginName } from '../prompt/names';
import { frameDocument, matchingLogins, sameOriginAncestors } from './autofill';
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
  tabId: number;
  windowId: number | null;
  timer: ReturnType<typeof setTimeout>;
  resolve: (answer: PasskeyAnswer) => void;
};

const waiting = new Map<string, Waiting>();
/** Page request id (from the content script) → prompt id, for aborts. */
const byPageRequest = new Map<string, string>();

const FALLBACK: PasskeyAnswer = { kind: 'fallback' };

/** After a cancel: how long that tab and origin get the browser's authenticator instead. */
export const CANCEL_BACKOFF = 10_000;
/** `tabId|origin` → until when. */
const backoff = new Map<string, number>();

function backoffKey(tabId: number, origin: string) {
  return `${tabId}|${origin}`;
}

function backingOff(tabId: number, origin: string): boolean {
  const key = backoffKey(tabId, origin);
  const until = backoff.get(key);
  if (until === undefined) return false;
  if (Date.now() < until) return true;
  backoff.delete(key);
  return false;
}

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

type Frame = { origin: string; url: string; tabId: number };

/**
 * The frame may ask: a top frame whose document's origin is its address's, or a frame of that
 * same origin with every ancestor of it too — known from `location.ancestorOrigins` only, so
 * not on browsers without it.
 */
export async function frameOrigin(sender: Sender): Promise<Frame | null> {
  const tabId = sender.tab?.id;
  if (tabId === undefined) return null;
  const frame = await frameDocument(sender);
  if (!frame || !sameOriginAncestors(sender, frame, { walked: false })) return null;
  return { origin: frame.origin, url: frame.url, tabId };
}

function vaultState(): Promise<VaultState> {
  return session.vaultState();
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
  const frame = await frameOrigin(sender);
  if (!frame) return FALLBACK;
  if (backingOff(frame.tabId, frame.origin)) return FALLBACK;
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
  const frame = await frameOrigin(sender);
  if (!frame) return FALLBACK;
  if (backingOff(frame.tabId, frame.origin)) return FALLBACK;
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
  frame: Frame,
  rpId: string,
  timeout: number | undefined,
): Promise<PasskeyAnswer> {
  // One window per tab: N frames of a page don't get N windows.
  for (const other of waiting.values()) {
    if (other.tabId === frame.tabId) {
      return Promise.resolve(error('NotAllowedError', 'Another request is running.'));
    }
  }
  const id = crypto.randomUUID();
  const limit = Math.min(Math.max(timeout ?? 300_000, 30_000), 600_000);
  return new Promise<PasskeyAnswer>((resolve) => {
    const entry: Waiting = {
      id,
      request,
      origin: frame.origin,
      rpId,
      url: frame.url,
      tabId: frame.tabId,
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

/** Cancelled: the browser's authenticator gets its turn, and this tab and origin a pause. */
function cancel(entry: Waiting) {
  backoff.set(backoffKey(entry.tabId, entry.origin), Date.now() + CANCEL_BACKOFF);
  finish(entry, FALLBACK);
}

// Closing the window is cancelling.
ext.windows.onRemoved.addListener((windowId) => {
  for (const entry of waiting.values()) {
    if (entry.windowId === windowId) {
      entry.windowId = null;
      cancel(entry);
    }
  }
});

ext.tabs.onRemoved.addListener((tabId) => {
  for (const key of backoff.keys()) if (key.startsWith(`${tabId}|`)) backoff.delete(key);
});

/**
 * Whether a login may take a passkey for `rpId`: one of its own addresses is on that very host
 * or under it — not a login that matches only through its base domain or equivalent domains
 * (R4-3).
 */
export function loginFitsRpId(entry: { uris?: { uri: string }[] | null }, rpId: string): boolean {
  const wanted = rpId.toLowerCase();
  return (entry.uris ?? []).some(({ uri }) => {
    const host = hostnameOf(uri)?.toLowerCase();
    return Boolean(host) && (host === wanted || host!.endsWith(`.${wanted}`));
  });
}

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
        ? (await matchingLogins(entry.url, { topFrame: false, strict: true }))
            .filter((e) => loginFitsRpId(e, entry.rpId))
            .map((e): PageItem & { hasPasskey: boolean } => ({
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
    cancel(entry);
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
          name: passkeyLoginName(entry.rpId, options.rp?.name),
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
