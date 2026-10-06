/**
 * A pretend Rust side for looking at the UI in a plain browser (screenshots
 * of the phone and iPad layout): `pnpm dev`, then open `/?mock` (add
 * `&locked` for the lock screen, `&loggedout` for the login). Only ever loaded by `pnpm dev` (main.tsx); the
 * build leaves it out. Everything in here is made up, with reserved example
 * domains only.
 */

import { emit } from '@tauri-apps/api/event';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';

const params = new URLSearchParams(window.location.search);
const now = Math.floor(Date.now() / 1000);
const iso = (daysAgo: number) => new Date(Date.now() - daysAgo * 86_400_000).toISOString();

type Item = {
  id: string;
  kind: 'login' | 'note' | 'card' | 'identity' | 'ssh-key' | 'wifi';
  name: string;
  subtitle: string | null;
  host: string | null;
  favorite: boolean;
  folderId: string | null;
  organizationId: string | null;
  collectionIds: string[];
  deleted: boolean;
  reprompt: boolean;
  viewPassword: boolean;
  hasTotp: boolean;
  hasPassword: boolean;
  hasUsername: boolean;
  broken: boolean;
  revisionDate: string | null;
};

const item = (over: Partial<Item> & Pick<Item, 'id' | 'name'>): Item => ({
  kind: 'login',
  subtitle: null,
  host: null,
  favorite: false,
  folderId: null,
  organizationId: null,
  collectionIds: [],
  deleted: false,
  reprompt: false,
  viewPassword: true,
  hasTotp: false,
  hasPassword: true,
  hasUsername: true,
  broken: false,
  revisionDate: iso(3),
  ...over,
});

const items: Item[] = [
  item({
    id: 'github',
    name: 'GitHub',
    subtitle: 'nyu@example.com',
    host: 'github.example.com',
    favorite: true,
    hasTotp: true,
    folderId: 'work',
  }),
  item({
    id: 'paypal',
    name: 'PayPal',
    subtitle: 'nyu@example.com',
    host: 'pay.example.com',
    favorite: true,
    folderId: 'money',
  }),
  item({
    id: 'bank',
    name: 'Sparkasse',
    subtitle: '1234 5678',
    host: 'bank.example.org',
    folderId: 'money',
    hasTotp: true,
  }),
  item({
    id: 'mail',
    name: 'UwUMail',
    subtitle: 'nyu@example.org',
    host: 'mail.example.org',
    folderId: 'private',
  }),
  item({
    id: 'steam',
    name: 'Steam',
    subtitle: 'nyu_gaming',
    host: 'store.example.net',
    folderId: 'gaming',
  }),
  item({
    id: 'discord',
    name: 'Discord',
    subtitle: 'nyu#0001',
    host: 'chat.example.net',
    folderId: 'gaming',
    favorite: true,
  }),
  item({
    id: 'nas',
    name: 'NAS im Keller',
    subtitle: 'admin',
    host: 'nas.test',
    folderId: 'private',
  }),
  item({ id: 'router', name: 'Router', subtitle: 'admin', host: '192.0.2.1', folderId: 'private' }),
  item({
    id: 'netflix',
    name: 'Streaming',
    subtitle: 'family@example.com',
    host: 'tv.example.com',
    organizationId: 'family',
    collectionIds: ['stream'],
  }),
  item({
    id: 'shop',
    name: 'Online-Shop',
    subtitle: 'nyu@example.com',
    host: 'shop.example',
    folderId: 'private',
  }),
  item({ id: 'forum', name: 'Forum', subtitle: 'nyu', host: 'forum.example', folderId: null }),
  item({
    id: 'wlan',
    kind: 'wifi',
    name: 'UwU-WLAN',
    subtitle: 'WPA2',
    hasPassword: false,
    hasUsername: false,
    folderId: 'private',
  }),
  item({
    id: 'visa',
    kind: 'card',
    name: 'Visa',
    subtitle: '•••• 4242',
    hasPassword: false,
    hasUsername: false,
    folderId: 'money',
  }),
  item({
    id: 'note',
    kind: 'note',
    name: 'Wiederherstellungscodes',
    subtitle: null,
    hasPassword: false,
    hasUsername: false,
    reprompt: true,
  }),
  item({
    id: 'me',
    kind: 'identity',
    name: 'Nyu Example',
    subtitle: 'Nyu Example',
    hasPassword: false,
    hasUsername: false,
  }),
  item({
    id: 'key',
    kind: 'ssh-key',
    name: 'Server-Schlüssel',
    subtitle: 'SHA256:abc…',
    hasPassword: false,
    hasUsername: false,
    folderId: 'work',
  }),
  item({ id: 'old', name: 'Altes Konto', subtitle: 'nyu', host: 'old.example', deleted: true }),
];

const overview = {
  folders: [
    { id: 'work', name: 'Arbeit' },
    { id: 'money', name: 'Finanzen' },
    { id: 'gaming', name: 'Gaming' },
    { id: 'private', name: 'Privat' },
  ],
  organizations: [{ id: 'family', name: 'Familie' }],
  collections: [
    { id: 'stream', organizationId: 'family', name: 'Streaming' },
    { id: 'home', organizationId: 'family', name: 'Zuhause' },
  ],
  skipped: 0,
};

const account = {
  id: 'a1',
  label: 'Privat',
  email: 'nyu@example.com',
  name: 'Nyu',
  server: 'vault.example.com',
  serverKind: 'self-hosted',
  unlocked: true,
  active: true,
  lastSync: now - 120,
};

let state: 'unlocked' | 'locked' | 'logged-out' = params.has('locked')
  ? 'locked'
  : params.has('loggedout')
    ? 'logged-out'
    : 'unlocked';

const status = () => ({
  state,
  accountId: 'a1',
  label: 'Privat',
  email: 'nyu@example.com',
  name: 'Nyu',
  server: 'vault.example.com',
  serverKind: 'self-hosted',
  serverUrl: 'https://vault.example.com',
  lastSync: now - 120,
  syncing: false,
  syncError: null,
  sessionExpired: false,
  live: 'realtime',
  hello: true,
  helloKind: /android/i.test(navigator.userAgent) ? 'fingerprint' : 'faceId',
  accounts: [
    { ...account, unlocked: state === 'unlocked' },
    {
      id: 'a2',
      label: 'Arbeit',
      email: 'nyu@example.org',
      name: null,
      server: 'vault.example.org',
      serverKind: 'self-hosted',
      unlocked: false,
      active: false,
      lastSync: now - 86_400,
    },
  ],
});

const uwu = {
  uwu: true,
  features: [
    'icons',
    'own-icons',
    'icon-library',
    'versions',
    'travel-mode',
    'reminders',
    'file-requests',
    'masked-addresses',
    'send-domains',
    'send-emails',
    'sends',
    'suite',
  ],
  travel: { enabled: false, hiddenCount: null },
  unseen: { securityNotices: 0, fileRequestSubmissions: 2 },
  organizations: overview.organizations,
  sendDomains: [],
  reminders: { steam: { due: iso(-1), everyMonths: 6, isDue: true } },
  masked: {},
  ownIcons: {},
  automaticIcons: true,
  limits: {
    maxFileBytes: 26_214_400,
    versionsPerItem: 20,
    versionDays: 90,
    fileRequestMaxFiles: 10,
    fileRequestMaxDays: 90,
  },
  extrasKeyChanged: false,
};

function detail(id: string) {
  const summary = items.find((i) => i.id === id)!;
  const base = {
    summary,
    locked: summary.reprompt,
    notes:
      id === 'github' ? 'Recovery-Codes liegen im Tresor unter „Wiederherstellungscodes“.' : null,
    fields: [],
    passwordHistory:
      id === 'github'
        ? [
            { index: 0, lastUsed: iso(40) },
            { index: 1, lastUsed: iso(200) },
          ]
        : [],
    attachments: 0,
    creationDate: iso(500),
  };
  if (summary.kind === 'login')
    return {
      ...base,
      login: {
        username: summary.subtitle,
        hasPassword: true,
        hasTotp: summary.hasTotp,
        passwordRevisionDate: iso(40),
        uris: summary.host
          ? [
              {
                uri: `https://${summary.host}/login`,
                match: null,
                host: summary.host,
                openable: true,
              },
            ]
          : [],
        passkeys: id === 'github' ? 1 : 0,
      },
    };
  if (summary.kind === 'card')
    return {
      ...base,
      card: {
        cardholderName: 'Nyu Example',
        brand: 'Visa',
        numberEnding: '4242',
        expMonth: '8',
        expYear: '2029',
        hasCode: true,
      },
    };
  if (summary.kind === 'wifi')
    return {
      ...base,
      fields: [
        { index: 0, name: 'uwulock-wifi', kind: 'hidden', value: null, hasValue: true },
        { index: 1, name: 'SSID', kind: 'text', value: 'UwU-WLAN', hasValue: true },
        { index: 2, name: 'Password', kind: 'hidden', value: null, hasValue: true },
        { index: 3, name: 'Security', kind: 'text', value: 'WPA2', hasValue: true },
      ],
    };
  if (summary.kind === 'identity')
    return {
      ...base,
      identity: [
        { name: 'firstName', sensitive: false, value: 'Nyu' },
        { name: 'lastName', sensitive: false, value: 'Example' },
        { name: 'email', sensitive: false, value: 'nyu@example.com' },
      ],
    };
  if (summary.kind === 'ssh-key')
    return {
      ...base,
      sshKey: {
        publicKey: 'ssh-ed25519 AAAAC3Nza… nyu@example.com',
        fingerprint: 'SHA256:abcdEFGHijkl',
        hasPrivateKey: true,
      },
    };
  return { ...base, notes: 'Eins: 1234-5678\nZwei: 8765-4321' };
}

const finding = (id: string, over: object) => {
  const summary = items.find((i) => i.id === id)!;
  return {
    id,
    name: summary.name,
    subtitle: summary.subtitle,
    bits: 70,
    weak: false,
    reused: 0,
    unsecured: false,
    breached: 0,
    breachSources: [],
    host: summary.host,
    uri: summary.host ? `https://${summary.host}` : null,
    passwordChanged: iso(400),
    ...over,
  };
};

const findings = [
  finding('shop', { breached: 3, breachSources: ['hibp'] }),
  finding('forum', { weak: true, bits: 28, reused: 1 }),
  finding('steam', { reused: 1 }),
  finding('router', { unsecured: true }),
];

const health = {
  uwu: true,
  switches: {
    hibp: true,
    xonPasswords: false,
    siteBreaches: true,
    emailCheck: true,
    changePassword: true,
  },
  report: { findings, checked: 14, breachesChecked: true, breachesIncomplete: false },
  checkedAt: iso(0),
  siteBreaches: {},
  siteSources: [],
  sitesFailed: false,
  twofa: [],
  twofaSource: null,
  twofaFailed: false,
  ignored: [],
  cards: [
    { finding: findings[0], problems: [{ kind: 'breached', count: 3, sources: ['hibp'] }] },
    {
      finding: findings[1],
      problems: [
        { kind: 'weak', bits: 28 },
        { kind: 'reused', others: 1 },
      ],
    },
    { finding: findings[2], problems: [{ kind: 'reused', others: 1 }] },
    { finding: findings[3], problems: [{ kind: 'unsecured' }] },
  ],
  emailOptIn: { optedIn: true, since: iso(30) },
};

const send = (over: object) => ({
  id: 's',
  kind: 0,
  name: '',
  notes: null,
  text: null,
  hidden: false,
  fileName: null,
  size: null,
  maxAccessCount: null,
  accessCount: 0,
  hasPassword: false,
  authType: 0,
  emails: [],
  disabled: false,
  hideEmail: true,
  revisionDate: iso(1),
  expirationDate: null,
  deletionDate: iso(-6),
  entry: false,
  link: 'https://send.example.com/#/send/Ab3xQ9/k3y',
  sendDomainId: null,
  ...over,
});

const sends = [
  send({
    id: 's1',
    name: 'WLAN für Gäste',
    text: 'UwU-WLAN · Passwort: Zimt-Insel-Feder-7',
    accessCount: 3,
    maxAccessCount: 10,
  }),
  send({
    id: 's2',
    kind: 1,
    name: 'Mietvertrag.pdf',
    fileName: 'Mietvertrag.pdf',
    size: 2_400_000,
    hasPassword: true,
    authType: 1,
    accessCount: 1,
  }),
  send({
    id: 's3',
    name: 'Streaming für Mama',
    text: '…',
    accessCount: 5,
    maxAccessCount: 5,
    entry: true,
  }),
];

const request = (over: object) => ({
  id: 'r',
  label: '',
  title: '',
  note: null,
  owner: 'Nyu',
  link: 'https://vault.example.com/#/r/Kp2x#key',
  passwordSet: false,
  expirationDate: iso(-25),
  deletionDate: iso(-55),
  maxSubmissions: null,
  submissionCount: 0,
  maxFiles: 10,
  maxFileBytes: 26_214_400,
  textAllowed: true,
  sendDomainId: null,
  disabled: false,
  unseen: 0,
  bytes: 0,
  foreignKey: false,
  ...over,
});

const requests = [
  request({
    id: 'r1',
    label: 'Steuerunterlagen',
    title: 'Unterlagen für die Steuer',
    submissionCount: 2,
    unseen: 2,
  }),
  request({ id: 'r2', label: 'Bewerbungsfotos', title: 'Fotos' }),
  request({
    id: 'r3',
    label: 'Schlüssel vom Vermieter',
    title: 'Übergabe',
    disabled: true,
    submissionCount: 1,
  }),
];

const masked = [
  {
    id: 'm1',
    email: 'kirsche.fuchs42@mask.example.com',
    state: 'enabled',
    forDomain: 'shop.example',
    description: 'Shopping',
    createdAt: iso(60),
    lastMessageAt: iso(1),
    cipherId: null,
  },
  {
    id: 'm2',
    email: 'nebel.glas7@mask.example.com',
    state: 'disabled',
    forDomain: 'newsletter.example',
    description: 'Newsletter',
    createdAt: iso(90),
    lastMessageAt: iso(30),
    cipherId: null,
  },
  {
    id: 'm3',
    email: 'zimt.insel19@mask.example.com',
    state: 'enabled',
    forDomain: 'forum.example',
    description: 'Forum',
    createdAt: iso(10),
    lastMessageAt: null,
    cipherId: 'forum',
  },
];

/** A tiny coloured square as a PNG data URL (library icons, own icons). */
function swatch(color: string): string {
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 64;
  const ctx = canvas.getContext('2d')!;
  ctx.fillStyle = color;
  ctx.fillRect(0, 0, 64, 64);
  ctx.fillStyle = '#fff';
  ctx.beginPath();
  ctx.arc(32, 32, 14, 0, Math.PI * 2);
  ctx.fill();
  return canvas.toDataURL('image/png');
}

const LIBRARY = [
  'Nextcloud',
  'Jellyfin',
  'Home Assistant',
  'Proxmox',
  'Router',
  'Drucker',
  'Kamera',
  'NAS',
  'Pi-hole',
  'Grafana',
];
const COLORS = [
  '#0e8fae',
  '#7c3aed',
  '#2563eb',
  '#d1364a',
  '#0f9a6a',
  '#d47a06',
  '#4b5563',
  '#e11d74',
  '#9333ea',
  '#0d7a8a',
];

const handlers: Record<string, (args: Record<string, unknown>) => unknown> = {
  vault_status: () => status(),
  vault_items: () => items,
  vault_overview: () => overview,
  vault_item: ({ id }) => detail(String(id)),
  uwu_status: () => uwu,
  item_icons: ({ ids }) =>
    Object.fromEntries(
      (ids as string[])
        .filter((id) => ['github', 'paypal', 'discord'].includes(id))
        .map((id, n) => [id, swatch(COLORS[n * 3]!)]),
    ),
  totp_code: () => {
    const remaining = 30 - (Math.floor(Date.now() / 1000) % 30);
    return { code: '482913', remaining, period: 30, next: '105277', showNext: remaining <= 10 };
  },
  item_passkeys: ({ id }) =>
    id === 'github'
      ? [
          {
            index: 0,
            readable: true,
            credentialId: 'c1',
            fingerprint: 'f1',
            rpId: 'github.example.com',
            rpName: 'GitHub',
            userName: 'nyu',
            userDisplayName: 'Nyu',
            creationDate: iso(25),
            discoverable: true,
          },
        ]
      : [],
  reveal_field: ({ field }) =>
    String(field).startsWith('card') ? '4242 4242 4242 4242' : 'h7#Kq2!vLm9@xR4pTz',
  generate_password: ({ options }) => {
    const o = options as { length: number };
    const password = 'qT7#mV2!rK9@wP4sZx8&Lm3$Hn6%Bv1^'.slice(0, Math.max(5, o.length));
    return { password, bits: 118, length: password.length, required: 4 };
  },
  sync_now: () => status(),
  health_report: () => health,
  health_email_opt_in: () => health.emailOptIn,
  update_status: () => null,
  distribution: () => 'direct',
  passkey_provider_status: () => ({ warning: null }),
  sends: () => sends,
  send_options: () => ({ emails: true, domains: [], defaultDomainId: null }),
  file_requests: () => requests,
  file_request_submissions: ({ requestId }) =>
    requestId === 'r1'
      ? [
          {
            id: 'u1',
            creationDate: iso(0),
            seen: false,
            text: 'Hier die Belege fürs letzte Jahr.',
            senderName: 'Papa',
            senderEmail: null,
            files: [
              { id: 'f1', name: 'Lohnsteuer.pdf', size: 120_000, risky: false },
              { id: 'f2', name: 'Rente.pdf', size: 90_000, risky: false },
            ],
            broken: false,
          },
          {
            id: 'u2',
            creationDate: iso(0),
            seen: false,
            text: null,
            senderName: 'Papa',
            senderEmail: null,
            files: [{ id: 'f3', name: 'Kontoauszug.pdf', size: 60_000, risky: false }],
            broken: false,
          },
        ]
      : [],
  masked_connection: () => ({
    connected: true,
    server: 'mail.example.org',
    username: 'nyu@example.org',
    domains: ['mask.example.com'],
    defaultDomain: 'mask.example.com',
    status: 'ok',
  }),
  masked_addresses: () => masked,
  icon_library: () => ({
    updated: iso(1),
    sources: [
      {
        id: 'demo',
        name: 'Demo-Icons',
        url: 'https://icons.example.com',
        license: 'CC0',
        licenseUrl: 'https://icons.example.com/license',
        attribution: 'Beispiel',
      },
    ],
    icons: LIBRARY.map((name) => ({
      source: 'demo',
      id: name.toLowerCase().replace(/\s+/g, '-'),
      name,
      variants: ['default', 'light', 'dark'],
      aliases: [],
    })),
  }),
  library_icon: ({ iconId, variant }) => {
    const n = LIBRARY.findIndex((name) => name.toLowerCase().replace(/\s+/g, '-') === iconId);
    return swatch(
      variant === 'light' ? '#f4f4f5' : variant === 'dark' ? '#1c1420' : COLORS[n % COLORS.length]!,
    );
  },
  item_versions: () => [],
  suite_view: ({ space }) => ({ space, exists: true, records: [] }),
  uwu_travel: () => uwu.travel,
  unlock: () => {
    state = 'unlocked';
    void emit('vault-status', status());
    return status();
  },
  unlock_with_hello: () => {
    // As if the face or fingerprint prompt was cancelled.
    throw { kind: 'biometric-cancelled' };
  },
  lock: () => {
    state = 'locked';
    void emit('vault-status', status());
  },
};

mockWindows('main');
mockIPC(
  (cmd, args) => {
    const handler = handlers[cmd];
    if (handler) return handler((args ?? {}) as Record<string, unknown>);
    if (!cmd.startsWith('plugin:')) console.debug('mock: no answer for', cmd, args);
    return null;
  },
  { shouldMockEvents: true },
);
