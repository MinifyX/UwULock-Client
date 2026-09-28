import { beforeEach, describe, expect, it } from 'vitest';
import {
  fillCard,
  fillField,
  fillIdentity,
  fillLogin,
  fillTotp,
  formatExpiry,
} from '../src/content/fill';
import {
  findCardFields,
  findIdentityFields,
  findLoginFields,
  findTotpFields,
} from '../src/content/forms';
import type { CardValues, IdentityValues, LoginValues } from '../src/shared/protocol';
import { load } from './fixtures/load';

const $ = <T extends Element = HTMLInputElement>(selector: string) =>
  document.querySelector(selector) as T;

/** Records the events a field gets, in order. */
function record(el: Element): string[] {
  const events: string[] = [];
  for (const type of ['focus', 'keydown', 'input', 'keyup', 'change', 'blur']) {
    el.addEventListener(type, (event) => {
      events.push(event.bubbles || type === 'focus' || type === 'blur' ? type : `${type}!`);
    });
  }
  return events;
}

/**
 * Like React: the element gets its own `value` property that remembers what the page last set;
 * an `input` event only counts as a change when the field's real value differs from that.
 */
function reactLike(input: HTMLInputElement) {
  const proto = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!;
  let tracked = input.value;
  Object.defineProperty(input, 'value', {
    configurable: true,
    get() {
      return proto.get!.call(this);
    },
    set(value: string) {
      tracked = value;
      proto.set!.call(this, value);
    },
  });
  const changes: string[] = [];
  input.addEventListener('input', () => {
    const now = input.value;
    if (now !== tracked) {
      tracked = now;
      changes.push(now);
    }
  });
  return changes;
}

const login = (values: Partial<LoginValues>): LoginValues => ({
  kind: 'login',
  username: null,
  password: null,
  totp: null,
  ...values,
});

beforeEach(() => {
  document.body.innerHTML = '';
});

describe('fields', () => {
  it('writes through the native setter and fires input and change', () => {
    load('login');
    const user = $('#user');
    const events = record(user);
    const changes = reactLike(user);
    expect(fillField(user, 'anna')).toBe(true);
    expect(user.value).toBe('anna');
    expect(changes).toEqual(['anna']);
    expect(events).toEqual(['focus', 'keydown', 'input', 'keyup', 'change', 'blur']);
  });

  it('events are composed, so they leave shadow roots', () => {
    const host = document.createElement('div');
    document.body.append(host);
    const shadow = host.attachShadow({ mode: 'open' });
    shadow.innerHTML = '<input>';
    const seen: string[] = [];
    host.addEventListener('input', () => seen.push('input'));
    host.addEventListener('change', () => seen.push('change'));
    fillField(shadow.querySelector('input')!, 'x');
    expect(seen).toEqual(['input', 'change']);
  });

  it('skips read-only, disabled and hidden fields', () => {
    document.body.innerHTML =
      '<input id="r" readonly><input id="d" disabled><input id="h" style="display:none">';
    for (const id of ['r', 'd', 'h']) {
      expect(fillField($(`#${id}`), 'x')).toBe(false);
      expect($(`#${id}`).value).toBe('');
    }
  });
});

describe('logins', () => {
  it('fills username and password', () => {
    load('login');
    const [fields] = findLoginFields(document);
    const result = fillLogin(fields!, null, login({ username: 'anna', password: 'geheim' }));
    expect(result).toEqual({ filled: true, totpFilled: false });
    expect($('#user').value).toBe('anna');
    expect($('#pass').value).toBe('geheim');
  });

  it('leaves a username that is already right alone', () => {
    load('login');
    $('#user').value = 'anna';
    const events = record($('#user'));
    fillLogin(findLoginFields(document)[0]!, null, login({ username: 'anna', password: 'p' }));
    expect(events).toEqual([]);
  });

  it('fills only the current password of a change-password form', () => {
    load('change-password');
    fillLogin(findLoginFields(document)[0]!, null, login({ username: 'anna', password: 'alt' }));
    expect($('#old').value).toBe('alt');
    expect($('#new').value).toBe('');
    expect($('#confirm').value).toBe('');
  });

  it('never puts a login password into a sign-up form', () => {
    load('signup');
    fillLogin(
      findLoginFields(document)[0]!,
      null,
      login({ username: 'a@example.com', password: 'p' }),
    );
    expect($('#pw1').value).toBe('');
    expect($('#pw2').value).toBe('');
    expect($('#mail').value).toBe('a@example.com');
  });

  it('fills a code into one field or into six boxes', () => {
    load('totp');
    const [single, split] = findTotpFields(document);
    const result = fillLogin(null, single!, login({ totp: '123456' }));
    expect(result).toEqual({ filled: true, totpFilled: true });
    expect($('#otp').value).toBe('123456');
    expect(fillTotp(split!, '987 654')).toBe(true);
    expect(split!.inputs.map((input) => input.value).join('')).toBe('987654');
  });
});

describe('cards', () => {
  const card: CardValues = {
    kind: 'card',
    cardholderName: 'Anna Beispiel',
    brand: 'Visa',
    number: '4111 1111 1111 1111',
    expMonth: '3',
    expYear: '2027',
    code: '123',
  };

  it('fills number, name, code and the expiry selects', () => {
    load('checkout');
    const changes: string[] = [];
    $('#month').addEventListener('change', () => changes.push('month'));
    expect(fillCard(findCardFields(document)[0]!, card)).toBe(true);
    expect($('#holder').value).toBe('Anna Beispiel');
    expect($('#cc').value).toBe('4111111111111111');
    expect($('#cvc').value).toBe('123');
    expect($<HTMLSelectElement>('#month').value).toBe('3');
    expect($<HTMLSelectElement>('#year').value).toBe('2027');
    expect(changes).toEqual(['month']);
  });

  it('matches selects by two-digit values and short years', () => {
    document.body.innerHTML = `<form>
      <input autocomplete="cc-number" id="n">
      <select autocomplete="cc-exp-month" id="m"><option value="01">01</option><option value="03">03</option></select>
      <select autocomplete="cc-exp-year" id="y"><option value="26">26</option><option value="27">27</option></select>
    </form>`;
    fillCard(findCardFields(document)[0]!, { ...card, expYear: '27' });
    expect($<HTMLSelectElement>('#m').value).toBe('03');
    expect($<HTMLSelectElement>('#y').value).toBe('27');
  });

  it('writes a combined expiry as the field asks', () => {
    const input = (attributes: string) => {
      document.body.innerHTML = `<input ${attributes}>`;
      return $('input');
    };
    expect(formatExpiry(input('placeholder="MM / YY"'), '3', '2027')).toBe('03 / 27');
    expect(formatExpiry(input('placeholder="MM/JJJJ"'), '3', '2027')).toBe('03/2027');
    expect(formatExpiry(input('maxlength="7"'), '3', '27')).toBe('03/2027');
    expect(formatExpiry(input('maxlength="4"'), '12', '2030')).toBe('1230');
    expect(formatExpiry(input(''), '12', '2030')).toBe('12/30');
  });
});

describe('addresses', () => {
  it('fills an address form, the country select by name', () => {
    load('address');
    const values: IdentityValues = {
      kind: 'identity',
      title: 'Frau',
      firstName: 'Anna',
      middleName: null,
      lastName: 'Beispiel',
      username: null,
      company: null,
      email: 'anna@example.com',
      phone: '+49 30 1234567',
      address1: 'Musterstraße 1',
      address2: null,
      address3: null,
      postalCode: '12345',
      city: 'Berlin',
      state: null,
      country: 'Deutschland',
    };
    expect(fillIdentity(findIdentityFields(document)[0]!, values)).toBe(true);
    expect($<HTMLSelectElement>('#a-title').value).toBe('frau');
    expect($('#a-first').value).toBe('Anna');
    expect($('#a-last').value).toBe('Beispiel');
    expect($('#a-company').value).toBe('');
    expect($('#a-street').value).toBe('Musterstraße 1');
    expect($('#a-zip').value).toBe('12345');
    expect($('#a-city').value).toBe('Berlin');
    expect($<HTMLSelectElement>('#a-country').value).toBe('DE');
    expect($('#a-mail').value).toBe('anna@example.com');

    fillIdentity(findIdentityFields(document)[0]!, { ...values, country: 'CH' });
    expect($<HTMLSelectElement>('#a-country').value).toBe('CH');
  });
});
