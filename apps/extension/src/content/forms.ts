/**
 * Finding the fields UwULock fills: logins and their single steps, one-time codes, cards and
 * addresses. Pure functions over a document, a shadow root or an element; nothing here talks to
 * the background or changes the page.
 *
 * Fields are grouped by their form. Fields outside any form are grouped by the closest
 * `fieldset`, `dialog` or `[role=form]`, else by their document or shadow root. Open shadow
 * roots are searched too, closed ones can't be.
 *
 * Visibility: jsdom (the tests) has no layout, every box is 0×0 there. Sizes and positions are
 * therefore only used when the page has a layout at all (its root element has a size); without
 * one, `isVisible` goes by styles and attributes only (`hidden`, `aria-hidden`, `display`,
 * `visibility`, `opacity`, an inline size of 0, an inline position far off-screen).
 */

export type Control = HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;

export type LoginKind = 'login' | 'username-step' | 'signup' | 'change-password';

export type LoginFields = {
  kind: LoginKind;
  form: HTMLFormElement | null;
  username: HTMLInputElement | null;
  /** The password a login fills: a login form's, or the current one of a change-password form. */
  password: HTMLInputElement | null;
  /** New passwords (sign-up, change password): never filled with a login's password. */
  newPasswords: HTMLInputElement[];
};

/** One field for the code, or one box per digit. */
export type TotpFields = { form: HTMLFormElement | null; inputs: HTMLInputElement[] };

export type CardRole = 'name' | 'number' | 'expiry' | 'expMonth' | 'expYear' | 'code' | 'brand';
export type CardFields = {
  form: HTMLFormElement | null;
  fields: Partial<Record<CardRole, Control>>;
};

export type IdentityRole =
  | 'title'
  | 'firstName'
  | 'middleName'
  | 'lastName'
  /** The whole name in one field. */
  | 'name'
  | 'username'
  | 'company'
  | 'email'
  | 'phone'
  /** The whole street address in one field. */
  | 'address'
  | 'address1'
  | 'address2'
  | 'address3'
  | 'postalCode'
  | 'city'
  | 'state'
  | 'country';
export type IdentityFields = {
  form: HTMLFormElement | null;
  fields: Partial<Record<IdentityRole, Control>>;
};

export type Scan = {
  logins: LoginFields[];
  totps: TotpFields[];
  cards: CardFields[];
  identities: IdentityFields[];
};

// ── Walking the page ──────────────────────────────────────

/** Elements looked at in one walk at most: a page bigger than this is searched only in part. */
const MAX_ELEMENTS = 30000;

type Group = { form: HTMLFormElement | null; controls: Control[] };

function isControl(el: Element): el is Control {
  return (
    el instanceof HTMLInputElement ||
    el instanceof HTMLSelectElement ||
    el instanceof HTMLTextAreaElement
  );
}

/** Every input, select and textarea under `root`, in document order, open shadow roots included. */
export function collectControls(root: ParentNode): Control[] {
  const out: Control[] = [];
  let budget = MAX_ELEMENTS;
  const walk = (start: Node) => {
    const doc = start.ownerDocument ?? (start as Document);
    const walker = doc.createTreeWalker(start, NodeFilter.SHOW_ELEMENT);
    for (let node = walker.nextNode(); node && budget > 0; node = walker.nextNode()) {
      budget -= 1;
      const el = node as Element;
      if (isControl(el)) out.push(el);
      const shadow = (el as HTMLElement).shadowRoot;
      if (shadow) walk(shadow);
    }
  };
  walk(root as Node);
  return out;
}

function groupKey(control: Control): Node {
  return (
    control.form ??
    control.closest('fieldset, dialog, [role="form"]') ??
    (control.getRootNode() as Document | ShadowRoot)
  );
}

function groupsOf(root: ParentNode): Group[] {
  const groups = new Map<Node, Group>();
  for (const control of collectControls(root)) {
    const key = groupKey(control);
    let group = groups.get(key);
    if (!group) {
      group = { form: control.form, controls: [] };
      groups.set(key, group);
    }
    group.controls.push(control);
  }
  return [...groups.values()];
}

// ── Visibility ────────────────────────────────────────────

function hasLayout(doc: Document): boolean {
  const rect = doc.documentElement.getBoundingClientRect();
  return rect.width > 0 || rect.height > 0;
}

function parentAcrossShadow(el: Element): Element | null {
  if (el.parentElement) return el.parentElement;
  const root = el.getRootNode();
  return root instanceof ShadowRoot ? root.host : null;
}

function inlinePx(value: string): number | null {
  const match = /^(-?\d+(?:\.\d+)?)(px)?$/.exec(value.trim());
  return match ? Number(match[1]) : null;
}

/** Whether a person can see `el`. See the note at the top for pages without a layout. */
export function isVisible(el: Element): boolean {
  if (el instanceof HTMLInputElement && el.type === 'hidden') return false;
  const view = el.ownerDocument.defaultView;
  const check = (el as Element & { checkVisibility?: (options?: object) => boolean })
    .checkVisibility;
  const native = typeof check === 'function';
  if (native && !check.call(el, { visibilityProperty: true, opacityProperty: true })) return false;

  for (let node: Element | null = el, depth = 0; node && depth < 50; depth += 1) {
    if (node.getAttribute('aria-hidden') === 'true') return false;
    if (!native) {
      if ((node as HTMLElement).hidden) return false;
      const style = view?.getComputedStyle(node);
      if (style?.display === 'none') return false;
      if (node === el && style) {
        if (style.visibility === 'hidden' || style.visibility === 'collapse') return false;
        if (style.opacity === '0') return false;
      }
    }
    node = parentAcrossShadow(node);
  }

  if (hasLayout(el.ownerDocument)) {
    const rect = el.getBoundingClientRect();
    if (rect.width <= 1 || rect.height <= 1) return false;
    const scrollX = view?.scrollX ?? 0;
    const scrollY = view?.scrollY ?? 0;
    if (rect.right + scrollX <= 0 || rect.bottom + scrollY <= 0) return false;
  } else {
    const style = (el as HTMLElement).style;
    if (style) {
      if (inlinePx(style.width) === 0 || inlinePx(style.height) === 0) return false;
      const left = inlinePx(style.left);
      const top = inlinePx(style.top);
      if ((left !== null && left <= -500) || (top !== null && top <= -500)) return false;
    }
  }
  return true;
}

/** Visible, enabled and not read-only: a field UwULock may write into. */
export function isFillable(el: Control): boolean {
  if (el.disabled) return false;
  if (!(el instanceof HTMLSelectElement) && el.readOnly) return false;
  return isVisible(el);
}

// ── What a field says about itself ────────────────────────

/** The field's name, id, autocomplete, placeholder, aria-label, title and label, lower-cased. */
export function hintText(el: Element): string {
  const parts: string[] = [];
  for (const name of ['name', 'id', 'autocomplete', 'placeholder', 'aria-label', 'title']) {
    const value = el.getAttribute(name);
    if (value) parts.push(value);
  }
  const labels = (el as HTMLInputElement).labels;
  if (labels) for (const label of Array.from(labels)) parts.push(label.textContent ?? '');
  const labelledBy = el.getAttribute('aria-labelledby');
  if (labelledBy) {
    const root = el.getRootNode() as Document | ShadowRoot;
    for (const id of labelledBy.split(/\s+/)) {
      const label = id && root.getElementById?.(id);
      if (label) parts.push(label.textContent ?? '');
    }
  }
  return parts
    .join(' ')
    .slice(0, 400)
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .toLowerCase();
}

/** The field name of the `autocomplete` attribute: `section-a billing cc-number` → `cc-number`. */
export function autocompleteName(el: Element): string {
  const tokens = (el.getAttribute('autocomplete') ?? '').toLowerCase().trim().split(/\s+/);
  for (let i = tokens.length - 1; i >= 0; i -= 1) {
    const token = tokens[i]!;
    if (token && token !== 'webauthn') return token;
  }
  return '';
}

const TEXT_TYPES = new Set(['text', 'email', 'tel']);
const CODE_TYPES = new Set(['text', 'tel', 'number']);

const USER_HINT =
  /user|login|log-in|e-?mail|benutzer|anmelde|account|konto|identifier|kennung|kundennummer/;
const SEARCH_HINT = /search|such|query|keyword|stichwort/;
const NEWSLETTER_HINT = /newsletter|subscri|abonn/;
const NEXT_HINT =
  /next|weiter|continue|fortfahren|sign.?in|log.?in|login|anmelden|einloggen|submit|senden/;
const NEW_PASSWORD_HINT =
  /new|neu|confirm|bestätig|bestaetig|wiederhol|repeat|retype|re-enter|verify|again|erneut|password.?2|passwort.?2/;
const CURRENT_PASSWORD_HINT = /current|\bold|aktuell|bisherig|\baltes?\b|existing|jetzig/;

function isSearch(el: HTMLInputElement): boolean {
  if (el.type === 'search' || el.getAttribute('role') === 'searchbox') return true;
  if (el.name === 'q' || el.name === 's') return true;
  return SEARCH_HINT.test(hintText(el));
}

/** Fields that are password fields now — or were, before a "show password" switched them. */
const seenPasswords = new WeakSet<HTMLInputElement>();

export function isPasswordField(el: Element): el is HTMLInputElement {
  if (!(el instanceof HTMLInputElement)) return false;
  if (el.type === 'password') {
    seenPasswords.add(el);
    return true;
  }
  return el.type === 'text' && seenPasswords.has(el);
}

function isTextField(el: Control): el is HTMLInputElement {
  return el instanceof HTMLInputElement && TEXT_TYPES.has(el.type) && !seenPasswords.has(el);
}

function looksLikeUsername(el: HTMLInputElement): boolean {
  const ac = autocompleteName(el);
  if (ac === 'username' || ac === 'email') return true;
  return el.type === 'email' || USER_HINT.test(hintText(el));
}

// ── Logins ────────────────────────────────────────────────

export type PasswordClasses = {
  kind: 'login' | 'signup' | 'change-password';
  current: HTMLInputElement | null;
  fresh: HTMLInputElement[];
};

/**
 * Which of a form's password fields (in order) is a login's, the current one or a new one.
 * One field is a login (unless it says it wants a new password); two are sign-up (password and
 * confirmation) unless one of them asks for the current password; three are current, new and
 * confirmation. More than three: nothing to do with.
 */
export function classifyPasswordFields(fields: HTMLInputElement[]): PasswordClasses | null {
  if (fields.length === 0 || fields.length > 3) return null;
  const isNew = (field: HTMLInputElement) => {
    const ac = autocompleteName(field);
    if (ac === 'new-password') return true;
    if (ac === 'current-password') return false;
    return NEW_PASSWORD_HINT.test(hintText(field));
  };
  const isCurrent = (field: HTMLInputElement) => {
    const ac = autocompleteName(field);
    if (ac === 'current-password') return true;
    if (ac === 'new-password') return false;
    return CURRENT_PASSWORD_HINT.test(hintText(field));
  };

  if (fields.length === 1) {
    const [only] = fields as [HTMLInputElement];
    return isNew(only) && !isCurrent(only)
      ? { kind: 'signup', current: null, fresh: [only] }
      : { kind: 'login', current: only, fresh: [] };
  }
  const current = fields.find(isCurrent) ?? (fields.length === 3 ? fields[0]! : null);
  if (!current) return { kind: 'signup', current: null, fresh: [...fields] };
  return { kind: 'change-password', current, fresh: fields.filter((f) => f !== current) };
}

function findUsername(inputs: Control[], before: HTMLInputElement): HTMLInputElement | null {
  const end = inputs.indexOf(before);
  let fallback: HTMLInputElement | null = null;
  for (let i = end - 1; i >= 0; i -= 1) {
    const el = inputs[i]!;
    if (!isTextField(el) || isSearch(el) || !isFillable(el)) continue;
    if (looksLikeUsername(el)) return el;
    fallback ??= el;
  }
  return fallback;
}

function buttonText(el: Element): string {
  const parts = [
    el.textContent ?? '',
    (el as HTMLInputElement).value ?? '',
    el.getAttribute('aria-label') ?? '',
    el.getAttribute('title') ?? '',
    el.id,
    el.getAttribute('name') ?? '',
  ];
  return parts.join(' ').slice(0, 200).toLowerCase();
}

const BUTTONS =
  'button, input[type="submit"], input[type="button"], input[type="image"], [role="button"]';

/** The buttons closest to `el`: in its form, or in the nearest ancestor that has any. */
export function nearbyButtons(el: Element): Element[] {
  const form = (el as HTMLInputElement).form;
  if (form) return Array.from(form.querySelectorAll(BUTTONS));
  let node: Element | null = el.parentElement;
  for (let depth = 0; node && depth < 6; depth += 1, node = node.parentElement) {
    const found = node.querySelectorAll(BUTTONS);
    if (found.length) return Array.from(found);
  }
  return [];
}

function usernameStep(group: Group): HTMLInputElement | null {
  const texts = group.controls.filter(
    (el): el is HTMLInputElement =>
      el instanceof HTMLInputElement &&
      (TEXT_TYPES.has(el.type) || el.type === 'number') &&
      isFillable(el) &&
      !isSearch(el),
  );
  if (texts.length !== 1) return null;
  const field = texts[0]!;
  if (!isTextField(field) || !looksLikeUsername(field) || isTotpField(field)) return null;
  if (NEWSLETTER_HINT.test(hintText(field))) return null;
  const buttons = nearbyButtons(field).map(buttonText);
  if (buttons.some((text) => NEWSLETTER_HINT.test(text))) return null;
  if (group.form && NEWSLETTER_HINT.test(hintText(group.form))) return null;
  return buttons.some((text) => NEXT_HINT.test(text)) ? field : null;
}

function loginsOf(groups: Group[]): LoginFields[] {
  const found: LoginFields[] = [];
  for (const group of groups) {
    const passwords = group.controls.filter(
      (el): el is HTMLInputElement => isPasswordField(el) && isFillable(el) && !cardRole(el),
    );
    if (passwords.length === 0) {
      const username = usernameStep(group);
      if (username) {
        found.push({
          kind: 'username-step',
          form: group.form,
          username,
          password: null,
          newPasswords: [],
        });
      }
      continue;
    }
    const classes = classifyPasswordFields(passwords);
    if (!classes) continue;
    found.push({
      kind: classes.kind,
      form: group.form,
      username: findUsername(group.controls, passwords[0]!),
      password: classes.current,
      newPasswords: classes.fresh,
    });
  }
  return found;
}

/** Login, sign-up, change-password forms and first login steps (a username on its own). */
export function findLoginFields(root: ParentNode): LoginFields[] {
  return loginsOf(groupsOf(root));
}

// ── One-time codes ────────────────────────────────────────

const TOTP_STRONG =
  /otp|2fa|mfa|one.?time|einmal|authenticat|two.?factor|zwei.?faktor|2.?step|zweistufig/;
const TOTP_WEAK = /code|token|\bpin\b|verif|bestätigung|bestaetigung/;
const TOTP_NOT =
  /postal|post.?code|zip|plz|postleitzahl|promo|coupon|gutschein|voucher|rabatt|discount|captcha|cvc|cvv|csc|card|karte|country|area|invite|einladung|referral|search|such/;

function codeLike(el: HTMLInputElement): boolean {
  return CODE_TYPES.has(el.type) && !seenPasswords.has(el);
}

export function isTotpField(el: HTMLInputElement): boolean {
  if (!codeLike(el)) return false;
  if (autocompleteName(el) === 'one-time-code') return true;
  const text = hintText(el);
  if (TOTP_NOT.test(text)) return false;
  const max = el.maxLength;
  if (max > 0 && (max < 4 || max > 10)) return false;
  if (TOTP_STRONG.test(text)) return true;
  if (!TOTP_WEAK.test(text)) return false;
  const numeric =
    el.inputMode === 'numeric' ||
    el.getAttribute('inputmode') === 'numeric' ||
    el.type === 'number' ||
    /\\d|\[0-9\]/.test(el.getAttribute('pattern') ?? '');
  return max > 0 || numeric;
}

function totpsOf(groups: Group[]): TotpFields[] {
  const found: TotpFields[] = [];
  for (const group of groups) {
    const inputs = group.controls.filter(
      (el): el is HTMLInputElement => el instanceof HTMLInputElement && isFillable(el),
    );
    // Runs of one-character boxes next to each other: 4 to 8 of them.
    let run: HTMLInputElement[] = [];
    const used = new Set<HTMLInputElement>();
    const close = () => {
      if (run.length >= 4 && run.length <= 8) {
        found.push({ form: group.form, inputs: run });
        for (const el of run) used.add(el);
      }
      run = [];
    };
    for (const el of inputs) {
      if (codeLike(el) && el.maxLength === 1 && !TOTP_NOT.test(hintText(el))) run.push(el);
      else close();
    }
    close();
    for (const el of inputs) {
      if (!used.has(el) && isTotpField(el)) found.push({ form: group.form, inputs: [el] });
    }
  }
  return found;
}

/** Fields for a one-time code: one field, or a box per digit. */
export function findTotpFields(root: ParentNode): TotpFields[] {
  return totpsOf(groupsOf(root));
}

// ── Cards ─────────────────────────────────────────────────

const CARD_AUTOCOMPLETE: Record<string, CardRole> = {
  'cc-name': 'name',
  'cc-number': 'number',
  'cc-exp': 'expiry',
  'cc-exp-month': 'expMonth',
  'cc-exp-year': 'expYear',
  'cc-csc': 'code',
  'cc-type': 'brand',
};

const CARD_CODE =
  /\b(cvc|cvv|csc|cvn|cid)\d?\b|security.?code|sicherheitscode|prüfnummer|pruefnummer|prüfziffer|kartenprüf|card.?code|card.?verification/;
const CARD_NAME =
  /card.?holder|holder.?name|karteninhaber|inhaber|name.?on.?card|cc.?name|card.?name|name.?auf.?der.?karte/;
const CARD_NUMBER =
  /card.?num|card.?no\b|cc.?num|kartennummer|kreditkartennummer|credit.?card|kreditkarte|debit.?card|\bpan\b/;
const CARD_EXPIRY =
  /\bexp|ablauf|gültig|gueltig|valid.?(thru|through|until|bis)|verfall|mm.?\/.?(yy|jj)/;
const CARD_MONTH = /month|monat|\bmm\b/;
const CARD_YEAR = /year|jahr|\byy(yy)?\b|\bjj(jj)?\b/;
const CARD_BRAND = /card.?type|kartentyp|card.?brand|kartenart|kartenmarke/;

/** What a field of a card form is for, or null. */
export function cardRole(el: Control): CardRole | null {
  const byAutocomplete = CARD_AUTOCOMPLETE[autocompleteName(el)];
  if (byAutocomplete) return byAutocomplete;
  const text = hintText(el);
  if (!text) return null;
  if (CARD_CODE.test(text)) return 'code';
  if (CARD_NAME.test(text)) return 'name';
  if (CARD_EXPIRY.test(text)) {
    if (CARD_MONTH.test(text) && !CARD_YEAR.test(text)) return 'expMonth';
    if (CARD_YEAR.test(text) && !CARD_MONTH.test(text)) return 'expYear';
    return 'expiry';
  }
  if (CARD_BRAND.test(text)) return 'brand';
  if (CARD_NUMBER.test(text)) return 'number';
  return null;
}

function optionTexts(select: HTMLSelectElement): string[] {
  return Array.from(select.options, (o) => `${o.value} ${o.text}`.trim().toLowerCase());
}

function looksLikeMonthSelect(select: HTMLSelectElement): boolean {
  const months = optionTexts(select).filter((text) => /^(0?[1-9]|1[0-2])\b|^[a-zä]{3}/.test(text));
  return months.length >= 12 && select.options.length <= 14;
}

function looksLikeYearSelect(select: HTMLSelectElement): boolean {
  return optionTexts(select).filter((text) => /^(20)?\d\d\b/.test(text)).length >= 3;
}

function cardsOf(groups: Group[]): CardFields[] {
  const found: CardFields[] = [];
  for (const group of groups) {
    const fields: Partial<Record<CardRole, Control>> = {};
    const unassigned: HTMLSelectElement[] = [];
    for (const el of group.controls) {
      if (!isVisible(el) || (el instanceof HTMLInputElement && !isCardInput(el))) continue;
      const role = cardRole(el);
      if (role && !fields[role]) fields[role] = el;
      else if (!role && el instanceof HTMLSelectElement) unassigned.push(el);
    }
    if (!fields.number) continue;
    if (!fields.expiry) {
      for (const select of unassigned) {
        if (!fields.expMonth && looksLikeMonthSelect(select)) fields.expMonth = select;
        else if (!fields.expYear && looksLikeYearSelect(select)) fields.expYear = select;
      }
    }
    found.push({ form: group.form, fields });
  }
  return found;
}

function isCardInput(el: HTMLInputElement): boolean {
  return ['text', 'tel', 'number', 'password', 'month'].includes(el.type);
}

/** Card forms: those with a field for the card number. */
export function findCardFields(root: ParentNode): CardFields[] {
  return cardsOf(groupsOf(root));
}

// ── Identities and addresses ──────────────────────────────

const IDENTITY_AUTOCOMPLETE: Record<string, IdentityRole> = {
  'honorific-prefix': 'title',
  'given-name': 'firstName',
  'additional-name': 'middleName',
  'family-name': 'lastName',
  name: 'name',
  organization: 'company',
  email: 'email',
  tel: 'phone',
  'tel-national': 'phone',
  'street-address': 'address',
  'address-line1': 'address1',
  'address-line2': 'address2',
  'address-line3': 'address3',
  'postal-code': 'postalCode',
  'address-level2': 'city',
  'address-level1': 'state',
  country: 'country',
  'country-name': 'country',
  username: 'username',
};

/** In this order: the first that matches wins. */
const IDENTITY_HINTS: [IdentityRole, RegExp][] = [
  ['email', /e-?mail/],
  ['title', /anrede|salutation|honorific/],
  ['middleName', /middle.?name|zweiter.?vorname/],
  ['firstName', /first.?name|\bfname\b|given.?name|vorname|forename/],
  ['lastName', /last.?name|\blname\b|surname|family.?name|nachname|familienname|zuname/],
  ['name', /full.?name|vollständiger.?name|vollstaendiger.?name|your.?name|ihr.?name/],
  ['company', /company|firma|organi[sz]ation|unternehmen/],
  ['phone', /phone|telefon|\btel\b|mobil|handy/],
  ['postalCode', /zip|postal|post.?code|\bplz\b|postleitzahl/],
  ['address3', /address.?(line)?.?3|\baddr3\b|adresse.?3/],
  ['address2', /address.?(line)?.?2|\baddr2\b|adresszusatz|adresse.?2|apartment|\bsuite\b/],
  ['address1', /street|stra(ss|ß)e|address.?(line)?.?1|\baddr1\b|\baddress\b|adresse|anschrift/],
  ['state', /\bstate\b|province|bundesland|bundesstaat|\bregion\b|county/],
  ['city', /city|\bort\b|stadt|town|wohnort|locality/],
  ['country', /country|\bland\b|\bstaat\b|nation/],
];

const IDENTITY_CORE = new Set<IdentityRole>([
  'name',
  'firstName',
  'lastName',
  'company',
  'address',
  'address1',
  'postalCode',
  'city',
  'state',
  'country',
]);

/** What a field of an address form is for, or null. */
export function identityRole(el: Control): IdentityRole | null {
  const byAutocomplete = IDENTITY_AUTOCOMPLETE[autocompleteName(el)];
  if (byAutocomplete) return byAutocomplete;
  if (el instanceof HTMLInputElement) {
    if (el.type === 'email') return 'email';
    if (el.type === 'tel') return 'phone';
  }
  const text = hintText(el);
  if (!text) return null;
  for (const [role, pattern] of IDENTITY_HINTS) if (pattern.test(text)) return role;
  return null;
}

function identitiesOf(groups: Group[], logins: LoginFields[]): IdentityFields[] {
  const inLogins = new Set<Element>();
  for (const login of logins) {
    if (login.username) inLogins.add(login.username);
  }
  const found: IdentityFields[] = [];
  for (const group of groups) {
    const fields: Partial<Record<IdentityRole, Control>> = {};
    let core = 0;
    for (const el of group.controls) {
      if (el instanceof HTMLInputElement && !TEXT_TYPES.has(el.type) && el.type !== 'number')
        continue;
      if (inLogins.has(el) || !isVisible(el) || cardRole(el)) continue;
      const role = identityRole(el);
      if (!role || fields[role]) continue;
      fields[role] = el;
      if (IDENTITY_CORE.has(role)) core += 1;
    }
    if (core >= 2) found.push({ form: group.form, fields });
  }
  return found;
}

/** Address forms: at least two of name, company, street, postal code, city, state, country. */
export function findIdentityFields(root: ParentNode): IdentityFields[] {
  const groups = groupsOf(root);
  return identitiesOf(groups, loginsOf(groups));
}

/** Everything at once, walking the page once. */
export function scanFields(root: ParentNode): Scan {
  const groups = groupsOf(root);
  const logins = loginsOf(groups);
  return {
    logins,
    totps: totpsOf(groups),
    cards: cardsOf(groups),
    identities: identitiesOf(groups, logins),
  };
}
