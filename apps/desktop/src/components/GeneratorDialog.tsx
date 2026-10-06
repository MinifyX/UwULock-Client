import { Button, ICONS, Segmented, Switch } from '@uwusuite/design';
import { useCallback, useEffect, useId, useState } from 'react';
import { copyGenerated, generatePassword, type Generated, type GeneratorOptions } from '../lib/api';
import { toastError } from '../lib/errors';
import { copiedText } from '../lib/format';
import {
  cleanMinimums,
  MAX_LENGTH,
  minimumOf,
  required,
  withMinimum,
  type CharSet,
} from '../lib/generator';
import { t, useLanguage } from '../lib/i18n';
import { getSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import {
  createMaskedAddress,
  has,
  maskedConnection,
  useUwu,
  type MaskedAddress,
  type MaskedConnection,
} from '../lib/uwu';
import { Colored } from './ItemDetail';
import { copyAddress, MaskedNotConnected } from './MaskedDialog';
import { Modal } from './Modal';
import { NyuBusy, playNyu } from './nyu/stage';

const KEY = 'uwulock.generator';

const DEFAULTS: GeneratorOptions = {
  length: 20,
  lowercase: true,
  uppercase: true,
  digits: true,
  symbols: true,
  avoidAmbiguous: false,
};

/** Only the options are remembered, never a password. */
export function loadOptions(): GeneratorOptions {
  try {
    const raw = JSON.parse(window.localStorage.getItem(KEY) ?? '{}') as Partial<GeneratorOptions>;
    const bool = (v: unknown, d: boolean) => (typeof v === 'boolean' ? v : d);
    const options: GeneratorOptions = {
      length:
        typeof raw.length === 'number' ? Math.min(128, Math.max(5, Math.round(raw.length))) : 20,
      lowercase: bool(raw.lowercase, DEFAULTS.lowercase),
      uppercase: bool(raw.uppercase, DEFAULTS.uppercase),
      digits: bool(raw.digits, DEFAULTS.digits),
      symbols: bool(raw.symbols, DEFAULTS.symbols),
      avoidAmbiguous: bool(raw.avoidAmbiguous, DEFAULTS.avoidAmbiguous),
      ...cleanMinimums(raw as Record<string, unknown>),
    };
    // Minimums that can't fit any more start over.
    return required(options) > MAX_LENGTH ? { ...options, ...cleanMinimums({}) } : options;
  } catch {
    return DEFAULTS;
  }
}

/** Remembers the options (the phone's generator page shares them). */
export function saveOptions(options: GeneratorOptions) {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(options));
  } catch {
    // Remembering is a convenience.
  }
}

export function strength(bits: number): { level: 1 | 2 | 3 | 4; label: string } {
  if (bits < 50) return { level: 1, label: t('schwach') };
  if (bits < 75) return { level: 2, label: t('okay') };
  if (bits < 100) return { level: 3, label: t('stark') };
  return { level: 4, label: t('sehr stark ✧') };
}

export function GeneratorDialog({
  onClose,
  onUse,
}: {
  onClose: () => void;
  /** Opened from the editor: the password goes into the field instead. */
  onUse?: (password: string) => void;
}) {
  useLanguage();
  const uwu = useUwu();
  // A masked address instead of a password: only on its own, not for the
  // editor's password field.
  const maskable = !onUse && has(uwu, 'masked-addresses');
  const [mode, setMode] = useState<'password' | 'masked'>('password');
  const [options, setOptions] = useState<GeneratorOptions>(loadOptions);
  const [result, setResult] = useState<Generated | null>(null);
  const ambiguousId = useId();

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
    if (!next.lowercase && !next.uppercase && !next.digits && !next.symbols) return;
    setOptions(next);
  };

  const copy = async () => {
    if (!result) return;
    try {
      await copyGenerated(result.password);
      toast(copiedText('password', getSettings().clipboardClear));
    } catch (e) {
      toastError(e);
    }
  };

  const meter = result ? strength(result.bits) : null;
  const sets: { key: CharSet; label: string }[] = [
    { key: 'uppercase', label: 'A–Z' },
    { key: 'lowercase', label: 'a–z' },
    { key: 'digits', label: '0–9' },
    { key: 'symbols', label: '!@#$%^&*' },
  ];

  if (maskable && mode === 'masked')
    return (
      <Modal
        title={t('Generator')}
        onCancel={onClose}
        footer={
          <>
            <span className="spacer" />
            <Button onClick={onClose}>{t('Schließen')}</Button>
          </>
        }
      >
        <ModeSwitch mode={mode} onChange={setMode} />
        <MaskedGenerator />
      </Modal>
    );

  return (
    <Modal
      title={maskable ? t('Generator') : t('Passwort-Generator')}
      onCancel={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('Schließen')}
          </Button>
          <span className="spacer" />
          <Button
            icon={ICONS.generate}
            onClick={() => {
              void roll(options);
              playNyu('generated');
            }}
          >
            {t('Neu würfeln')}
          </Button>
          {onUse ? (
            <Button
              variant="primary"
              icon={ICONS.done}
              data-autofocus
              disabled={!result}
              onClick={() => result && onUse(result.password)}
            >
              {t('Übernehmen')}
            </Button>
          ) : (
            <Button variant="primary" icon={ICONS.copy} data-autofocus onClick={() => void copy()}>
              {t('Kopieren')}
            </Button>
          )}
        </>
      }
    >
      {maskable && <ModeSwitch mode={mode} onChange={setMode} />}
      <div className="generator">
        <output className="generated" aria-live="polite">
          {result ? <Colored text={result.password} /> : '…'}
        </output>
        {meter && (
          <div className="meter" data-level={meter.level}>
            <span className="meter-bar">
              <span />
              <span />
              <span />
              <span />
            </span>
            <span className="meter-label">
              {meter.label} · {t('{bits} Bit', { bits: result?.bits ?? 0 })}
            </span>
          </div>
        )}
        <label className="field">
          <span>
            {t('Länge')} <b>{options.length}</b>
            {result && result.length > options.length && (
              <span className="generator-raised">
                {' '}
                {t('→ {n}, damit die Mindestanzahlen passen', { n: result.length })}
              </span>
            )}
          </span>
          <input
            type="range"
            min={5}
            max={64}
            value={Math.min(options.length, 64)}
            onChange={(e) => set({ length: Number(e.target.value) })}
          />
        </label>
        <div className="generator-sets" role="group" aria-label={t('Zeichen')}>
          {sets.map(({ key, label }) => (
            <label key={key} className="check chip-check">
              <input
                type="checkbox"
                checked={options[key]}
                onChange={(e) => set({ [key]: e.target.checked })}
              />
              <span className="mono">{label}</span>
            </label>
          ))}
        </div>
        <fieldset className="generator-mins">
          <legend>{t('Mindestens')}</legend>
          {sets
            .filter(({ key }) => options[key])
            .map(({ key, label }) => (
              <label key={key} className="mini-field">
                <span className="mono">{label}</span>
                <input
                  type="number"
                  min={1}
                  max={MAX_LENGTH}
                  value={minimumOf(options, key)}
                  aria-label={t('Mindestens {set}', { set: label })}
                  onChange={(e) => setOptions(withMinimum(options, key, Number(e.target.value)))}
                />
              </label>
            ))}
        </fieldset>
        <div className="flex items-center justify-between gap-3 text-meta">
          <label htmlFor={ambiguousId} className="cursor-pointer">
            {t('Verwechselbare Zeichen weglassen (l, 1, I, O, 0)')}
          </label>
          <Switch
            id={ambiguousId}
            checked={options.avoidAmbiguous}
            onChange={(on) => set({ avoidAmbiguous: on })}
          />
        </div>
      </div>
    </Modal>
  );
}

function ModeSwitch({
  mode,
  onChange,
}: {
  mode: 'password' | 'masked';
  onChange: (mode: 'password' | 'masked') => void;
}) {
  useLanguage();
  return (
    <Segmented
      label={t('Was erzeugt wird')}
      value={mode}
      onChange={onChange}
      className="w-full [&>button]:flex-1"
      options={[
        { value: 'password', label: t('Passwort') },
        { value: 'masked', label: t('Maskierte Adresse') },
      ]}
    />
  );
}

/** A new masked address from UwUMail, for a site, copied right away. */
export function MaskedGenerator() {
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

  if (!connection) return <NyuBusy label={t('Einen Moment …')} />;
  if (!connection.connected || connection.status === 'revoked')
    return <MaskedNotConnected connection={connection} />;

  const create = async () => {
    setBusy(true);
    try {
      const address = await createMaskedAddress(site || null, description || null, null);
      setMade(address);
      await copyAddress(address.email);
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="generator">
      <output className="generated mono" aria-live="polite">
        {made?.email ?? '…'}
      </output>
      <label className="field">
        <span>{t('Für Website')}</span>
        <input
          type="text"
          value={site}
          spellCheck={false}
          placeholder="shop.example.com"
          onChange={(e) => setSite(e.target.value)}
        />
      </label>
      <label className="field">
        <span>{t('Beschreibung')}</span>
        <input
          type="text"
          value={description}
          maxLength={200}
          onChange={(e) => setDescription(e.target.value)}
        />
      </label>
      <div className="form-actions">
        {made && (
          <Button icon={ICONS.copy} onClick={() => void copyAddress(made.email)}>
            {t('Kopieren')}
          </Button>
        )}
        <span className="spacer" />
        <Button
          variant="primary"
          icon={ICONS.maskedAddress}
          disabled={busy}
          onClick={() => void create()}
        >
          {made ? t('Noch eine') : t('Adresse erstellen')}
        </Button>
      </div>
    </div>
  );
}
