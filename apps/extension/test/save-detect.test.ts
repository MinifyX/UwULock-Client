import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { installSaveDetection, type Submission } from '../src/content/save-detect';
import { load } from './fixtures/load';

const ask = vi.hoisted(() => vi.fn(async () => null));
vi.mock('../src/shared/messages', () => ({ ask }));

const $ = <T extends Element = HTMLInputElement>(selector: string) =>
  document.querySelector(selector) as T;

let sent: Submission[];
let stop: (() => void) | null = null;
let clock = 0;

function install(options: { enabled?: boolean; trustAll?: boolean; viaAsk?: boolean } = {}) {
  stop = installSaveDetection(document, {
    enabled: () => options.enabled ?? true,
    ...(options.viaAsk ? {} : { send: (submission: Submission) => sent.push(submission) }),
    ...(options.trustAll ? { trusted: () => true } : {}),
    now: () => clock,
  });
}

/** jsdom would try to navigate; the page's own handler runs after ours (capture). */
const noNavigation = (event: Event) => event.preventDefault();

beforeEach(() => {
  sent = [];
  clock = 1000;
  ask.mockClear();
  document.addEventListener('submit', noNavigation);
});

afterEach(() => {
  stop?.();
  stop = null;
  document.removeEventListener('submit', noNavigation);
  document.body.innerHTML = '';
});

describe('save detection', () => {
  it('sends a login form once when its button is clicked', () => {
    load('login');
    // jsdom's click() is untrusted, as a page's would be: trust it here to see click and submit.
    install({ viaAsk: true, trustAll: true });
    $('#user').value = 'anna';
    $('#pass').value = 'geheim';
    $<HTMLButtonElement>('button').click(); // a click, then the form's submit
    expect(ask).toHaveBeenCalledTimes(1);
    expect(ask).toHaveBeenCalledWith({
      type: 'content:submitted',
      username: 'anna',
      password: 'geheim',
      newPassword: null,
    });
  });

  it('sends the same values again only after a while', () => {
    load('login');
    install();
    $('#user').value = 'anna';
    $('#pass').value = 'geheim';
    const form = $<HTMLFormElement>('#login');
    form.requestSubmit();
    form.requestSubmit();
    expect(sent).toHaveLength(1);
    clock += 5000;
    form.requestSubmit();
    expect(sent).toHaveLength(2);
    $('#pass').value = 'anders';
    form.requestSubmit();
    expect(sent.map((s) => s.password)).toEqual(['geheim', 'geheim', 'anders']);
  });

  it('sends the new and the current password of a change-password form', () => {
    load('change-password');
    install();
    $('#old').value = 'alt';
    $('#new').value = 'neu-und-lang';
    $('#confirm').value = 'neu-und-lang';
    $<HTMLFormElement>('#change').requestSubmit();
    expect(sent).toEqual([
      { type: 'content:submitted', username: null, password: 'alt', newPassword: 'neu-und-lang' },
    ]);
  });

  it('sends nothing when the new passwords differ', () => {
    load('change-password');
    install();
    $('#old').value = 'alt';
    $('#new').value = 'eins';
    $('#confirm').value = 'zwei';
    $<HTMLFormElement>('#change').requestSubmit();
    expect(sent).toEqual([]);
  });

  it('sends nothing without a password', () => {
    load('login');
    install({ trustAll: true });
    $('#user').value = 'anna';
    $<HTMLButtonElement>('button').click();
    expect(sent).toEqual([]);
  });

  it('sends a username on its own for the first step', () => {
    load('username-step');
    install({ trustAll: true });
    $('#identifier').value = 'anna@example.com';
    $<HTMLButtonElement>('#next').click();
    expect(sent).toEqual([
      {
        type: 'content:submitted',
        username: 'anna@example.com',
        password: null,
        newPassword: null,
      },
    ]);
  });

  it('finds a form without <form> from its button, and Enter in the password', () => {
    load('formless');
    install({ trustAll: true });
    $('#email').value = 'anna@example.com';
    $('#pw').value = 'geheim';
    $('#go').dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(sent).toHaveLength(1);
    expect(sent[0]).toMatchObject({ username: 'anna@example.com', password: 'geheim' });
    clock += 5000;
    $('#pw').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    expect(sent).toHaveLength(2);
  });

  it('ignores events the page made, and pages with the prompt off', () => {
    load('login');
    install();
    $('#user').value = 'anna';
    $('#pass').value = 'geheim';
    $('#login').dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    $('#pass').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    expect(sent).toEqual([]);
    stop!();
    install({ enabled: false });
    $<HTMLFormElement>('#login').requestSubmit();
    expect(sent).toEqual([]);
  });
});
