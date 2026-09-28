/**
 * Writing values into a page's fields so the page notices: the value goes in through the
 * browser's own setter (a React or Vue input keeps its own copy of the value and compares it
 * with the field's when an `input` event arrives), followed by the events a person typing
 * would cause. Read-only, disabled and invisible fields are left alone.
 *
 * Values are passed in and written into fields; nothing here keeps them.
 */

import type { CardValues, IdentityValues, LoginValues } from '../shared/protocol';
import {
  isFillable,
  type CardFields,
  type Control,
  type IdentityFields,
  type IdentityRole,
  type LoginFields,
  type TotpFields,
} from './forms';

function prototypeOf(el: Control): object {
  if (el instanceof HTMLTextAreaElement) return HTMLTextAreaElement.prototype;
  if (el instanceof HTMLSelectElement) return HTMLSelectElement.prototype;
  return HTMLInputElement.prototype;
}

/** Sets `value` past any setter the page put on the element itself. */
export function setNativeValue(el: Control, value: string) {
  const setter = Object.getOwnPropertyDescriptor(prototypeOf(el), 'value')?.set;
  if (setter) setter.call(el, value);
  else el.value = value;
}

function fire(el: Element, type: string) {
  const init = { bubbles: true, composed: true, cancelable: type.startsWith('key') };
  let event: Event;
  if (type === 'input') {
    event = new InputEvent('input', { ...init, inputType: 'insertReplacementText' });
  } else if (type.startsWith('key')) {
    event = new KeyboardEvent(type, init);
  } else {
    event = new Event(type, init);
  }
  el.dispatchEvent(event);
}

/** Writes one value the way a person typing (or pasting) it would. False if the field refused. */
export function fillField(el: Control, value: string): boolean {
  if (!isFillable(el)) return false;
  if (el instanceof HTMLSelectElement) return fillSelect(el, [value]);
  el.focus({ preventScroll: true });
  fire(el, 'keydown');
  setNativeValue(el, value);
  fire(el, 'input');
  fire(el, 'keyup');
  fire(el, 'change');
  el.blur();
  return true;
}

// ── Selects ───────────────────────────────────────────────

const normalize = (text: string) => text.normalize('NFD').replace(/[̀-ͯ]/g, '').trim().toLowerCase();

/** The first option whose value or text equals one of `wanted` (case and accents ignored). */
export function findOption(
  select: HTMLSelectElement,
  wanted: string[],
  loose?: (option: HTMLOptionElement) => boolean,
): HTMLOptionElement | null {
  const wants = new Set(wanted.filter(Boolean).map(normalize));
  const options = Array.from(select.options).filter((o) => !o.disabled);
  return (
    options.find((o) => wants.has(normalize(o.value)) || wants.has(normalize(o.text))) ??
    (loose ? (options.find(loose) ?? null) : null)
  );
}

function choose(select: HTMLSelectElement, option: HTMLOptionElement | null): boolean {
  if (!option || !isFillable(select)) return false;
  select.focus({ preventScroll: true });
  setNativeValue(select, option.value);
  if (select.value !== option.value) option.selected = true;
  fire(select, 'input');
  fire(select, 'change');
  select.blur();
  return true;
}

export function fillSelect(select: HTMLSelectElement, wanted: string[]): boolean {
  return choose(select, findOption(select, wanted));
}

const MONTHS: string[][] = [
  ['jan', 'januar', 'january', 'jän', 'jänner'],
  ['feb', 'februar', 'february'],
  ['mar', 'mär', 'märz', 'march', 'maerz'],
  ['apr', 'april'],
  ['may', 'mai'],
  ['jun', 'juni', 'june'],
  ['jul', 'juli', 'july'],
  ['aug', 'august'],
  ['sep', 'sept', 'september'],
  ['oct', 'okt', 'oktober', 'october'],
  ['nov', 'november'],
  ['dec', 'dez', 'dezember', 'december'],
];

/** `"3"`, `"03"`, `"März"` → 3; null when it isn't a month. */
function monthNumber(value: string | null): number | null {
  if (!value) return null;
  const number = Number.parseInt(value, 10);
  if (number >= 1 && number <= 12) return number;
  const text = normalize(value);
  const index = MONTHS.findIndex((names) => names.some((name) => normalize(name) === text));
  return index >= 0 ? index + 1 : null;
}

function selectMonth(select: HTMLSelectElement, month: number): boolean {
  const two = String(month).padStart(2, '0');
  const names = MONTHS[month - 1] ?? [];
  const startsWithMonth = (o: HTMLOptionElement) => {
    const text = normalize(o.text);
    return (
      new RegExp(`^0?${month}\\b`).test(text) ||
      names.some((name) => text.startsWith(normalize(name)))
    );
  };
  return choose(select, findOption(select, [two, String(month), ...names], startsWithMonth));
}

/** `"27"` → `"2027"`. */
function fullYear(year: string): string {
  const digits = year.trim();
  return /^\d{2}$/.test(digits) ? `20${digits}` : digits;
}

function selectYear(select: HTMLSelectElement, year: string): boolean {
  const full = fullYear(year);
  return fillSelect(select, [full, full.slice(-2)]);
}

const countryNames = (() => {
  const make = (language: string) => {
    try {
      return new Intl.DisplayNames([language], { type: 'region' });
    } catch {
      return null;
    }
  };
  const names = [make('de'), make('en')];
  return (code: string): string[] => {
    const found: string[] = [];
    for (const displayNames of names) {
      try {
        const name = displayNames?.of(code.toUpperCase());
        if (name && name.toUpperCase() !== code.toUpperCase()) found.push(name);
      } catch {
        // not a region code
      }
    }
    return found;
  };
})();

/** A country, given as a code (`DE`) or a name (`Deutschland`, `Germany`). */
function selectCountry(select: HTMLSelectElement, country: string): boolean {
  const wanted = /^[a-z]{2}$/i.test(country) ? [country, ...countryNames(country)] : [country];
  const byCode = (o: HTMLOptionElement) =>
    /^[a-z]{2}$/i.test(o.value) &&
    countryNames(o.value).some((name) => normalize(name) === normalize(country));
  return choose(select, findOption(select, wanted, byCode));
}

// ── Logins ────────────────────────────────────────────────

export type LoginFillResult = { filled: boolean; totpFilled: boolean };

/**
 * The username (unless the field already says so) and the password of a login form — for a
 * change-password form the current one, never the new ones — and the one-time code.
 */
export function fillLogin(
  login: LoginFields | null,
  totp: TotpFields | null,
  values: LoginValues,
): LoginFillResult {
  let filled = false;
  if (login) {
    const { username, password } = login;
    if (username && values.username) {
      filled =
        (username.value === values.username && isFillable(username)) ||
        fillField(username, values.username) ||
        filled;
    }
    if (password && values.password && login.kind !== 'signup') {
      filled = fillField(password, values.password) || filled;
    }
  }
  const totpFilled = !!(totp && values.totp && fillTotp(totp, values.totp));
  return { filled: filled || totpFilled, totpFilled };
}

/** One field gets the code, boxes get one character each. */
export function fillTotp(totp: TotpFields, code: string): boolean {
  const clean = code.replace(/\s+/g, '');
  if (totp.inputs.length === 1) return fillField(totp.inputs[0]!, clean);
  let filled = false;
  totp.inputs.forEach((input, index) => {
    const char = clean[index];
    if (char !== undefined) filled = fillField(input, char) || filled;
  });
  return filled;
}

// ── Cards ─────────────────────────────────────────────────

/** `MM/YY`, `MM/YYYY`, `MMYY`… as the field's placeholder or length asks. */
export function formatExpiry(el: HTMLInputElement, month: string, year: string): string {
  const mm = String(monthNumber(month) ?? month).padStart(2, '0');
  const yyyy = fullYear(year);
  const yy = yyyy.slice(-2);
  const hint = `${el.placeholder} ${el.getAttribute('aria-label') ?? ''}`.toLowerCase();
  const max = el.maxLength > 0 ? el.maxLength : Infinity;

  const pattern = /(mm|\d\d)(\s*[/.-]?\s*)(yyyy|jjjj|aaaa|yy|jj|aa|\d{4}|\d\d)\b/.exec(hint);
  if (pattern) {
    const separator = pattern[2] ?? '/';
    const long = pattern[3]!.length === 4;
    const value = `${mm}${separator}${long ? yyyy : yy}`;
    if (value.length <= max) return value;
  }
  if (max === 4) return `${mm}${yy}`;
  if (max === 6) return `${mm}${yyyy}`;
  if (max >= 7 && max !== Infinity) return `${mm}/${yyyy}`;
  return `${mm}/${yy}`;
}

export function fillCard(card: CardFields, values: CardValues): boolean {
  const { fields } = card;
  let filled = false;
  const put = (el: Control | undefined, value: string | null) => {
    if (el && value) filled = fillField(el, value) || filled;
  };
  put(fields.number, values.number?.replace(/[\s-]/g, '') ?? null);
  put(fields.name, values.cardholderName);
  put(fields.code, values.code);
  if (fields.brand && values.brand) {
    if (fields.brand instanceof HTMLSelectElement) {
      const brand = values.brand;
      const wanted = brand.toLowerCase() === 'amex' ? [brand, 'American Express'] : [brand];
      const loose = (o: HTMLOptionElement) =>
        normalize(o.text).includes(normalize(brand)) || normalize(o.value) === normalize(brand);
      filled = choose(fields.brand, findOption(fields.brand, wanted, loose)) || filled;
    } else put(fields.brand, values.brand);
  }

  const month = monthNumber(values.expMonth);
  const year = values.expYear?.trim() || null;
  if (fields.expiry && month && year) {
    const el = fields.expiry;
    if (el instanceof HTMLInputElement && el.type === 'month') {
      put(el, `${fullYear(year)}-${String(month).padStart(2, '0')}`);
    } else if (el instanceof HTMLInputElement) {
      put(el, formatExpiry(el, String(month), year));
    }
  }
  if (fields.expMonth && month) {
    const el = fields.expMonth;
    if (el instanceof HTMLSelectElement) filled = selectMonth(el, month) || filled;
    else put(el, String(month).padStart(2, '0'));
  }
  if (fields.expYear && year) {
    const el = fields.expYear;
    if (el instanceof HTMLSelectElement) filled = selectYear(el, year) || filled;
    else if (el instanceof HTMLInputElement && el.maxLength === 2)
      put(el, fullYear(year).slice(-2));
    else put(el, fullYear(year));
  }
  return filled;
}

// ── Identities ────────────────────────────────────────────

function identityValue(role: IdentityRole, v: IdentityValues, multiline: boolean): string | null {
  const join = (parts: (string | null)[], separator: string) =>
    parts.filter((part) => part && part.trim()).join(separator) || null;
  switch (role) {
    case 'name':
      return join([v.firstName, v.middleName, v.lastName], ' ');
    case 'address':
      return join([v.address1, v.address2, v.address3], multiline ? '\n' : ', ');
    case 'title':
      return v.title;
    default:
      return v[role];
  }
}

export function fillIdentity(identity: IdentityFields, values: IdentityValues): boolean {
  let filled = false;
  for (const [role, el] of Object.entries(identity.fields) as [IdentityRole, Control][]) {
    const value = identityValue(role, values, el instanceof HTMLTextAreaElement);
    if (!value) continue;
    if (el instanceof HTMLSelectElement) {
      const done = role === 'country' ? selectCountry(el, value) : fillSelect(el, [value]);
      filled = done || filled;
    } else {
      filled = fillField(el, value) || filled;
    }
  }
  return filled;
}
