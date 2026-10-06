/**
 * The Generator tab: a password with the length, the character sets and how
 * many of each it gets at least (steppers); or, with UwUMail connected, a
 * masked address. The options are the desktop generator's (same storage);
 * passwords are never remembered.
 */

import { haptic, ICONS, ListRow, ListSection, Stepper } from '@uwusuite/design';
import { useCallback, useEffect, useState, type CSSProperties } from 'react';
import {
  copyGenerated,
  generatePassword,
  type Generated,
  type GeneratorOptions,
} from '../../lib/api';
import { toastError } from '../../lib/errors';
import {
  effectiveLength,
  MAX_LENGTH,
  minimumOf,
  withMinimum,
  type CharSet,
} from '../../lib/generator';
import { N_, t, useLanguage } from '../../lib/i18n';
import {
  createMaskedAddress,
  has,
  maskedConnection,
  useUwu,
  type MaskedAddress,
  type MaskedConnection,
} from '../../lib/uwu';
import { loadOptions, saveOptions, strength } from '../../components/GeneratorDialog';
import { Colored } from '../../components/ItemDetail';
import { copyAddress, MaskedNotConnected } from '../../components/MaskedDialog';
import { playNyu } from '../../components/nyu/stage';
import { copiedNote } from '../state';
import { BigButton, FieldInput, Page, Segmented, Toggle } from '../ui';

const SETS: { key: CharSet; label: string; chars: string }[] = [
  { key: 'uppercase', label: N_('Großbuchstaben'), chars: 'A–Z' },
  { key: 'lowercase', label: N_('Kleinbuchstaben'), chars: 'a–z' },
  { key: 'digits', label: N_('Ziffern'), chars: '0–9' },
  { key: 'symbols', label: N_('Sonderzeichen'), chars: '!@#$' },
];

/** The longest password the slider offers; the minimums may still ask for more. */
const SLIDER_MAX = 64;

export function GeneratorPage() {
  useLanguage();
  const uwu = useUwu();
  const maskable = has(uwu, 'masked-addresses');
  const [mode, setMode] = useState<'password' | 'masked'>('password');
  useEffect(() => {
    if (!maskable) setMode('password');
  }, [maskable]);

  return (
    <Page title={t('Generator')} largeTitle>
      {maskable && (
        <Segmented
          label={t('Was erzeugt wird')}
          value={mode}
          onChange={setMode}
          options={[
            { value: 'password', label: t('Passwort') },
            { value: 'masked', label: t('Maskiert') },
          ]}
        />
      )}
      {mode === 'masked' && maskable ? <MaskedMode /> : <PasswordMode />}
    </Page>
  );
}

function PasswordMode() {
  useLanguage();
  const [options, setOptions] = useState<GeneratorOptions>(loadOptions);
  const [result, setResult] = useState<Generated | null>(null);

  const roll = useCallback(async (next: GeneratorOptions) => {
    try {
      setResult(await generatePassword(next));
    } catch (e) {
      toastError(e);
    }
  }, []);

  useEffect(() => {
    void roll(options);
    saveOptions(options);
  }, [options, roll]);

  const set = (patch: Partial<GeneratorOptions>) => {
    const next = { ...options, ...patch };
    // At least one set stays on.
    if (!next.lowercase && !next.uppercase && !next.digits && !next.symbols) {
      haptic('error');
      return;
    }
    setOptions(next);
  };

  const copy = async () => {
    if (!result) return;
    try {
      await copyGenerated(result.password);
      haptic('success');
      copiedNote('password');
    } catch (e) {
      toastError(e);
    }
  };

  const effective = effectiveLength(options);
  const meter = result ? strength(result.bits) : null;
  const slider = Math.min(options.length, SLIDER_MAX);
  return (
    <>
      <div className="m-gen-card">
        <output className="m-gen-value" aria-live="polite">
          {result ? <Colored text={result.password} /> : '…'}
        </output>
        <div>
          <div className="m-strength" aria-hidden>
            {[1, 2, 3, 4].map((level) => (
              <i key={level} data-on={meter && level <= meter.level ? '' : undefined} />
            ))}
          </div>
          <div className="m-strength-label">
            <span>{meter?.label ?? '…'}</span>
            <span>{t('{bits} Bit', { bits: result?.bits ?? 0 })}</span>
          </div>
        </div>
        <div className="m-gen-buttons">
          <button
            type="button"
            onClick={() => {
              haptic('light');
              void roll(options);
              playNyu('generated');
            }}
          >
            <ICONS.generate aria-hidden />
            {t('Neu würfeln')}
          </button>
          <button type="button" data-primary onClick={() => void copy()} disabled={!result}>
            <ICONS.copy aria-hidden />
            {t('Kopieren')}
          </button>
        </div>
      </div>

      <ListSection>
        <div className="m-slider">
          <div className="m-slider-top">
            <span>{t('Länge')}</span>
            <b>{t('{n} Zeichen', { n: effective })}</b>
          </div>
          <input
            type="range"
            min={5}
            max={SLIDER_MAX}
            value={slider}
            aria-label={t('Länge')}
            style={{ '--fill': `${((slider - 5) / (SLIDER_MAX - 5)) * 100}%` } as CSSProperties}
            onChange={(event) => set({ length: Number(event.target.value) })}
          />
          {effective > options.length && (
            <span className="m-len-note">
              {t('Mindestens {n}, damit die Mindestanzahlen passen.', { n: effective })}
            </span>
          )}
        </div>
      </ListSection>

      <ListSection header={t('Zeichen und Mindestanzahl')}>
        {SETS.map(({ key, label, chars }) => {
          const on = options[key];
          const minimum = minimumOf(options, key);
          return (
            <ListRow
              key={key}
              title={t(label)}
              subtitle={
                <>
                  <span className="m-set">{chars}</span>
                  {on && (
                    <>
                      {' · '}
                      {t('mindestens')} <b>{minimum}</b>
                    </>
                  )}
                </>
              }
              trailing={
                <>
                  {on && (
                    <Stepper
                      label={t('Mindestens {set}', { set: chars })}
                      value={minimum}
                      min={1}
                      max={Math.min(Math.max(16, minimum), MAX_LENGTH)}
                      onChange={(value) => setOptions(withMinimum(options, key, value))}
                    />
                  )}
                  <Toggle
                    label={t(label)}
                    checked={on}
                    onChange={(checked) => set({ [key]: checked })}
                  />
                </>
              }
            />
          );
        })}
      </ListSection>

      <div className="m-gap" />
      <ListSection>
        <ListRow
          title={t('Verwechselbare Zeichen weglassen (l, 1, I, O, 0)')}
          wrap
          trailing={
            <Toggle
              label={t('Verwechselbare Zeichen weglassen (l, 1, I, O, 0)')}
              checked={options.avoidAmbiguous}
              onChange={(avoidAmbiguous) => set({ avoidAmbiguous })}
            />
          }
        />
      </ListSection>
    </>
  );
}

/** A new masked address from UwUMail, for a site, copied right away. */
function MaskedMode() {
  useLanguage();
  const [connection, setConnection] = useState<MaskedConnection | null>(null);
  const [site, setSite] = useState('');
  const [description, setDescription] = useState('');
  const [made, setMade] = useState<MaskedAddress | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    void maskedConnection()
      .then(setConnection)
      .catch((e) => {
        toastError(e);
        setConnection({
          connected: false,
          server: null,
          username: null,
          domains: null,
          defaultDomain: null,
          status: null,
        });
      });
  }, []);
  if (!connection) return <p className="m-footnote">{t('Einen Moment …')}</p>;
  if (!connection.connected || connection.status === 'revoked')
    return (
      <div className="m-embed-block">
        <MaskedNotConnected connection={connection} />
      </div>
    );
  const create = async () => {
    setBusy(true);
    try {
      const address = await createMaskedAddress(site || null, description || null, null);
      setMade(address);
      await copyAddress(address.email);
      haptic('success');
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      {made && (
        <div className="m-gen-card">
          <output className="m-gen-value" aria-live="polite">
            {made.email}
          </output>
          <div className="m-gen-buttons">
            <button
              type="button"
              data-primary
              // copyAddress tells how it went itself.
              onClick={() => void copyAddress(made.email)}
            >
              <ICONS.copy aria-hidden />
              {t('Kopieren')}
            </button>
          </div>
        </div>
      )}
      <ListSection>
        {connection.defaultDomain && (
          <ListRow title={t('Domain')} value={connection.defaultDomain} />
        )}
        <FieldInput
          label={t('Für Website')}
          value={site}
          placeholder="shop.example.com"
          inputMode="url"
          onChange={setSite}
        />
        <FieldInput
          label={t('Beschreibung')}
          value={description}
          maxLength={200}
          placeholder={t('Wofür ist die Adresse?')}
          onChange={setDescription}
        />
      </ListSection>
      <div className="m-buttons">
        <BigButton icon={ICONS.maskedAddress} disabled={busy} onClick={() => void create()}>
          {made ? t('Noch eine') : t('Adresse erstellen')}
        </BigButton>
      </div>
      <p className="m-footnote">
        {t('Mails an die Adresse landen in deinem Postfach. Du kannst sie jederzeit abschalten.')}
      </p>
    </>
  );
}
