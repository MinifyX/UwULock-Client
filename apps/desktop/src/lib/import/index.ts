/**
 * Moving in from another password manager — the web vault's import module (UwULock-Server
 * `web/src/lib/import`), shared as it is: change both together. Everything happens on this
 * device: the file is read here and turned into Bitwarden's unencrypted JSON export, the user
 * sees a preview, and only then does Rust (`import_vault`, `run.ts`) seal the items and send
 * them. The server never sees the file or anything in it unencrypted.
 *
 * Pure TypeScript without effects, run by Node in the tests (`apps/desktop/test/import-*`); what
 * needs Tauri is in `kdf.ts` and `run.ts`.
 */

import { N_, t } from './i18n.ts';
import { isProtectedBitwarden, readBitwardenCsv, readBitwardenJson } from './bitwarden.ts';
import { readAppleCsv, readChromeCsv, readFirefoxCsv } from './browsers.ts';
import { ImportError, parseJson, text } from './bytes.ts';
import { Collector, hasWifiMarker } from './collect.ts';
import { CsvTable } from './csv.ts';
import { detect, kindOf, sourceOfZip } from './detect.ts';
import { openKdbx, parseXml } from './kdbx.ts';
import { readKeepassCsv, readKeepassXml } from './keepass.ts';
import { readLastPassCsv } from './lastpass.ts';
import { checkFileSize } from './limits.ts';
import { read1PasswordCsv, read1pux } from './onepassword.ts';
import { encrypted, readProtonCsv, readProtonJson, type ProtonExport } from './protonpass.ts';
import {
  ItemType,
  type Credentials,
  type ExportItem,
  type ImportFile,
  type KdbxKdf,
  type Parsed,
  type Source,
} from './types.ts';
import { Zip } from './zip.ts';

export type { Credentials, ImportFile, KdbxKdf, Parsed, Source } from './types.ts';
export { ImportError } from './bytes.ts';
export { detect } from './detect.ts';

/** The apps to choose from, in the order the menu lists them. */
export const SOURCES: { value: Source; label: string }[] = [
  { value: 'bitwarden', label: 'Bitwarden, Vaultwarden, UwULock' },
  { value: 'keepass', label: 'KeePass, KeePassXC' },
  { value: '1password', label: '1Password' },
  { value: 'chrome', label: N_('Chrome, Edge und andere Chromium-Browser') },
  { value: 'firefox', label: 'Firefox' },
  { value: 'apple', label: N_('Apple Passwörter (Safari)') },
  { value: 'protonpass', label: 'Proton Pass' },
  { value: 'lastpass', label: 'LastPass' },
];

/** What a source is called, for the preview: also one that is not in the menu. */
export function sourceLabel(source: Source): string {
  if (source === 'credential-exchange') return t('Andere App (Credential Exchange)');
  return t(SOURCES.find((s) => s.value === source)?.label ?? source);
}

/** Whether the file is a KeePass database, which needs its password (and key file) to open. */
export function needsPassword(bytes: Uint8Array): boolean {
  return kindOf(bytes) === 'kdbx';
}

/**
 * Whether the file is Bitwarden's JSON export with a password of its own: Rust opens it with
 * that password (`import_open_bitwarden`), and what comes out is read like any other export.
 */
export function isProtectedExport(bytes: Uint8Array): boolean {
  if (kindOf(bytes) !== 'json') return false;
  try {
    return isProtectedBitwarden(parseJson(bytes));
  } catch {
    return false;
  }
}

type Options = { credentials?: Credentials; kdf?: KdbxKdf };

/**
 * Reads `file` as an export of `source` ('auto': whatever it looks like). Throws an
 * `ImportError` with a message for the user when the file doesn't fit or can't be opened; never
 * the raw error of something that broke on the file's content, which could show part of it.
 */
export async function readImport(
  file: ImportFile,
  source: Source | 'auto',
  options: Options = {},
): Promise<Parsed> {
  try {
    return await readFile(file, source, options);
  } catch (error) {
    // The WebAssembly module's failures (plain objects, not Errors) are worded by `errorText`.
    if (error instanceof ImportError || !(error instanceof Error)) throw error;
    throw new ImportError(
      t('Die Datei ließ sich nicht lesen: Sie ist beschädigt oder anders aufgebaut als erwartet.'),
    );
  }
}

async function readFile(
  file: ImportFile,
  source: Source | 'auto',
  options: Options,
): Promise<Parsed> {
  checkFileSize(file.bytes.length);
  const chosen = source === 'auto' ? detect(file.bytes) : source;
  if (!chosen) {
    throw new ImportError(
      t('UwULock erkennt nicht, aus welcher App diese Datei kommt. Wähl die App bitte aus.'),
    );
  }
  const kind = kindOf(file.bytes);
  const collector = new Collector();
  const wrong = () =>
    new ImportError(
      t('Diese Datei ist kein Export, den UwULock von {app} kennt.', {
        app: sourceLabel(chosen),
      }),
    );
  const json = (bytes = file.bytes) => parseJson(bytes);
  const done = (format: string, submit?: Parsed['submit']): Parsed => {
    const { data, warnings } = collector.result();
    submit ??= { format: 'json', text: JSON.stringify(data) };
    return { source: chosen, format, data, warnings, submit };
  };
  const csv = (read: (table: CsvTable, collector: Collector) => void) => {
    if (kind !== 'csv') throw wrong();
    read(new CsvTable(text(file.bytes)), collector);
    return done('CSV');
  };

  switch (chosen) {
    case 'bitwarden': {
      if (kind === 'json') {
        const data = readBitwardenJson(json());
        return {
          source: chosen,
          format: 'JSON',
          data,
          warnings: [],
          submit: { format: 'json', text: text(file.bytes) },
        };
      }
      if (kind !== 'csv') throw wrong();
      readBitwardenCsv(new CsvTable(text(file.bytes)), collector);
      return done('CSV', { format: 'csv', text: text(file.bytes) });
    }
    case 'keepass': {
      if (kind === 'kdbx') {
        if (!options.credentials || !options.kdf) {
          throw new ImportError(t('Für diese Datei braucht es ihr Passwort.'));
        }
        const { doc, version } = await openKdbx(file.bytes, options.credentials, options.kdf);
        readKeepassXml(doc, collector);
        return done(version);
      }
      if (kind === 'xml') {
        const doc = parseXml(text(file.bytes));
        if (!doc) throw wrong();
        readKeepassXml(doc, collector);
        return done('XML');
      }
      return csv(readKeepassCsv);
    }
    case '1password': {
      if (kind === 'zip') {
        await read1pux(new Zip(file.bytes), collector);
        return done('1PUX');
      }
      if (kind === 'json') {
        // export.data, taken out of the .1pux by hand.
        await read1pux({ names: ['export.data'], read: async () => file.bytes }, collector);
        return done('1PUX');
      }
      return csv(read1PasswordCsv);
    }
    case 'protonpass': {
      if (kind === 'pgp') throw encrypted();
      if (kind === 'zip') {
        const zip = new Zip(file.bytes);
        if (sourceOfZip(zip) !== 'protonpass') throw wrong();
        if (zip.names.some((name) => name.endsWith('.pgp'))) throw encrypted();
        const name = zip.names.find((n) => /(^|\/)data\.json$/.test(n))!;
        readProtonJson(json(await zip.read(name)) as ProtonExport, collector);
        const files = zip.names.filter((n) => /(^|\/)files\/./.test(n)).length;
        if (files) collector.attachment(files);
        return done('ZIP');
      }
      if (kind === 'json') {
        readProtonJson(json() as ProtonExport, collector);
        return done('JSON');
      }
      return csv(readProtonCsv);
    }
    case 'chrome':
      return csv(readChromeCsv);
    case 'firefox':
      return csv(readFirefoxCsv);
    case 'apple':
      return csv(readAppleCsv);
    case 'lastpass':
      return csv(readLastPassCsv);
    case 'credential-exchange':
      // Not a file: the system hands these over (`run.ts`, `parsedFromCollected`).
      throw wrong();
  }
}

/** One line of the preview list. */
export type PreviewItem = {
  name: string;
  type: ItemType;
  /** The user name, the card holder, the person: whatever tells two items apart. */
  detail: string;
  folder: string | null;
  totp: boolean;
  /** A Wi-Fi network: a note with UwULock's marker (docs/wifi.md). */
  wifi: boolean;
};

export type Summary = {
  logins: number;
  notes: number;
  cards: number;
  identities: number;
  sshKeys: number;
  wifi: number;
  folders: string[];
  items: PreviewItem[];
};

export function summarize(parsed: Parsed): Summary {
  const folders = new Map(parsed.data.folders.map((folder) => [folder.id, folder.name]));
  const count = (type: ItemType) => items.filter((item) => item.type === type && !item.wifi).length;
  const detail = (item: ExportItem) =>
    item.login?.username ??
    item.card?.cardholderName ??
    [item.identity?.firstName, item.identity?.lastName].filter(Boolean).join(' ') ??
    '';
  const items: PreviewItem[] = parsed.data.items
    .filter((item) => [1, 2, 3, 4, 5].includes(item.type))
    .map((item) => ({
      name: item.name,
      type: item.type,
      detail: detail(item) || '',
      folder: (item.folderId && folders.get(item.folderId)) || null,
      totp: Boolean(item.login?.totp),
      wifi: item.type === ItemType.Note && hasWifiMarker(item),
    }));
  return {
    logins: count(ItemType.Login),
    notes: count(ItemType.Note),
    cards: count(ItemType.Card),
    identities: count(ItemType.Identity),
    sshKeys: count(ItemType.SshKey),
    wifi: items.filter((item) => item.wifi).length,
    folders: parsed.data.folders.map((folder) => folder.name),
    items,
  };
}
