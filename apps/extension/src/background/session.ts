/**
 * Accounts: logging in (with two-step login and Bitwarden's check of new devices), unlocking
 * with the master password or a PIN, locking, logging out, switching.
 *
 * The master password goes into the WebAssembly module and nowhere else; the server gets its
 * hash once, at login. The user key lives in the module's memory while the vault is open, and
 * in `storage.session` so a service worker that was ended can open the vault again when it
 * wakes (see store.ts). Several accounts can be logged in; one is open at a time.
 */

import { ext } from '../shared/browser';
import type {
  LoginStep,
  PendingLoginInfo,
  ServerChoice,
  Status,
  TwoFactorMethod,
} from '../shared/protocol';
import { changed } from './events';
import { anonymous, ApiError, claims, failure, request, whenSessionEnds } from './http';
import { deviceType, endpoints, normalizeServerUrl } from './server';
import {
  type Account,
  account,
  accountKey,
  accounts,
  activeAccount,
  cachedSync,
  deviceId,
  forgetKdfFloor,
  kdfFloor,
  local,
  migrateAccounts,
  pinAttempts,
  removeAccount,
  removeSession,
  saveAccount,
  session,
  setLocal,
  setKdfFloor,
  setPinAttempts,
  setSession,
  updateAccount,
} from './store';
import { uwuInfo } from './uwu';
import * as vault from './vault';
import { call } from './wasm';

// ── What is open ──────────────────────────────────────────

/** The account whose vault is open in the module, if any. */
let unlockedId: string | null = null;
let expiredNotice: string | null = null;

export function unlockedAccountId(): string | null {
  return unlockedId;
}

/**
 * After the background was started again: open the vault from `storage.session` and the cached
 * sync, if it was open before. Every handler waits for this first.
 */
export const restored: Promise<void> = (async () => {
  try {
    await migrateAccounts();
    const saved = await session('unlocked');
    if (!saved) return;
    const found = await account(saved.accountId);
    if (!found?.protectedKey || (await local('activeAccount')) !== found.id) {
      await removeSession('unlocked');
      return;
    }
    await call((core) =>
      core.unlockWithKey(found.email, found.kdf, found.protectedKey!, saved.userKey),
    );
    unlockedId = found.id;
    const text = await cachedSync(found.id);
    if (text) await vault.open(found, text);
  } catch {
    // Whatever went wrong, the vault is locked now; the master password opens it again.
    await call((core) => core.lock()).catch(() => undefined);
    await removeSession('unlocked').catch(() => undefined);
    unlockedId = null;
  }
})();

async function remember(found: Account) {
  const userKey = await call((core) => core.userKey());
  await setSession('unlocked', { accountId: found.id, userKey });
  await setSession('lastActive', Date.now());
  unlockedId = found.id;
}

/** The vault is open for `found`: remember the key, load what we have, fetch what's new. */
async function opened(found: Account) {
  await remember(found);
  await setPinAttempts(found.id, 0);
  const text = await cachedSync(found.id);
  if (text) await vault.open(found, text).catch(() => undefined);
  changed();
  // The sync after an unlock runs on; the popup shows the cached vault meanwhile.
  void vault.sync(found).catch(() => undefined);
}

export async function lock(): Promise<void> {
  await call((core) => core.lock()).catch(() => undefined);
  await removeSession('unlocked');
  unlockedId = null;
  vault.closed();
  changed();
}

// ── Status ────────────────────────────────────────────────

/** Logged out, locked or unlocked — without the rest of the status (and without using up its notices). */
export async function vaultState(): Promise<Status['state']> {
  const active = await local('activeAccount');
  if (!active || !(await account(active))) return 'logged-out';
  return unlockedId === active ? 'unlocked' : 'locked';
}

export async function status(): Promise<Status> {
  const list = await accounts();
  const active = await activeAccount();
  const pending = await pendingLogin();
  const pinSession = await session('pin');
  const sync = vault.syncState();
  const state: Status['state'] = !active
    ? 'logged-out'
    : unlockedId === active.id
      ? 'unlocked'
      : 'locked';
  const ends = active ? endpoints(active.server) : null;
  const notice = expiredNotice;
  expiredNotice = null;
  return {
    state,
    accountId: active?.id ?? null,
    email: active?.email ?? null,
    name: active?.name ?? null,
    server: ends?.label ?? null,
    serverKind: active?.server.kind ?? null,
    serverUrl: active?.server.kind === 'self-hosted' ? (active.server.url ?? null) : null,
    webVault: ends?.web ?? null,
    lastSync: active?.lastSync ?? null,
    syncing: sync.syncing,
    syncError: sync.error,
    sessionExpired: notice !== null,
    pinSet: Boolean(
      active && (active.pinProtected || (pinSession && pinSession.accountId === active.id)),
    ),
    accounts: list.map((a) => ({
      id: a.id,
      email: a.email,
      name: a.name,
      server: endpoints(a.server).label,
      serverKind: a.server.kind,
      active: a.id === active?.id,
      unlocked: a.id === unlockedId,
    })),
    uwu: active?.uwu ?? null,
    pendingSaves: ((await session('pendingSaves')) ?? []).length,
    login: pending
      ? {
          email: pending.email,
          server: endpoints(pending.server).label,
          step: pending.step,
          methods: pending.methods,
          message: pending.message,
        }
      : null,
  };
}

// ── Logging in ────────────────────────────────────────────

type PendingLogin = Omit<PendingLoginInfo, 'server'> & {
  server: ServerChoice;
  kdf: string;
  hash: string;
  /** Provider 7's options from the server, for the security key page. */
  webauthn: Record<string, unknown> | null;
  webauthnRemember: boolean;
};

async function pendingLogin(): Promise<PendingLogin | null> {
  return ((await session('pendingLogin')) as PendingLogin | null | undefined) ?? null;
}

const TWO_FACTOR_KINDS: Record<number, [TwoFactorMethod['kind'], boolean]> = {
  0: ['authenticator', true],
  1: ['email', true],
  2: ['duo', false],
  3: ['yubikey', true],
  4: ['u2f', false],
  6: ['duo', false],
  7: ['webauthn', true],
};

function lowerKeys(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object') return {};
  return Object.fromEntries(Object.entries(value).map(([k, v]) => [k.toLowerCase(), v]));
}

/** How the master key is derived for this address. */
async function prelogin(server: ServerChoice, email: string): Promise<string> {
  const ends = endpoints(server);
  let body: Record<string, unknown> | null = null;
  // Bitwarden answers on the identity server; older Vaultwardens only on the API.
  for (const base of [ends.identity, ends.api]) {
    try {
      body = lowerKeys(await anonymous(`${base}/accounts/prelogin`, { body: { email } }));
      break;
    } catch (error) {
      if (error instanceof ApiError && (error.status === 404 || error.status === 405)) continue;
      throw failure(error);
    }
  }
  if (!body) throw { kind: 'server', message: 'The server does not answer the prelogin.' };
  return JSON.stringify({
    kdf: Number(body.kdf ?? 0),
    kdfIterations: body.kdfiterations ?? null,
    kdfMemory: body.kdfmemory ?? null,
    kdfParallelism: body.kdfparallelism ?? null,
  });
}

/** The account this browser has for the address on this server, if any. */
async function knownAccount(server: ServerChoice, email: string): Promise<Account | undefined> {
  const identity = endpoints(server).identity;
  return (await accounts()).find(
    (a) => a.email === email && endpoints(a.server).identity === identity,
  );
}

/**
 * Logging in again to an account this browser knows: the server doesn't get to ask for a
 * cheaper key derivation than the last login accepted, or the hash sent next would be that
 * much easier to guess the master password from (the desktop app's rule). What was accepted
 * stays when the server ends the session, so a hostile server can't clear it by logging this
 * browser out first. Whoever lowered it on purpose logs out here, or forgets it in the popup.
 */
async function refuseWeakerKdf(server: ServerChoice, email: string, kdf: string): Promise<void> {
  const floors = [
    await kdfFloor(endpoints(server).identity, email),
    (await knownAccount(server, email))?.kdf ?? null,
  ];
  for (const floor of floors) {
    if (floor && (await call((core) => core.kdfIsWeakerThan(kdf, floor)))) {
      throw {
        kind: 'weaker-kdf',
        message: "The server asks for a weaker key derivation than this account's last login used.",
      };
    }
  }
}

/** The popup's way out after a refused weaker KDF that the user lowered on purpose. */
export async function forgetKdf(server: ServerChoice, email: string): Promise<void> {
  const choice: ServerChoice =
    server.kind === 'self-hosted'
      ? { kind: 'self-hosted', url: normalizeServerUrl(server.url ?? '') }
      : { kind: server.kind };
  const address = email.trim().toLowerCase();
  if (await knownAccount(choice, address)) {
    throw { kind: 'invalid', message: 'Log this account out first.' };
  }
  await forgetKdfFloor(endpoints(choice).identity, address);
}

/** Whether the popup got the host permission for this server (it asks when the button is clicked). */
async function permitted(server: ServerChoice): Promise<boolean> {
  return ext.permissions.contains({ origins: endpoints(server).origins }).catch(() => false);
}

function base64Url(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

export async function login(
  server: ServerChoice,
  email: string,
  password: string,
): Promise<LoginStep> {
  const choice: ServerChoice =
    server.kind === 'self-hosted'
      ? { kind: 'self-hosted', url: normalizeServerUrl(server.url ?? '') }
      : { kind: server.kind };
  if (!(await permitted(choice))) {
    throw { kind: 'permission', message: 'UwULock may not reach this server yet.' };
  }
  const address = email.trim().toLowerCase();
  if (!address.includes('@')) throw { kind: 'invalid', message: 'This is not an email address.' };
  const kdf = await prelogin(choice, address);
  await refuseWeakerKdf(choice, address, kdf);
  const hash = await call((core) => core.deriveLogin(address, password, kdf));
  const pending: PendingLogin = {
    email: address,
    server: choice,
    kdf,
    hash,
    step: 'two-factor',
    methods: [],
    message: null,
    webauthn: null,
    webauthnRemember: false,
  };
  await setSession('pendingLogin', pending);
  return token(pending, {});
}

async function token(pending: PendingLogin, extra: Record<string, string>): Promise<LoginStep> {
  const device = deviceType();
  const known = await knownAccount(pending.server, pending.email);
  const form = new URLSearchParams({
    grant_type: 'password',
    username: pending.email,
    password: pending.hash,
    scope: 'api offline_access',
    client_id: 'browser',
    deviceType: String(device.kind),
    deviceIdentifier: await deviceId(),
    deviceName: device.name,
    ...extra,
  });
  if (!extra.twoFactorToken && known?.rememberToken) {
    form.set('twoFactorToken', known.rememberToken);
    form.set('twoFactorProvider', '5');
    form.set('twoFactorRemember', '0');
  }
  let body: Record<string, unknown>;
  try {
    body = await anonymous<Record<string, unknown>>(
      `${endpoints(pending.server).identity}/connect/token`,
      { form, extraHeaders: { 'Auth-Email': base64Url(pending.email) } },
    );
  } catch (error) {
    if (!(error instanceof ApiError)) throw failure(error);
    const refusal = lowerKeys(error.body);
    const providers = refusal.twofactorproviders2 as Record<string, unknown> | undefined;
    if (providers && typeof providers === 'object') {
      const methods: TwoFactorMethod[] = Object.entries(providers)
        .map(([key, details]) => {
          const provider = Number(key);
          const [kind, supported] = TWO_FACTOR_KINDS[provider] ?? ['other', false];
          const lowered = lowerKeys(details);
          return {
            provider,
            kind,
            supported,
            hint: typeof lowered.email === 'string' ? lowered.email : null,
          };
        })
        .filter((m) => m.provider !== 5)
        .sort((a, b) => Number(b.supported) - Number(a.supported) || a.provider - b.provider);
      const webauthn = providers['7'];
      const next: PendingLogin = {
        ...pending,
        step: 'two-factor',
        methods,
        message: extra.twoFactorToken ? error.message : null,
        webauthn:
          webauthn && typeof webauthn === 'object' ? (webauthn as Record<string, unknown>) : null,
      };
      await setSession('pendingLogin', next);
      return { step: 'two-factor', methods, message: next.message };
    }
    const description = String(refusal.error_description ?? '').toLowerCase();
    if (description.includes('new device') || description.includes('device verification')) {
      await setSession('pendingLogin', { ...pending, step: 'new-device', message: null });
      return { step: 'new-device' };
    }
    if (error.status === 400 || error.status === 401) {
      throw {
        kind: 'refused',
        message:
          extra.newDeviceOtp || extra.twoFactorToken
            ? error.message
            : 'Email or master password is wrong.',
      };
    }
    throw failure(error);
  }
  return loggedIn(pending, body);
}

async function loggedIn(pending: PendingLogin, body: Record<string, unknown>): Promise<LoginStep> {
  const lowered = lowerKeys(body);
  const accessToken = String(lowered.access_token);
  const who = claims(accessToken);
  const userId = String(who.sub ?? pending.email);
  // Keyed by the server too: what another server's account with the same user id keeps (its
  // protected user key, its remember-me token) is never taken over.
  const id = accountKey(endpoints(pending.server).identity, userId);
  const previous = await account(id);
  const next: Account = {
    id,
    userId,
    email: pending.email,
    name: typeof who.name === 'string' ? who.name : (previous?.name ?? null),
    server: pending.server,
    kdf: pending.kdf,
    protectedKey: typeof lowered.key === 'string' ? lowered.key : (previous?.protectedKey ?? null),
    accessToken,
    refreshToken: String(lowered.refresh_token ?? ''),
    expiresAt:
      Date.now() + Math.min(Math.max(Number(lowered.expires_in ?? 3600), 60), 86_400) * 1000,
    lastSync: previous?.lastSync ?? null,
    rememberToken:
      typeof lowered.twofactortoken === 'string'
        ? lowered.twofactortoken
        : (previous?.rememberToken ?? null),
    pinProtected: null,
    uwu: null,
  };
  // Another account may be open: it is locked, this one takes over.
  if (unlockedId && unlockedId !== id) await lock();
  await saveAccount(next);
  await setKdfFloor(endpoints(next.server).identity, next.email, next.kdf);
  await setLocal('activeAccount', id);
  await removeSession('pendingLogin');
  next.uwu = (await uwuInfo(next)) ?? null;
  await saveAccount(next);
  // The master key from the first step is still in the module — unless the background was
  // ended while somebody fetched their code. Then the vault stays locked until the password.
  if (next.protectedKey) {
    try {
      await call((core) => core.unlock(next.email, next.kdf, next.protectedKey!));
      await remember(next);
      await vault.sync(next);
    } catch {
      await call((core) => core.lock()).catch(() => undefined);
      unlockedId = null;
    }
  }
  changed();
  return { step: 'done', status: await status() };
}

async function requirePending(): Promise<PendingLogin> {
  const pending = await pendingLogin();
  if (!pending) throw { kind: 'expired', message: 'Log in again.' };
  return pending;
}

export async function loginTwoFactor(provider: number, code: string, remember: boolean) {
  const pending = await requirePending();
  return token(pending, {
    twoFactorProvider: String(provider),
    twoFactorToken: code.trim().replace(/\s/g, ''),
    twoFactorRemember: remember ? '1' : '0',
  });
}

export async function loginNewDevice(code: string) {
  const pending = await requirePending();
  return token(pending, { newDeviceOtp: code.trim().replace(/\s/g, '') });
}

/** A new code by email: for two-step login, or for Bitwarden's check of a new device. */
export async function loginSendEmail() {
  const pending = await requirePending();
  const ends = endpoints(pending.server);
  try {
    if (pending.step === 'new-device') {
      await anonymous(`${ends.api}/accounts/resend-new-device-otp`, {
        body: { email: pending.email, masterPasswordHash: pending.hash },
      });
    } else {
      await anonymous(`${ends.api}/two-factor/send-email-login`, {
        body: {
          email: pending.email,
          masterPasswordHash: pending.hash,
          deviceIdentifier: await deviceId(),
        },
      });
    }
  } catch (error) {
    throw failure(error);
  }
}

export async function loginCancel() {
  await removeSession('pendingLogin');
  if (!unlockedId) await call((core) => core.lock()).catch(() => undefined);
  changed();
}

/**
 * The second step with a security key: the page the server keeps for it (Bitwarden's WebAuthn
 * fallback connector) opens in a tab, and the content script there sends the answer back.
 */
export async function loginWebAuthn(remember: boolean) {
  const pending = await requirePending();
  if (!pending.webauthn) throw { kind: 'unsupported', message: 'The server sent no challenge.' };
  await setSession('pendingLogin', { ...pending, webauthnRemember: remember });
  const data = base64Url(
    JSON.stringify({ data: JSON.stringify(pending.webauthn), btnText: 'UwULock' }),
  )
    .replace(/-/g, '+')
    .replace(/_/g, '/');
  const language = (navigator.language ?? 'en').slice(0, 2);
  const url = `${endpoints(pending.server).web}/webauthn-fallback-connector.html?data=${encodeURIComponent(data)}&v=2&locale=${language}&parent=${encodeURIComponent(ext.runtime.getURL('/'))}`;
  await ext.tabs.create({ url });
}

/** The security key page answered — if it is the server's own page, finish the login. */
export async function webAuthnResult(
  senderUrl: string | undefined,
  tabId: number | undefined,
  data: string,
) {
  const pending = await pendingLogin();
  if (!pending?.webauthn || !senderUrl) return;
  const page = new URL(senderUrl);
  const web = new URL(endpoints(pending.server).web);
  if (page.origin !== web.origin || !page.pathname.endsWith('/webauthn-fallback-connector.html'))
    return;
  await token(pending, {
    twoFactorProvider: '7',
    twoFactorToken: data,
    twoFactorRemember: pending.webauthnRemember ? '1' : '0',
  }).catch(async (error) => {
    await setSession('pendingLogin', { ...pending, message: failure(error).message });
    changed();
  });
  if (tabId !== undefined) await ext.tabs.remove(tabId).catch(() => undefined);
  // The popup closed when the tab opened: open it again where the browser allows.
  await openPopup();
}

// ── Unlocking ─────────────────────────────────────────────

async function lockedAccount(): Promise<Account> {
  const found = await activeAccount();
  if (!found) throw { kind: 'session-expired', message: 'Log in first.' };
  if (!found.protectedKey)
    throw { kind: 'session-expired', message: 'Log in again with your master password.' };
  return found;
}

export async function unlock(password: string): Promise<Status> {
  const found = await lockedAccount();
  await call((core) =>
    core.unlockWithPassword(found.email, found.kdf, found.protectedKey!, password),
  );
  await opened(found);
  return status();
}

const PIN_ATTEMPTS = 5;
/** A PIN that stays on disk across restarts guards the user key against a copied profile. */
export const PIN_MIN = 4;
export const PIN_MIN_AFTER_RESTART = 6;

/** One PIN try at a time: tries sent at once can't slip past the count. */
let pinQueue: Promise<unknown> = Promise.resolve();

function oneAtATime<T>(work: () => Promise<T>): Promise<T> {
  const run = pinQueue.then(work, work);
  pinQueue = run.catch(() => undefined);
  return run;
}

export function unlockWithPin(pin: string): Promise<Status> {
  return oneAtATime(() => tryPin(pin));
}

async function clearPin(accountId: string) {
  await removeSession('pin');
  await updateAccount(accountId, { pinProtected: null });
  await setPinAttempts(accountId, 0);
  changed();
}

async function tryPin(pin: string): Promise<Status> {
  const found = await lockedAccount();
  const inSession = await session('pin');
  const wrapped =
    inSession && inSession.accountId === found.id ? inSession.protected : found.pinProtected;
  if (!wrapped) throw { kind: 'no-pin', message: 'No PIN is set.' };
  // Counted before the try, in storage.local: ending the background (or the browser) halfway
  // through a try doesn't take it back.
  const attempts = (await pinAttempts(found.id)) + 1;
  if (attempts > PIN_ATTEMPTS) {
    await clearPin(found.id);
    throw { kind: 'pin-cleared', message: 'Too many wrong PINs.' };
  }
  await setPinAttempts(found.id, attempts);
  try {
    await call((core) =>
      core.unlockWithPin(found.email, found.kdf, found.protectedKey!, pin, wrapped),
    );
  } catch (error) {
    if (attempts >= PIN_ATTEMPTS) {
      // Guessing goes no further: the PIN is gone, the master password is needed.
      await clearPin(found.id);
      throw { kind: 'pin-cleared', message: 'Too many wrong PINs.' };
    }
    throw error;
  }
  await opened(found);
  return status();
}

/** A PIN for the open vault; `null` removes it. `afterRestart`: it keeps working after the browser restarts. */
export async function setPin(pin: string | null, afterRestart: boolean): Promise<Status> {
  const found = await requireUnlocked();
  if (pin === null) {
    await removeSession('pin');
    await updateAccount(found.id, { pinProtected: null });
  } else {
    const min = afterRestart ? PIN_MIN_AFTER_RESTART : PIN_MIN;
    if (/\s/.test(pin) || [...pin].length < min) {
      throw { kind: 'pin-too-short', message: `The PIN needs at least ${min} characters.` };
    }
    const wrapped = await call((core) => core.pinProtect(pin));
    if (afterRestart) {
      await updateAccount(found.id, { pinProtected: wrapped });
      await removeSession('pin');
    } else {
      await setSession('pin', { accountId: found.id, protected: wrapped });
      await updateAccount(found.id, { pinProtected: null });
    }
    await setPinAttempts(found.id, 0);
  }
  changed();
  return status();
}

export async function requireUnlocked(): Promise<Account> {
  const found = await activeAccount();
  if (!found || unlockedId !== found.id) throw { kind: 'locked', message: 'The vault is locked.' };
  return found;
}

// ── Logging out, switching ────────────────────────────────

export async function logout(id?: string): Promise<Status> {
  const found = id ? await account(id) : await activeAccount();
  if (!found) return status();
  if (found.id === unlockedId) await lock();
  // UwULock Server forgets this browser's refresh token too; others just see it unused.
  if (found.uwu) {
    await request(found, `/uwu/v1/devices/${encodeURIComponent(await deviceId())}`, {
      method: 'DELETE',
    }).catch(() => undefined);
  }
  await removeAccount(found.id);
  // Logging out on purpose is the way to accept a lowered KDF at the next login.
  await forgetKdfFloor(endpoints(found.server).identity, found.email);
  if ((await local('activeAccount')) === found.id) {
    await setLocal('activeAccount', (await accounts())[0]?.id ?? null);
  }
  const pin = await session('pin');
  if (pin?.accountId === found.id) await removeSession('pin');
  changed();
  return status();
}

export async function switchAccount(id: string): Promise<Status> {
  const found = await account(id);
  if (!found) throw { kind: 'not-found', message: 'This account is not here any more.' };
  if (unlockedId && unlockedId !== id) await lock();
  await setLocal('activeAccount', id);
  changed();
  return status();
}

// The server ended the session: logged out elsewhere, a new master password, the device
// removed. Whatever was open closes, and the account goes; the login screen says why. The KDF
// its login accepted stays (see `refuseWeakerKdf`).
whenSessionEnds((ended) => {
  void (async () => {
    if (ended.id === unlockedId) await lock();
    await removeAccount(ended.id);
    if ((await local('activeAccount')) === ended.id) {
      await setLocal('activeAccount', (await accounts())[0]?.id ?? null);
    }
    expiredNotice = ended.email;
    changed();
  })();
});

// ── Locking by itself ─────────────────────────────────────

export async function touch() {
  if (unlockedId) await setSession('lastActive', Date.now());
}

/** Called by the alarm every minute, and when the popup closes. */
export async function checkTimeout(minutes: number, popupClosed = false) {
  if (!unlockedId || minutes < 0) return;
  if (minutes === 0) {
    if (popupClosed) await lock();
    return;
  }
  const last = (await session('lastActive')) ?? Date.now();
  if (Date.now() - last > minutes * 60_000) await lock();
}

/** The popup, from the background: where the browser lets an extension open it. */
export async function openPopup() {
  const action = ext.action as typeof chrome.action & { openPopup?: () => Promise<void> };
  try {
    if (action.openPopup) {
      await action.openPopup();
      return;
    }
  } catch {
    // No window with focus, or no user gesture: a small window instead.
  }
  await ext.windows
    .create({
      url: ext.runtime.getURL('popup.html?window=1'),
      type: 'popup',
      width: 380,
      height: 600,
    })
    .catch(() => undefined);
}
