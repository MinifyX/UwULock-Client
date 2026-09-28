import { beforeEach, describe, expect, it } from 'vitest';
import {
  classifyPasswordFields,
  findCardFields,
  findIdentityFields,
  findLoginFields,
  findTotpFields,
  isVisible,
  scanFields,
} from '../src/content/forms';
import { load } from './fixtures/load';

const $ = <T extends Element = HTMLInputElement>(selector: string) =>
  document.querySelector(selector) as T;

beforeEach(() => {
  document.body.innerHTML = '';
});

describe('logins', () => {
  it('finds a simple login form, not the search box in it', () => {
    load('login');
    const [login, ...rest] = findLoginFields(document);
    expect(rest).toHaveLength(0);
    expect(login).toMatchObject({ kind: 'login', form: $('#login') });
    expect(login!.username).toBe($('#user'));
    expect(login!.password).toBe($('#pass'));
    expect(login!.newPasswords).toEqual([]);
  });

  it('finds email and password without a form', () => {
    load('formless');
    const [login] = findLoginFields(document);
    expect(login).toMatchObject({ kind: 'login', form: null });
    expect(login!.username).toBe($('#email'));
    expect(login!.password).toBe($('#pw'));
  });

  it('finds fields in open shadow roots', () => {
    const host = document.createElement('div');
    document.body.append(host);
    const shadow = host.attachShadow({ mode: 'open' });
    shadow.innerHTML = '<form><input name="user"><input type="password" name="pw"></form>';
    const [login] = findLoginFields(document);
    expect(login!.username).toBe(shadow.querySelector('[name=user]'));
    expect(login!.password).toBe(shadow.querySelector('[name=pw]'));
  });

  it('finds the username on its own as a first step', () => {
    load('username-step');
    const [login] = findLoginFields(document);
    expect(login).toMatchObject({ kind: 'username-step', password: null });
    expect(login!.username).toBe($('#identifier'));
  });

  it('finds a password on its own as a second step', () => {
    load('password-step');
    const [login] = findLoginFields(document);
    expect(login).toMatchObject({ kind: 'login', username: null });
    expect(login!.password).toBe($('#pw'));
  });

  it('takes sign-up forms as sign-up: no password to fill, both are new', () => {
    load('signup');
    const [login] = findLoginFields(document);
    expect(login).toMatchObject({ kind: 'signup', password: null });
    expect(login!.username).toBe($('#mail'));
    expect(login!.newPasswords).toEqual([$('#pw1'), $('#pw2')]);
  });

  it('tells the current password from the new ones', () => {
    load('change-password');
    const [login] = findLoginFields(document);
    expect(login).toMatchObject({ kind: 'change-password', username: null });
    expect(login!.password).toBe($('#old'));
    expect(login!.newPasswords).toEqual([$('#new'), $('#confirm')]);
  });

  it('classifies by autocomplete first', () => {
    const make = (autocomplete: string) => {
      const input = document.createElement('input');
      input.type = 'password';
      input.setAttribute('autocomplete', autocomplete);
      return input;
    };
    const current = make('current-password');
    const fresh = make('new-password');
    expect(classifyPasswordFields([fresh, current])).toEqual({
      kind: 'change-password',
      current,
      fresh: [fresh],
    });
    expect(classifyPasswordFields([fresh])).toMatchObject({ kind: 'signup', current: null });
    expect(classifyPasswordFields([current])).toMatchObject({ kind: 'login', current });
    expect(classifyPasswordFields([current, current, current, current])).toBeNull();
  });

  it('keeps passwords switched to text by "show password"', () => {
    load('login');
    findLoginFields(document);
    $('#pass').type = 'text';
    const [login] = findLoginFields(document);
    expect(login!.password).toBe($('#pass'));
    expect(login!.username).toBe($('#user'));
  });

  it('ignores hidden, invisible and disabled password fields', () => {
    load('honeypot');
    for (const id of ['hp1', 'hp2', 'hp3', 'hp4', 'hp5'])
      expect(isVisible($(`#${id}`))).toBe(false);
    const [login, ...rest] = findLoginFields(document);
    expect(rest).toHaveLength(0);
    expect(login).toMatchObject({ kind: 'login' });
    expect(login!.password).toBe($('#pw'));
    expect(login!.username).toBe($('#user'));
  });

  it('does not take search boxes or newsletter fields for logins', () => {
    load('not-login');
    const found = scanFields(document);
    expect(found.logins).toEqual([]);
    expect(found.totps).toEqual([]);
    expect(found.identities).toEqual([]);
  });
});

describe('one-time codes', () => {
  it('finds a single code field and six boxes', () => {
    load('totp');
    const [single, split, ...rest] = findTotpFields(document);
    expect(rest).toHaveLength(0);
    expect(single!.inputs).toEqual([$('#otp')]);
    expect(split!.inputs).toEqual(Array.from(document.querySelectorAll('#split .d')));
    expect(findLoginFields(document)).toEqual([]);
  });

  it('takes autocomplete=one-time-code, and not a postal code', () => {
    document.body.innerHTML =
      '<input id="a" autocomplete="one-time-code"><input id="b" name="postcode" maxlength="5" inputmode="numeric">';
    expect(findTotpFields(document).map((t) => t.inputs[0]!.id)).toEqual(['a']);
  });
});

describe('cards', () => {
  it('finds a checkout form with German labels and selects', () => {
    load('checkout');
    const [card] = findCardFields(document);
    expect(card!.fields).toEqual({
      name: $('#holder'),
      number: $('#cc'),
      expMonth: $('#month'),
      expYear: $('#year'),
      code: $('#cvc'),
    });
    // The card's code is a password field, but no login.
    expect(findLoginFields(document)).toEqual([]);
  });

  it('reads autocomplete with sections', () => {
    document.body.innerHTML = `<form>
      <input id="n" autocomplete="section-pay billing cc-number">
      <input id="e" autocomplete="cc-exp" placeholder="MM/JJ">
      <input id="c" autocomplete="cc-csc"></form>`;
    const [card] = findCardFields(document);
    expect(Object.keys(card!.fields).sort()).toEqual(['code', 'expiry', 'number']);
  });
});

describe('addresses', () => {
  it('finds an address form with German labels', () => {
    load('address');
    const [identity] = findIdentityFields(document);
    expect(identity!.fields).toEqual({
      title: $('#a-title'),
      firstName: $('#a-first'),
      lastName: $('#a-last'),
      company: $('#a-company'),
      address1: $('#a-street'),
      postalCode: $('#a-zip'),
      city: $('#a-city'),
      country: $('#a-country'),
      phone: $('#a-phone'),
      email: $('#a-mail'),
    });
    expect(findLoginFields(document)).toEqual([]);
  });

  it('reads autocomplete tokens', () => {
    document.body.innerHTML = `<form>
      <input id="g" autocomplete="shipping given-name"><input id="f" autocomplete="family-name">
      <textarea id="s" autocomplete="street-address"></textarea>
      <input id="l2" autocomplete="address-level2"><input id="l1" autocomplete="address-level1"></form>`;
    const [identity] = findIdentityFields(document);
    expect(Object.keys(identity!.fields).sort()).toEqual([
      'address',
      'city',
      'firstName',
      'lastName',
      'state',
    ]);
  });
});
