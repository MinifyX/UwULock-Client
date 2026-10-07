/**
 * What every importer makes: Bitwarden's unencrypted JSON export, which Rust reads, seals and
 * sends (`import_vault`, uwulock-core's `import`; the web vault's is web/wasm/src/transfer.rs in
 * UwULock-Server). Only the fields that import honours are here.
 */

export type Source =
  | 'bitwarden'
  | 'keepass'
  | '1password'
  | 'chrome'
  | 'firefox'
  | 'apple'
  | 'protonpass'
  | 'lastpass'
  /** Handed over by the system (Apple's Credential Exchange), not read from a file. */
  | 'credential-exchange';

export const ItemType = { Login: 1, Note: 2, Card: 3, Identity: 4, SshKey: 5 } as const;
export type ItemType = (typeof ItemType)[keyof typeof ItemType];

export const FieldType = { Text: 0, Hidden: 1, Boolean: 2 } as const;
export type FieldType = (typeof FieldType)[keyof typeof FieldType];

export type ExportField = { name: string; value: string; type: FieldType };

/**
 * A passkey as Bitwarden's JSON export has it, every value plain (Rust seals all but
 * `creationDate`). Binary values are base64url without padding (`credentialId` may also be a
 * UUID, as Bitwarden writes its own); `keyValue` is the PKCS#8 private key.
 */
export type ExportPasskey = {
  credentialId: string;
  keyType: 'public-key';
  keyAlgorithm: 'ECDSA';
  keyCurve: 'P-256';
  keyValue: string;
  rpId: string;
  rpName: string | null;
  userHandle: string | null;
  userName: string | null;
  userDisplayName: string | null;
  counter: string;
  discoverable: string;
  creationDate: string;
};

export type ExportLogin = {
  username: string | null;
  password: string | null;
  totp: string | null;
  uris: { uri: string; match: number | null }[];
  fido2Credentials?: ExportPasskey[];
};

export type ExportCard = {
  cardholderName: string | null;
  brand: string | null;
  number: string | null;
  expMonth: string | null;
  expYear: string | null;
  code: string | null;
};

export type ExportIdentity = {
  title: string | null;
  firstName: string | null;
  middleName: string | null;
  lastName: string | null;
  address1: string | null;
  address2: string | null;
  address3: string | null;
  city: string | null;
  state: string | null;
  postalCode: string | null;
  country: string | null;
  company: string | null;
  email: string | null;
  phone: string | null;
  ssn: string | null;
  username: string | null;
  passportNumber: string | null;
  licenseNumber: string | null;
};

export type ExportSshKey = { privateKey: string; publicKey: string; keyFingerprint: string };

export type ExportItem = {
  type: ItemType;
  name: string;
  notes: string | null;
  favorite: boolean;
  reprompt: 0 | 1;
  folderId: string | null;
  fields: ExportField[];
  login?: ExportLogin;
  secureNote?: { type: 0 };
  card?: ExportCard;
  identity?: ExportIdentity;
  sshKey?: ExportSshKey;
  passwordHistory?: { password: string; lastUsedDate: string }[];
};

export type ExportFolder = { id: string; name: string };

export type BitwardenExport = { encrypted: false; folders: ExportFolder[]; items: ExportItem[] };

/** An import, read and ready: what the preview shows, and what goes to the vault. */
export type Parsed = {
  source: Source;
  /** The file's format, for the preview: "KDBX 4.1", "CSV", … */
  format: string;
  data: BitwardenExport;
  /** Things the user should know before importing, in their language. */
  warnings: string[];
  /**
   * What `import_vault` gets. Bitwarden's own CSV goes as it is: Rust reads it (and undoes its
   * guards against spreadsheet formulas) better than a conversion would.
   */
  submit: { format: 'json' | 'csv'; text: string };
};

/** How a KeePass file is opened: its password and key file, and the slow key derivations. */
export type KdbxKdf = {
  /** Argon2d (`id` false) or Argon2id; `memoryKiB` in KiB. Gives 32 bytes. */
  argon2(
    id: boolean,
    version: number,
    key: Uint8Array,
    salt: Uint8Array,
    memoryKiB: number,
    iterations: number,
    lanes: number,
  ): Promise<Uint8Array> | Uint8Array;
  /** AES-KDF: `key` encrypted `rounds` times under `seed`, then its SHA-256. */
  aesKdf(key: Uint8Array, seed: Uint8Array, rounds: number): Promise<Uint8Array> | Uint8Array;
};

export type ImportFile = { name: string; bytes: Uint8Array };

export type Credentials = { password: string; keyFile?: Uint8Array | null };
