/**
 * The item editor's form, apart from how it is drawn: what the editor holds
 * for each kind, the form of an item as Rust hands it over, and the draft it
 * sends back. The desktop's editor (ItemEditor.tsx) and the phone's
 * (mobile/ItemEdit.tsx) share it.
 */

import type { Draft, FieldKind, ItemDetail as Detail, ItemKind, ItemSummary } from './api';
import { IDENTITY_FIELDS } from './items';
import { isEnterprise, readWifi, wifiFields, type WifiView } from './wifi';

/**
 * A value the editor may not have: a password, a card number, a hidden field.
 * `keep` means the item's own value stays — the editor never saw it, and it
 * never goes through the window unless someone asks to see it.
 */
export type Sec = { mode: 'keep' | 'value'; value: string };

export const keep = (has: boolean): Sec =>
  has ? { mode: 'keep', value: '' } : { mode: 'value', value: '' };
export const sent = (secret: Sec): string | null => (secret.mode === 'keep' ? null : secret.value);

export type UriRow = { key: number; uri: string; match: number | null };
export type FieldRow = {
  key: number;
  name: string;
  kind: FieldKind;
  value: Sec;
  /** Which field of the item this row was, for the value and the link. */
  from: number | null;
};

/** A Wi-Fi network's own fields; the editor's other fields are the item's other ones. */
export type WifiForm = {
  ssid: string;
  password: Sec;
  security: string;
  hidden: boolean;
  eap: string;
  phase2: string;
  identity: string;
  anonymous: string;
  ca: string;
  from: WifiView['from'];
};

export type Form = {
  name: string;
  folderId: string;
  favorite: boolean;
  reprompt: boolean;
  notes: string;
  username: string;
  password: Sec;
  totp: Sec;
  uris: UriRow[];
  cardholderName: string;
  brand: string;
  number: Sec;
  expMonth: string;
  expYear: string;
  code: Sec;
  identity: Record<string, string>;
  identitySecrets: Record<string, Sec>;
  privateKey: Sec;
  publicKey: string;
  fingerprint: string;
  fields: FieldRow[];
  wifi: WifiForm;
};

export function emptyForm(kind: ItemKind): Form {
  return {
    name: '',
    folderId: '',
    favorite: false,
    reprompt: false,
    notes: '',
    username: '',
    password: keep(false),
    totp: keep(false),
    uris: kind === 'login' ? [{ key: 1, uri: '', match: null }] : [],
    cardholderName: '',
    brand: '',
    number: keep(false),
    expMonth: '',
    expYear: '',
    code: keep(false),
    identity: {},
    identitySecrets: {},
    privateKey: keep(false),
    publicKey: '',
    fingerprint: '',
    fields: [],
    wifi: {
      ssid: '',
      password: keep(false),
      security: 'WPA2',
      hidden: false,
      eap: 'PEAP',
      phase2: 'MSCHAPV2',
      identity: '',
      anonymous: '',
      ca: '',
      from: {},
    },
  };
}

export function fieldRow(field: NonNullable<Detail['fields']>[number]): FieldRow {
  return {
    key: field.index + 1,
    name: field.name ?? '',
    kind: field.kind,
    value:
      field.kind === 'hidden' ? keep(field.hasValue) : { mode: 'value', value: field.value ?? '' },
    from: field.index,
  };
}

/** The item as it is now, as far as the page is allowed to know it. */
export function formOf(summary: ItemSummary, detail: Detail): Form {
  const form = emptyForm(summary.kind);
  form.name = summary.name;
  form.folderId = summary.folderId ?? '';
  form.favorite = summary.favorite;
  form.reprompt = summary.reprompt;
  form.notes = detail.notes ?? '';
  if (detail.login) {
    form.username = detail.login.username ?? '';
    form.password = keep(detail.login.hasPassword);
    form.totp = keep(detail.login.hasTotp);
    form.uris = detail.login.uris.map((uri, index) => ({
      key: index + 1,
      uri: uri.uri,
      match: uri.match,
    }));
  }
  if (detail.card) {
    form.cardholderName = detail.card.cardholderName ?? '';
    form.brand = detail.card.brand ?? '';
    form.number = keep(Boolean(detail.card.numberEnding));
    form.expMonth = detail.card.expMonth ?? '';
    form.expYear = detail.card.expYear ?? '';
    form.code = keep(detail.card.hasCode);
  }
  for (const field of IDENTITY_FIELDS) {
    const entry = detail.identity?.find((e) => e.name === field.name);
    if (field.sensitive) form.identitySecrets[field.name] = keep(Boolean(entry));
    else form.identity[field.name] = entry?.value ?? '';
  }
  if (detail.sshKey) {
    form.privateKey = keep(detail.sshKey.hasPrivateKey);
    form.publicKey = detail.sshKey.publicKey ?? '';
    form.fingerprint = detail.sshKey.fingerprint ?? '';
  }
  if (summary.kind === 'wifi') {
    const wifi = readWifi(detail.fields ?? []);
    const password = wifi.password;
    form.wifi = {
      ssid: wifi.ssid,
      password: !password
        ? keep(false)
        : password.kind === 'hidden'
          ? keep(password.hasValue)
          : { mode: 'value', value: password.value ?? '' },
      security: wifi.security || 'WPA2',
      hidden: wifi.hidden,
      // An Enterprise network without a method gets the usual one shown, and saved.
      eap: wifi.eap || (isEnterprise(wifi.security) ? '' : 'PEAP'),
      phase2: wifi.phase2 || (isEnterprise(wifi.security) ? '' : 'MSCHAPV2'),
      identity: wifi.identity,
      anonymous: wifi.anonymous,
      ca: wifi.ca,
      from: wifi.from,
    };
    form.fields = wifi.others.map(fieldRow);
    return form;
  }
  form.fields = (detail.fields ?? []).map(fieldRow);
  return form;
}

export function draftOf(form: Form, kind: ItemKind): Draft {
  const draft: Draft = {
    // A Wi-Fi network is a secure note to the vault.
    kind: kind === 'wifi' ? 'note' : kind,
    name: form.name.trim(),
    notes: form.notes,
    favorite: form.favorite,
    reprompt: form.reprompt,
    folderId: form.folderId || null,
    fields: form.fields.map((field) => ({
      name: field.name.trim() || null,
      kind: field.kind,
      value: field.kind === 'linked' ? null : sent(field.value),
      from: field.from,
    })),
  };
  if (kind === 'login') {
    draft.login = {
      username: form.username,
      password: sent(form.password),
      totp: sent(form.totp),
      uris: form.uris
        .filter((uri) => uri.uri.trim())
        .map((uri) => ({ uri: uri.uri.trim(), match: uri.match })),
    };
  }
  if (kind === 'card') {
    draft.card = {
      cardholderName: form.cardholderName,
      brand: form.brand,
      number: sent(form.number),
      expMonth: form.expMonth,
      expYear: form.expYear,
      code: sent(form.code),
    };
  }
  if (kind === 'identity') {
    const values: Record<string, string> = {};
    for (const field of IDENTITY_FIELDS) {
      if (field.sensitive) {
        const secret = form.identitySecrets[field.name];
        if (secret && secret.mode === 'value') values[field.name] = secret.value;
      } else {
        values[field.name] = form.identity[field.name] ?? '';
      }
    }
    draft.identity = values;
  }
  if (kind === 'wifi') {
    const wifi = form.wifi;
    draft.fields = wifiFields(
      {
        ssid: wifi.ssid.trim(),
        password: sent(wifi.password),
        security: wifi.security,
        hidden: wifi.hidden,
        eap: wifi.eap,
        phase2: wifi.phase2,
        identity: wifi.identity,
        anonymous: wifi.anonymous,
        ca: wifi.ca,
        from: wifi.from,
      },
      draft.fields,
    );
  }
  if (kind === 'ssh-key') {
    draft.sshKey = {
      privateKey: sent(form.privateKey),
      publicKey: form.publicKey,
      fingerprint: form.fingerprint,
    };
  }
  return draft;
}
