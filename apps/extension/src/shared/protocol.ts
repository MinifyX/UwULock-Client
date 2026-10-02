/**
 * What the parts of the extension say to each other.
 *
 * The background (a service worker in Chromium, an event page in Firefox) holds the keys and
 * the opened vault. Everything else asks it:
 *
 * - the popup and the prompt window (extension pages) with `PageRequest`s — they may ask for
 *   anything, as the desktop app's page asks its Rust side;
 * - the content scripts in web pages with `ContentRequest`s — they learn how many items fit the
 *   page they run in, and the values of exactly one item after somebody picked it. Which page
 *   that is, the background takes from the sender, never from the message;
 * - the inline menu's list, an extension page (menu.html) in a frame in the web page, over a
 *   port (`MenuRequest`, `MenuMessage`): it lists the names and takes the pick, so what a page's
 *   own renderer does can't forge one (security review 0.3, CL-L8).
 *
 * The background answers every request with `Reply<T>`: `{ ok: true, value }` or
 * `{ ok: false, error: { kind, message } }`.
 */

export type Failure = { kind: string; message: string };

export type Reply<T> = { ok: true; value: T } | { ok: false; error: Failure };

// ── Shared shapes ─────────────────────────────────────────

export type VaultState = 'logged-out' | 'locked' | 'unlocked';

export type ServerKind = 'bitwarden-us' | 'bitwarden-eu' | 'self-hosted';

/**
 * `wifi` is a secure note with UwULock's marker field (docs/wifi.md): the popup lists and shows
 * it as a network; autofill only ever sees the note it is, and offers notes nowhere.
 */
export type ItemKind = 'login' | 'note' | 'card' | 'identity' | 'ssh-key' | 'wifi';

/** An item as a web page may see it listed: never a secret. */
export type PageItem = {
  id: string;
  kind: ItemKind;
  name: string;
  /** The username, or for cards the brand and last digits. */
  subtitle: string | null;
  favorite: boolean;
  hasTotp: boolean;
  /** Asks for the master password before anything is filled. */
  reprompt: boolean;
};

/** What a content script learns about its page: how many items fit it, not which. */
export type PageInfo = {
  state: VaultState;
  /**
   * How many logins match this frame's address, and how many cards and addresses may be
   * offered here (the page itself and frames of its origin). The names are for the menu's
   * frame only (`MenuView`).
   */
  counts: { logins: number; cards: number; identities: number };
  /** The frame is plain http: filling asks first. */
  insecure: boolean;
  /** Show the button and menu in login fields. */
  inlineMenu: boolean;
  /** Offer to save or update logins after a form was sent. */
  savePrompt: boolean;
  language: 'de' | 'en';
  /** UwULock Server's own features the account's server offers (see `Status.uwu`); empty elsewhere. */
  uwuFeatures: string[];
};

export type LoginValues = {
  kind: 'login';
  username: string | null;
  password: string | null;
  /** The current one-time code, if the item has an authenticator key. */
  totp: string | null;
};

export type CardValues = {
  kind: 'card';
  cardholderName: string | null;
  brand: string | null;
  number: string | null;
  expMonth: string | null;
  expYear: string | null;
  code: string | null;
};

export type IdentityValues = {
  kind: 'identity';
  title: string | null;
  firstName: string | null;
  middleName: string | null;
  lastName: string | null;
  username: string | null;
  company: string | null;
  email: string | null;
  phone: string | null;
  address1: string | null;
  address2: string | null;
  address3: string | null;
  postalCode: string | null;
  city: string | null;
  state: string | null;
  country: string | null;
};

export type FillValues = LoginValues | CardValues | IdentityValues;

/** Why a fill was refused. `insecure`: ask the person first, then send again with `confirmedInsecure`. */
export type FillRefusal =
  'locked' | 'insecure' | 'no-match' | 'reprompt' | 'expired' | 'not-found' | 'refused';

export type FillAnswer =
  { filled: true; values: FillValues } | { filled: false; reason: FillRefusal };

/** A save or update offered in the page's notification bar. The credentials stay in the background. */
export type SavePrompt = {
  id: string;
  action: 'save' | 'update';
  /** The site, as shown: `github.com`. */
  host: string;
  username: string | null;
  /** For an update: the item that would change. */
  itemName: string | null;
};

export type SaveAnswer = 'save' | 'update' | 'never' | 'dismiss';

// ── Passkeys ──────────────────────────────────────────────
//
// The page's `navigator.credentials.create/get` options, with every buffer as base64url, as the
// script in the page hands them over; and the credential it gets back, the same way.

export type PasskeyUser = { id: string; name: string; displayName: string };

export type PasskeyCreateOptions = {
  rp: { id?: string; name: string };
  user: PasskeyUser;
  challenge: string;
  pubKeyCredParams: { type: string; alg: number }[];
  excludeCredentials?: { id: string; type: string }[];
  authenticatorSelection?: {
    residentKey?: string;
    requireResidentKey?: boolean;
    userVerification?: string;
    authenticatorAttachment?: string;
  };
  attestation?: string;
  timeout?: number;
  extensions?: Record<string, unknown>;
};

export type PasskeyGetOptions = {
  rpId?: string;
  challenge: string;
  allowCredentials?: { id: string; type: string }[];
  userVerification?: string;
  timeout?: number;
  mediation?: string;
  extensions?: Record<string, unknown>;
};

export type PasskeyCredential = {
  id: string;
  rawId: string;
  type: 'public-key';
  authenticatorAttachment: 'platform' | 'cross-platform';
  clientDataJSON: string;
  /** create */
  attestationObject?: string;
  authenticatorData: string;
  publicKey?: string;
  publicKeyAlgorithm?: number;
  transports?: string[];
  /** get */
  signature?: string;
  userHandle?: string | null;
  clientExtensionResults: Record<string, unknown>;
};

/**
 * The answer for the page: a credential; `fallback` — let the browser's own authenticator do
 * it (cancelled, nothing matching, or not ours to do); or a DOMException to throw.
 */
export type PasskeyAnswer =
  | { kind: 'credential'; credential: PasskeyCredential }
  | { kind: 'fallback' }
  | {
      kind: 'error';
      name: 'NotAllowedError' | 'InvalidStateError' | 'SecurityError' | 'AbortError';
      message: string;
    };

// ── Content script → background ───────────────────────────

export type ContentRequest =
  /** Names for the page's inline menu, and the settings the page needs. */
  | { type: 'content:page-info' }
  /**
   * The values of one item, answering a `bg:fill-offer` with its `token`: after somebody picked
   * it in the popup, the context menu, the menu's frame, or pressed the shortcut. Nothing is
   * filled without one.
   */
  | { type: 'content:fill'; itemId: string; token: string; confirmedInsecure?: boolean }
  /**
   * The inline menu opens for a field of this frame: a session the menu's frame is shown for
   * (`menu.html#<session>`). `kind`: what the field takes; `maskable`: a username or email field.
   */
  | { type: 'content:menu-open'; kind: MenuKind; maskable: boolean }
  | { type: 'content:menu-close'; session: string }
  /** The field's ↓ key: the list's first entry is selected and takes the keys. */
  | { type: 'content:menu-focus'; session: string }
  /** A login form was sent (or a username on its own, the first step of two). */
  | {
      type: 'content:submitted';
      username: string | null;
      password: string | null;
      /** A change-password form: the new one. */
      newPassword: string | null;
    }
  /** A page loaded: is a save prompt waiting for this tab? (top frame only) */
  | { type: 'content:pending-prompt' }
  /**
   * The bar's answer. The reply is null, or — when another account was opened since the bar
   * asked — the question again, for the account open now (nothing was saved).
   */
  | { type: 'content:prompt-answer'; id: string; answer: SaveAnswer }
  /** The page had no field for the one-time code: copy it instead, like Bitwarden does. */
  | { type: 'content:copy-totp'; itemId: string }
  /** The vault is locked: open the popup to unlock. */
  | { type: 'content:open-popup' }
  /** UwULock Server's / Bitwarden's WebAuthn fallback connector answered (two-step login). */
  | { type: 'content:webauthn-result'; data: string; remember: boolean }
  | { type: 'content:passkey-create'; requestId: string; options: PasskeyCreateOptions }
  | { type: 'content:passkey-get'; requestId: string; options: PasskeyGetOptions }
  | { type: 'content:passkey-abort'; requestId: string };

// ── Background → content script ───────────────────────────

/** To the extension's own pages: something changed, ask for the status again. */
export type StatusMessage = { type: 'bg:status-changed' };

export type BackgroundMessage =
  /**
   * Somebody asked to fill this tab (popup, shortcut, context menu). Every frame that has a
   * form claims the values with `content:fill` and this token; the background decides per
   * frame whether that frame may have them.
   */
  | {
      type: 'bg:fill-offer';
      token: string;
      itemId: string;
      kind: ItemKind;
      /** Picked in this frame's inline menu: fill the menu's field. */
      session?: string;
    }
  /**
   * The menu's frame took a pick: is the frame, as this page shows it, uncovered and unchanged
   * long enough? Answered with a boolean (the clickjacking guard of what lives in the page).
   */
  | { type: 'bg:menu-guard'; session: string }
  /** The menu's frame wants this height. */
  | { type: 'bg:menu-size'; session: string; height: number }
  /** Close the menu; `refocus`: back into its field. */
  | { type: 'bg:menu-close'; session: string; refocus: boolean }
  /** Type `value` into the menu's field (a new masked address). */
  | { type: 'bg:fill-text'; session: string; value: string }
  /** Show the save/update bar (top frame). */
  | { type: 'bg:save-prompt'; prompt: SavePrompt }
  /** The vault was unlocked, locked or changed: ask for `content:page-info` again. */
  | { type: 'bg:vault-changed' };

// ── The inline menu's frame ⇄ background (a port named `menu`) ─

/** `signup`: a sign-up form's username or email field, where only a masked address is offered. */
export type MenuKind = 'login' | 'card' | 'identity' | 'signup';

/** What the menu's frame lists. */
export type MenuView = {
  state: VaultState;
  kind: MenuKind;
  items: PageItem[];
  /** Offer "New masked address" (UwULock Server, a username or email field). */
  masked: boolean;
  language: 'de' | 'en';
};

/** What the frame answers after a pick; `unseen`: the page's side refused the click. */
export type MenuPickAnswer = { filled: true } | { filled: false; reason: FillRefusal | 'unseen' };

export type MenuRequest =
  | { type: 'hello'; session: string }
  | { type: 'pick'; itemId: string; confirmedInsecure?: boolean }
  | { type: 'masked' }
  | { type: 'open-popup' }
  | { type: 'open-masked-settings' }
  | { type: 'size'; height: number }
  | { type: 'close'; refocus: boolean };

export type MenuMessage =
  | { type: 'view'; view: MenuView }
  /** The session is over (the background was restarted, or the page closed the menu). */
  | { type: 'gone' }
  | { type: 'picked'; answer: MenuPickAnswer }
  | { type: 'masked'; failure: Failure | null }
  /** The field's arrow key opened the list: select its first entry. */
  | { type: 'select-first' };

// ── Extension pages (popup, prompt) → background ──────────

export type AccountBrief = {
  id: string;
  email: string;
  name: string | null;
  server: string;
  serverKind: ServerKind;
  active: boolean;
  unlocked: boolean;
};

export type Status = {
  state: VaultState;
  accountId: string | null;
  email: string | null;
  name: string | null;
  server: string | null;
  serverKind: ServerKind | null;
  serverUrl: string | null;
  webVault: string | null;
  lastSync: number | null;
  syncing: boolean;
  syncError: string | null;
  /** The server logged this browser out: the popup says so on the login screen. */
  sessionExpired: boolean;
  pinSet: boolean;
  accounts: AccountBrief[];
  /** UwULock Server's own features, from `/uwu/v1/info`; null for Bitwarden and Vaultwarden. */
  uwu: UwuInfo | null;
  /** Logins sent while the vault was locked, waiting for a decision. */
  pendingSaves: number;
  /** A login waiting for its second step — the popup may have closed while somebody fetched the code. */
  login: PendingLoginInfo | null;
};

export type PendingLoginInfo = {
  email: string;
  server: string;
  step: 'two-factor' | 'new-device';
  methods: TwoFactorMethod[];
  message: string | null;
};

/** What UwULock Server says about itself: which of its own features this extension may offer. */
export type UwuInfo = {
  version: string | null;
  features: string[];
  /** Automatic icons (`GET <url>/<host>/icon.png`); null when the server has them switched off. */
  icons?: { automatic: boolean; url: string | null } | null;
  /** The admin's send domains, without the main host. */
  sendDomains?: SendDomain[];
  /**
   * The admin's feature switches (UwULock Server 0.6.0-beta.2), `true` when the extra works; what
   * is off is already left out of `features`. Missing from an older server.
   */
  switches?: Record<string, boolean> | null;
};

export type SendDomain = { id: string; url: string };

// ── UwULock Server's extras ───────────────────────────────

/** `GET /uwu/v1/masked/connection`, what the popup needs of it. */
export type MaskedConnection = {
  connected: boolean;
  /** `ok`, `revoked` (connect again), `unreachable` (the last call failed). */
  status: string | null;
  server: string | null;
  username: string | null;
  defaultDomain: string | null;
  /** Where the web vault connects an account: `<web>/#/settings/masked`. */
  settingsUrl: string;
};

export type MaskedAddress = { id: string; email: string; forDomain: string | null };

/** A value of an item that can go into a Send; `label` is a custom field's own name. */
export type ShareableField = { name: string; label?: string };

export type ShareOptions = {
  /** `[name, label]`: the value, and what the recipient reads before it. */
  fields: [string, string][];
  /** Hours until the server deletes it. */
  deletionHours: number;
  /** null: as often as anybody wants until it is deleted. */
  maxAccessCount: number | null;
  password: string | null;
  /**
   * An entry Send (uwulock-core `entry_send`): UwULock Server's Send page shows it as the entry.
   * Only then may `fields` name `totp` — live codes there, never the key in the readable text.
   */
  entry?: boolean;
};

export type SharedSend = { id: string; link: string; deletionDate: string; onSendDomain: boolean };

/** The owner's file request, as the popup lists it (read only). */
export type FileRequestEntry = {
  id: string;
  /** null: the label didn't open (the extras key was reset), shown as "unnamed". */
  label: string | null;
  expirationDate: string | null;
  submissionCount: number;
  maxSubmissions: number | null;
  unseen: number;
  disabled: boolean;
  expired: boolean;
  /** `<web>/#/file-requests/<id>`, where it is managed. */
  manageUrl: string;
};

/**
 * The file requests, or why there are none to show: the extras key isn't there yet (`none`,
 * made by the web vault or the desktop app) or was lost in a key rotation (`lost`).
 */
export type FileRequests =
  | { state: 'open'; requests: FileRequestEntry[]; webUrl: string }
  | { state: 'none' | 'lost'; requests: []; webUrl: string };

export type TwoFactorMethod = {
  provider: number;
  kind: 'authenticator' | 'email' | 'yubikey' | 'duo' | 'webauthn' | 'u2f' | 'other';
  supported: boolean;
  hint: string | null;
};

export type LoginStep =
  | { step: 'done'; status: Status }
  | { step: 'two-factor'; methods: TwoFactorMethod[]; message: string | null }
  | { step: 'new-device' };

export type ServerChoice = { kind: ServerKind; url?: string };

export type ItemSummary = {
  id: string;
  kind: ItemKind;
  name: string;
  subtitle: string | null;
  host: string | null;
  favorite: boolean;
  folderId: string | null;
  organizationId: string | null;
  collectionIds: string[];
  deleted: boolean;
  archived: boolean;
  reprompt: boolean;
  /** False when the organisation hides this item's passwords from this member. */
  viewPassword: boolean;
  hasTotp: boolean;
  hasPassword: boolean;
  hasUsername: boolean;
  broken: boolean;
  revisionDate: string | null;
};

export type Folder = { id: string; name: string };
export type Collection = { id: string; organizationId: string; name: string };
export type Organization = { id: string; name: string };
export type Overview = {
  folders: Folder[];
  collections: Collection[];
  organizations: Organization[];
  skipped: number;
};

export type FieldKind = 'text' | 'hidden' | 'boolean' | 'linked';

export type ItemDetail = {
  summary: ItemSummary;
  locked: boolean;
  notes?: string | null;
  login?: {
    username: string | null;
    hasPassword: boolean;
    hasTotp: boolean;
    passwordRevisionDate: string | null;
    uris: { uri: string; match: number | null; host: string | null; openable: boolean }[];
    passkeys: number;
  } | null;
  card?: {
    cardholderName: string | null;
    brand: string | null;
    numberEnding: string | null;
    expMonth: string | null;
    expYear: string | null;
    hasCode: boolean;
  } | null;
  identity?: { name: string; sensitive: boolean; value: string | null }[] | null;
  sshKey?: { publicKey: string | null; fingerprint: string | null; hasPrivateKey: boolean } | null;
  fields?: {
    index: number;
    name: string | null;
    kind: FieldKind;
    value: string | null;
    hasValue: boolean;
  }[];
  passwordHistory?: { index: number; lastUsed: string | null }[];
  attachments?: number;
  creationDate?: string | null;
};

/**
 * `next` is the code of the following period, `showNext` true in its last 10 seconds
 * ("Nächster: 123 456"); `copy` with the field `totp-next` copies it.
 */
export type TotpCode = {
  code: string;
  remaining: number;
  period: number;
  next: string;
  showNext: boolean;
};

/** A passkey of a login, nothing secret. `readable: false`: it can only be deleted. */
export type PasskeyInfo = {
  index: number;
  readable: boolean;
  credentialId: string;
  rpId: string;
  rpName: string | null;
  userName: string | null;
  userDisplayName: string | null;
  creationDate: string;
  discoverable: boolean;
};

/** What the editor sends back; see the desktop app's `Draft`: `null` keeps a value, `''` clears it. */
export type Draft = {
  kind: ItemKind;
  name: string;
  notes?: string | null;
  favorite: boolean;
  reprompt: boolean;
  folderId: string | null;
  login?: {
    username?: string | null;
    password?: string | null;
    totp?: string | null;
    uris: { uri: string; match: number | null }[];
  };
  card?: {
    cardholderName?: string | null;
    brand?: string | null;
    number?: string | null;
    expMonth?: string | null;
    expYear?: string | null;
    code?: string | null;
  };
  identity?: Record<string, string>;
  fields: { name: string | null; kind: FieldKind; value?: string | null; from: number | null }[];
};

export type PasswordOptions = {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  avoidAmbiguous: boolean;
  /** At least this many of a set that is on (0 or 1: at least one); the length is raised to fit. */
  minLowercase?: number;
  minUppercase?: number;
  minNumber?: number;
  minSpecial?: number;
};

export type PassphraseOptions = {
  words: number;
  separator: string;
  capitalize: boolean;
  includeNumber: boolean;
};

export type GeneratorSettings = {
  mode: 'password' | 'passphrase';
  password: PasswordOptions;
  passphrase: PassphraseOptions;
};

/** A generated password kept for a while, like Bitwarden's generator history. */
export type Generated = { password: string; date: number };

export type LockTimeout = 0 | 1 | 5 | 15 | 30 | 60 | 240 | -1;

export type Settings = {
  language: 'system' | 'de' | 'en';
  theme: 'system' | 'light' | 'dark';
  /** The popup's font, in this browser (the desktop app's `lib/fonts.ts`). */
  font: 'uwu' | 'manrope' | 'rubik' | 'dmsans' | 'system';
  /** Minutes without use. 0: as soon as the popup closes; -1: only when the browser restarts. */
  lockTimeout: LockTimeout;
  /** Lock when the browser says the computer's screen was locked (`idle` state `locked`). */
  lockWithSystem: boolean;
  /** Seconds; 0 never. */
  clipboardClear: number;
  inlineMenu: boolean;
  savePrompt: boolean;
  /** Copy the one-time code after a login was filled, when the page has no field for it. */
  copyTotp: boolean;
  /** Offer to create and use passkeys. */
  passkeys: boolean;
  /** Icons in the vault list: own icons and the server's automatic ones (UwULock Server). */
  showIcons: boolean;
  /** Hosts for which "never" was picked in the save prompt. */
  neverSave: string[];
  /** Bitwarden's default match detection for addresses without one: 0 domain … 5 never. */
  defaultMatch: number;
  generator: GeneratorSettings;
};

/** A login sent while the vault was locked, as the popup lists it after unlocking. */
export type PendingSave = {
  id: string;
  host: string;
  username: string | null;
  action: 'save' | 'update';
  itemName: string | null;
};

/** The passkey window: what it asks, and what the person answered. */
export type PasskeyPrompt =
  | {
      id: string;
      kind: 'create';
      origin: string;
      rpId: string;
      rpName: string;
      userName: string;
      /** Logins of this site the passkey could be added to (their passkey is replaced). */
      candidates: (PageItem & { hasPasskey: boolean })[];
      /** A passkey from `excludeCredentials` is in the vault already. */
      excluded: boolean;
      userVerification: 'required' | 'preferred' | 'discouraged';
      /** The vault is locked or logged out: the window unlocks first. */
      state: VaultState;
    }
  | {
      id: string;
      kind: 'get';
      origin: string;
      rpId: string;
      choices: {
        itemId: string;
        credentialId: string;
        name: string;
        userName: string | null;
        reprompt: boolean;
      }[];
      userVerification: 'required' | 'preferred' | 'discouraged';
      state: VaultState;
    };

/**
 * `password`: the master password, when the site asks for user verification or the item for
 * the re-prompt; the background checks it.
 */
export type PasskeyDecision =
  | { id: string; choice: 'cancel' }
  | { id: string; choice: 'browser' }
  | { id: string; choice: 'create'; itemId: string | null; password: string | null }
  | {
      id: string;
      choice: 'use';
      itemId: string;
      credentialId: string;
      password: string | null;
    };

export type PageRequest =
  | { type: 'status' }
  | { type: 'login'; server: ServerChoice; email: string; password: string }
  | { type: 'login-two-factor'; provider: number; code: string; remember: boolean }
  | { type: 'login-webauthn'; remember: boolean }
  | { type: 'login-new-device'; code: string }
  | { type: 'login-send-email' }
  | { type: 'login-cancel' }
  | { type: 'forget-kdf'; server: ServerChoice; email: string }
  | { type: 'unlock'; password: string }
  | { type: 'unlock-pin'; pin: string }
  | { type: 'set-pin'; pin: string | null; afterRestart: boolean }
  | { type: 'lock' }
  | { type: 'logout'; id?: string }
  | { type: 'switch-account'; id: string }
  | { type: 'touch' }
  | { type: 'sync' }
  | { type: 'overview' }
  | { type: 'items' }
  | { type: 'item'; id: string }
  | { type: 'reveal'; id: string; field: string }
  | { type: 'copy'; id: string; field: string }
  | { type: 'copy-text'; text: string }
  | { type: 'totp'; id: string }
  | { type: 'verify-reprompt'; id: string; password: string }
  | { type: 'save-item'; id: string | null; draft: Draft }
  | { type: 'set-favorite'; id: string; favorite: boolean }
  | { type: 'delete-item'; id: string; permanent: boolean }
  | { type: 'restore-item'; id: string }
  | { type: 'save-folder'; id: string | null; name: string }
  | { type: 'delete-folder'; id: string }
  | { type: 'open-uri'; id: string; index: number }
  | { type: 'item-passkeys'; id: string }
  /** `credentialId`: only if the passkey at `index` still is that one (else `conflict`). */
  | { type: 'delete-passkey'; id: string; index: number; credentialId: string | null }
  | { type: 'generate'; settings: GeneratorSettings }
  | { type: 'generator-history' }
  | { type: 'clear-generator-history' }
  | { type: 'settings' }
  | { type: 'set-settings'; patch: Partial<Settings> }
  /** The items for the active tab, and whether it is plain http. */
  | { type: 'tab-items' }
  /** `password`: the master password, which an item with the re-prompt needs for every fill. */
  | { type: 'fill-tab'; id: string; confirmedInsecure: boolean; password?: string }
  | { type: 'pending-saves' }
  | { type: 'answer-pending-save'; id: string; answer: SaveAnswer }
  | { type: 'passkey-prompt'; id: string }
  | { type: 'passkey-decide'; decision: PasskeyDecision }
  /** Icons as data URLs for items the popup shows: own, else automatic; absent ones get the glyph. */
  | { type: 'icons'; ids: string[] }
  | { type: 'masked-connection' }
  /** A masked address for the active tab's site; `cipherId` from the item editor. */
  | { type: 'masked-create'; cipherId: string | null }
  | { type: 'share-fields'; id: string }
  | { type: 'share-item'; id: string; options: ShareOptions }
  | { type: 'file-requests' }
  | { type: 'copy-file-request-link'; id: string };

export type TabItems = {
  url: string | null;
  host: string | null;
  insecure: boolean;
  /** The active tab can't be filled (a browser page, the web store). */
  fillable: boolean;
  logins: ItemSummary[];
  cards: ItemSummary[];
  identities: ItemSummary[];
};
