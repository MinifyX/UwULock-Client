/**
 * What another app hands over through Apple's Credential Exchange (iOS 26: Apple Passwords →
 * "Export data to another app" → UwULock): the FIDO Credential Exchange Format (CXF 1.0) as JSON,
 * the way the iOS plugin encodes Apple's `ASExportedCredentialData` (CredentialExchange.swift in
 * tauri-plugin-uwulock-mobile). Turned into Bitwarden's JSON export like every other import; the page
 * hands it to the preview with `parsedFromCollected` (run.ts), so the preview and `import_vault`
 * are the same.
 *
 * Read leniently: the spec's names (`username`, `rpId`, `credentialId`, `creationAt`) and Swift's
 * (`userName`, `relyingPartyIdentifier`, `credentialID`, `created`); binary values in base64url
 * (the spec) or base64; editable fields as `{ fieldType, value }` or plain strings; dates in
 * seconds or as ISO text.
 *
 * - basic-auth, passkey, totp → one login (Bitwarden keeps one passkey per login: a second one,
 *   or a second password, becomes a login of its own); `scope.urls` and Android apps → addresses
 * - credit-card → card; wifi → Wi-Fi network (docs/wifi.md); note → the notes
 * - address, person-name → identity; anything else → custom fields, as the other importers do
 * - collections → folders ("A/B" for one inside another)
 *
 * https://fidoalliance.org/specs/cx/cxf-v1.0-ps-20250814.html
 */

import { base32, fromBase64, ImportError } from './bytes.ts';
import { blank, cardBrand, Collector, some, splitName } from './collect.ts';
import { t } from './i18n.ts';
import {
  FieldType,
  ItemType,
  type ExportItem,
  type BitwardenExport,
  type ExportPasskey,
} from './types.ts';

type Json = Record<string, unknown>;

const isObject = (value: unknown): value is Json =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const list = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);

/** The first of `keys` that `object` has. */
function pick(object: Json, ...keys: string[]): unknown {
  for (const key of keys) if (key in object && object[key] != null) return object[key];
  return undefined;
}

/** An editable field's value (`{ fieldType, value }`), or a plain string. */
function value(raw: unknown): string | null {
  if (typeof raw === 'string') return raw;
  if (typeof raw === 'number' || typeof raw === 'boolean') return String(raw);
  if (isObject(raw)) return value(raw.value);
  return null;
}

function str(object: Json, ...keys: string[]): string | null {
  return some(value(pick(object, ...keys)));
}

/** base64url or base64 → bytes. */
function bytes(raw: unknown): Uint8Array | null {
  if (typeof raw !== 'string' || !raw.trim()) return null;
  try {
    const plain = raw.trim().replace(/-/g, '+').replace(/_/g, '/').replace(/=+$/, '');
    return fromBase64(plain + '='.repeat((4 - (plain.length % 4)) % 4));
  } catch {
    return null;
  }
}

/** URL-safe base64 without padding, as Bitwarden keeps a passkey's values. */
function b64url(data: Uint8Array): string {
  return base64(data).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function base64(data: Uint8Array): string {
  let binary = '';
  for (const byte of data) binary += String.fromCharCode(byte);
  return btoa(binary);
}

/** Seconds (or milliseconds, or ISO text) → ISO text. */
function date(raw: unknown): string | null {
  if (typeof raw === 'number' && Number.isFinite(raw)) {
    const ms = raw > 1e12 ? raw : raw * 1000;
    return new Date(ms).toISOString();
  }
  if (typeof raw === 'string' && !Number.isNaN(Date.parse(raw))) {
    return new Date(Date.parse(raw)).toISOString();
  }
  return null;
}

/** A TOTP credential as the vault keeps it: the base32 secret, or otpauth:// when not the default. */
function totp(credential: Json, title: string): string | null {
  const raw = pick(credential, 'secret');
  if (typeof raw !== 'string' || !raw.trim()) return null;
  const clean = raw.trim().replace(/\s+/g, '');
  // Apple's Data comes as base64 (JSONEncoder); base32 only when it can't be anything else:
  // upper case only (a case-insensitive test would take ~7 % of base64 secrets for base32 and
  // store another seed) and a length base32 can have.
  const unpadded = clean.replace(/=+$/, '');
  let secret: string | null =
    /^[A-Z2-7]+=*$/.test(clean) && [0, 2, 4, 5, 7].includes(unpadded.length % 8) ? unpadded : null;
  if (!secret) {
    const decoded = bytes(clean);
    secret = decoded && decoded.length > 0 ? base32(decoded) : null;
  }
  if (!secret) return null;
  const algorithm = String(pick(credential, 'algorithm') ?? 'sha1').toUpperCase();
  const whole = (value: unknown, min: number, max: number, fallback: number) => {
    const n = Number(value ?? fallback);
    return Number.isInteger(n) && n >= min && n <= max ? n : fallback;
  };
  const digits = whole(pick(credential, 'digits'), 6, 10, 6);
  const period = whole(pick(credential, 'period'), 1, 300, 30);
  const issuer = str(credential, 'issuer');
  const user = str(credential, 'username', 'userName');
  if (algorithm === 'SHA1' && digits === 6 && period === 30) return secret;
  const label = encodeURIComponent([issuer ?? title, user].filter(Boolean).join(':') || 'UwULock');
  const params = new URLSearchParams({
    secret,
    algorithm,
    digits: String(digits),
    period: String(period),
  });
  if (issuer) params.set('issuer', issuer);
  return `otpauth://totp/${label}?${params.toString()}`;
}

function passkey(credential: Json, created: string): ExportPasskey | null {
  const id = bytes(pick(credential, 'credentialId', 'credentialID'));
  const key = bytes(pick(credential, 'key'));
  const rpId = str(credential, 'rpId', 'relyingPartyIdentifier');
  if (!id || id.length === 0 || !key || key.length === 0 || !rpId) return null;
  const handle = bytes(pick(credential, 'userHandle'));
  return {
    credentialId: b64url(id),
    keyType: 'public-key',
    keyAlgorithm: 'ECDSA',
    keyCurve: 'P-256',
    // PKCS#8, as the spec has it and Bitwarden keeps it.
    keyValue: b64url(key),
    rpId: rpId.toLowerCase(),
    rpName: null,
    userHandle: handle && handle.length > 0 ? b64url(handle) : null,
    userName: str(credential, 'username', 'userName'),
    userDisplayName: str(credential, 'userDisplayName'),
    // Importers start the counter at 0 (CXF 1.0).
    counter: '0',
    discoverable: 'true',
    creationDate: created,
  };
}

/** "2027-08" (CXF's year-month) → ["8", "2027"]. */
function yearMonth(raw: string | null): [string | null, string | null] {
  const match = raw?.match(/^(\d{4})-(\d{1,2})/);
  return match ? [String(Number(match[2])), match[1]!] : [null, null];
}

/** Every editable field of a credential, as custom fields: for kinds the vault has no place for. */
function fieldsOf(collector: Collector, item: ExportItem, credential: Json, prefix: string) {
  for (const [key, raw] of Object.entries(credential)) {
    if (key === 'type' || key === 'id') continue;
    const text = value(raw);
    if (blank(text)) continue;
    const concealed = isObject(raw) && raw.fieldType === 'concealed-string';
    collector.extra(item, `${prefix}: ${key}`, text, concealed ? FieldType.Hidden : undefined);
  }
}

/** The collections' items as folders: item id → "Collection/Sub". */
function folders(account: Json): Map<string, string> {
  const out = new Map<string, string>();
  // Nested at most this deep (folders deeper than that are no folders anyone keeps).
  const walk = (collection: unknown, path: string[], depth = 0) => {
    if (!isObject(collection) || depth > 32) return;
    const title = str(collection, 'title');
    const here = title ? [...path, title.replace(/\//g, '∕')] : path;
    for (const linked of list(collection.items)) {
      const id = isObject(linked) ? pick(linked, 'item') : linked;
      if (typeof id === 'string' && here.length && !out.has(id)) out.set(id, here.join('/'));
    }
    for (const sub of list(pick(collection, 'subCollections', 'subcollections')))
      walk(sub, here, depth + 1);
  };
  for (const collection of list(account.collections)) walk(collection, []);
  return out;
}

export type HandedOver = { data: BitwardenExport; warnings: string[]; format: string };

/** Reads what the system handed over. Throws an ImportError when it isn't CXF. */
export function readCredentialExchange(json: string): HandedOver {
  let root: unknown;
  try {
    root = JSON.parse(json);
  } catch {
    throw new ImportError(t('Die übergebenen Daten ließen sich nicht lesen.'));
  }
  if (!isObject(root) || !Array.isArray(root.accounts)) {
    throw new ImportError(t('Die übergebenen Daten ließen sich nicht lesen.'));
  }
  const collector = new Collector();
  const exporter = str(
    root,
    'exporterDisplayName',
    'exporterRpId',
    'exporterRelyingPartyIdentifier',
  );
  for (const account of root.accounts) {
    if (!isObject(account)) continue;
    const inFolder = folders(account);
    for (const raw of list(account.items)) {
      if (!isObject(raw)) continue;
      const title = str(raw, 'title') ?? '';
      collector.entry(title, () => readItem(collector, raw, title, inFolder));
    }
  }
  const { data, warnings } = collector.result();
  return { data, warnings, format: exporter ? `${exporter} (CXF)` : 'CXF' };
}

function readItem(collector: Collector, raw: Json, title: string, inFolder: Map<string, string>) {
  const id = typeof raw.id === 'string' ? raw.id : null;
  const folder = id ? (inFolder.get(id) ?? null) : null;
  const created = date(pick(raw, 'creationAt', 'created')) ?? new Date().toISOString();
  const favorite = raw.favorite === true;
  const credentials = list(raw.credentials).filter(isObject);
  const scope = isObject(raw.scope) ? raw.scope : {};
  const urls = list(scope.urls).filter((u): u is string => typeof u === 'string');
  const apps = list(pick(scope, 'androidApps'))
    .filter(isObject)
    .map((app) => str(app, 'bundleId', 'bundleID'))
    .filter((id): id is string => !!id)
    .map((id) => `androidapp://${id}`);

  const logins: (ExportItem & { login: NonNullable<ExportItem['login']> })[] = [];
  const login = () => {
    const made = collector.login(title);
    made.favorite = favorite;
    collector.uris(made, ...urls);
    for (const app of apps) made.login.uris.push({ uri: app, match: null });
    logins.push(made);
    return made;
  };
  /** The first login that still has room for `what`. */
  const roomFor = (what: 'password' | 'passkey' | 'totp') =>
    logins.find((l) =>
      what === 'passkey'
        ? !l.login.fido2Credentials?.length
        : what === 'totp'
          ? !l.login.totp
          : // A passkey of the same item may have come first and set the user name.
            l.login.password == null,
    ) ?? login();

  const notes: string[] = [];
  const others: Json[] = [];
  /** Cards, networks, identities: added after the logins, in the item's order. */
  const made: ExportItem[] = [];
  for (const credential of credentials) {
    switch (credential.type) {
      case 'basic-auth': {
        const target = roomFor('password');
        target.login.username = str(credential, 'username', 'userName') ?? target.login.username;
        target.login.password = value(pick(credential, 'password'));
        break;
      }
      case 'passkey': {
        const made = passkey(credential, created);
        if (!made) {
          collector.warn(t('Ein Passkey ließ sich nicht übernehmen.'));
          break;
        }
        const target = roomFor('passkey');
        target.login.fido2Credentials = [made];
        target.login.username ??= made.userName;
        if (!target.login.uris.length) collector.uris(target, made.rpId);
        break;
      }
      case 'totp': {
        const code = totp(credential, title);
        if (code) roomFor('totp').login.totp = code;
        break;
      }
      case 'note': {
        const text = str(credential, 'content');
        if (text) notes.push(text);
        break;
      }
      default:
        others.push(credential);
    }
  }

  for (const credential of others) {
    const kind = String(credential.type ?? '');
    if (kind === 'credit-card') {
      const card = collector.item(ItemType.Card, title);
      card.favorite = favorite;
      const number = str(credential, 'number');
      const [month, year] = yearMonth(str(credential, 'expiryDate'));
      card.card = {
        cardholderName: str(credential, 'fullName'),
        brand: str(credential, 'cardType') ?? cardBrand(number),
        number,
        expMonth: month,
        expYear: year,
        code: str(credential, 'verificationNumber'),
      };
      collector.field(card, 'PIN', str(credential, 'pin'), FieldType.Hidden);
      collector.field(card, t('Gültig ab'), str(credential, 'validFrom'));
      made.push(card);
    } else if (kind === 'wifi') {
      const wifi = collector.item(ItemType.Note, title);
      wifi.favorite = favorite;
      collector.wifi(wifi, {
        ssid: str(credential, 'ssid'),
        password: value(pick(credential, 'passphrase')),
        security: str(credential, 'networkSecurityType'),
        hidden: str(credential, 'hidden')?.toLowerCase() === 'true',
      });
      made.push(wifi);
    } else if (kind === 'address' || kind === 'person-name') {
      const person = collector.item(ItemType.Identity, title);
      person.favorite = favorite;
      const identity = person.identity!;
      if (kind === 'address') {
        identity.address1 = str(credential, 'streetAddress');
        identity.postalCode = str(credential, 'postalCode');
        identity.city = str(credential, 'city');
        identity.state = str(credential, 'territory');
        identity.country = str(credential, 'country');
        identity.phone = str(credential, 'tel', 'telephone');
      } else {
        identity.title = str(credential, 'title');
        identity.firstName = str(credential, 'given');
        identity.middleName = str(credential, 'given2');
        identity.lastName =
          [str(credential, 'surnamePrefix'), str(credential, 'surname')]
            .filter(Boolean)
            .join(' ') || null;
        if (!identity.firstName && !identity.lastName) {
          const [first, middle, last] = splitName(title);
          Object.assign(identity, { firstName: first, middleName: middle, lastName: last });
        }
      }
      made.push(person);
    } else if (kind === 'generated-password') {
      const target = logins[0] ?? login();
      if (target.login.password == null) target.login.password = str(credential, 'password');
      else
        collector.extra(
          target,
          t('Erzeugtes Passwort'),
          str(credential, 'password'),
          FieldType.Hidden,
        );
    } else if (kind === 'custom-fields') {
      const target = logins[0] ?? login();
      const label = str(credential, 'label');
      for (const field of list(credential.fields).filter(isObject)) {
        const name = [label, str(field, 'label')].filter(Boolean).join(': ') || t('Feld');
        const concealed = field.fieldType === 'concealed-string';
        collector.field(target, name, value(field.value), concealed ? FieldType.Hidden : undefined);
      }
    } else {
      // api-key, ssh-key, passport, drivers-license, identity-document, …: kept as fields.
      const target = logins[0] ?? login();
      if (kind === 'ssh-key') {
        const key = bytes(pick(credential, 'privateKey'));
        if (key) {
          collector.extra(
            target,
            t('Privater SSH-Schlüssel (PKCS#8, base64)'),
            base64(key),
            FieldType.Hidden,
          );
        }
        collector.extra(target, t('SSH-Schlüssel-Kommentar'), str(credential, 'keyComment'));
      } else {
        fieldsOf(collector, target, credential, kind || t('Feld'));
      }
    }
  }

  // Only a note (or nothing at all): a secure note. Otherwise the notes go with the first item.
  const all: ExportItem[] = [...logins, ...made];
  if (all.length === 0) {
    const note = collector.item(ItemType.Note, title);
    note.favorite = favorite;
    all.push(note);
  }
  collector.appendNote(all[0]!, notes.join('\n\n'));
  for (const item of all) collector.add(item, folder);
}
