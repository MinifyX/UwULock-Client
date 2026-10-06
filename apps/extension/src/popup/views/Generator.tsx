import { useCallback, useEffect, useState } from 'react';
import { Icon } from '../../legacy/Icon';
import { playNyu } from '@desktop/components/nyu/stage';
import { MAX_LENGTH, minimumOf, withMinimum, type CharSet } from '@desktop/lib/generator';
import { t } from '../../shared/i18n';
import type { Generated, GeneratorSettings, Status } from '../../shared/protocol';
import { clearGeneratorHistory, copyText, generate, generatorHistory, setSettings } from '../api';
import {
  Colored,
  copiedText,
  publishSettings,
  toast,
  toastError,
  Toggle,
  useSettings,
  uwuFeature,
  when,
} from '../lib';
import { MaskedPanel } from './Masked';

/**
 * Passwords and passphrases, with the settings kept for next time, and the last ones generated
 * until the browser closes.
 */
export function Generator({ status }: { status: Status }) {
  const settings = useSettings();
  // Masked addresses are made only on a click, so this mode is not kept for next time.
  const [maskedChosen, setMasked] = useState(false);
  const maskable = uwuFeature(status, 'masked-addresses');
  // Switched off by the admin meanwhile: back to passwords.
  const masked = maskedChosen && maskable;
  const [options, setOptions] = useState<GeneratorSettings | null>(settings?.generator ?? null);
  const [result, setResult] = useState<{
    password: string;
    bits: number;
    length?: number;
  } | null>(null);
  const [history, setHistory] = useState<Generated[]>([]);
  const [showHistory, setShowHistory] = useState(false);

  const run = useCallback(async (next: GeneratorSettings) => {
    try {
      setResult(await generate(next));
      setHistory(await generatorHistory());
    } catch (e) {
      toastError(e);
    }
  }, []);

  useEffect(() => {
    if (options) void run(options);
    // Once, with what was saved; every change runs it again in `change`.
  }, []);

  if (!options || !settings) return <div className="popup-scroll" aria-busy />;

  const change = (next: GeneratorSettings) => {
    setOptions(next);
    void run(next);
    void setSettings({ generator: next }).then(publishSettings, () => undefined);
  };

  const copy = async (text: string) => {
    try {
      await copyText(text);
      toast(copiedText('password', settings.clipboardClear));
    } catch (e) {
      toastError(e);
    }
  };

  const pw = options.password;
  const pp = options.passphrase;
  const level = !result
    ? 0
    : result.bits < 50
      ? 1
      : result.bits < 70
        ? 2
        : result.bits < 100
          ? 3
          : 4;

  return (
    <div className="popup-scroll generator">
      <div className="segmented wide" role="radiogroup" aria-label={t('Art')}>
        <button
          type="button"
          role="radio"
          aria-checked={!masked && options.mode === 'password'}
          onClick={() => {
            setMasked(false);
            change({ ...options, mode: 'password' });
          }}
        >
          {t('Passwort')}
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={!masked && options.mode === 'passphrase'}
          onClick={() => {
            setMasked(false);
            change({ ...options, mode: 'passphrase' });
          }}
        >
          {t('Passphrase')}
        </button>
        {maskable && (
          <button type="button" role="radio" aria-checked={masked} onClick={() => setMasked(true)}>
            {t('Maskierte Adresse')}
          </button>
        )}
      </div>

      {masked ? (
        <MaskedPanel />
      ) : (
        <>
          <div className="generated">{result ? <Colored text={result.password} /> : '…'}</div>
          <div
            className="meter"
            data-level={level}
            aria-label={t('Stärke: {bits} Bit', { bits: result?.bits ?? 0 })}
          >
            <span className="meter-bar">
              <span />
              <span />
              <span />
              <span />
            </span>
            <span className="muted">{t('{bits} Bit', { bits: result?.bits ?? 0 })}</span>
          </div>
          <div className="form-actions">
            <button
              type="button"
              onClick={() => {
                void run(options);
                playNyu('generated');
              }}
            >
              <Icon name="refresh" size={14} /> {t('Neu')}
            </button>
            <span className="spacer" />
            <button
              type="button"
              className="primary"
              onClick={() => result && void copy(result.password)}
            >
              <Icon name="copy" size={14} /> {t('Kopieren')}
            </button>
          </div>

          {options.mode === 'password' ? (
            <div className="setting-list">
              <label className="field">
                <span>
                  {t('Länge: {n}', { n: pw.length })}
                  {options.mode === 'password' && result?.length && result.length > pw.length && (
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
                  value={pw.length}
                  onChange={(e) =>
                    change({ ...options, password: { ...pw, length: Number(e.target.value) } })
                  }
                />
              </label>
              {(
                [
                  ['uppercase', 'A–Z'],
                  ['lowercase', 'a–z'],
                  ['digits', '0–9'],
                  ['symbols', '!@#$%^&*'],
                ] as const
              ).map(([key, label]) => (
                <div className="setting-row" key={key}>
                  <span className="setting-label mono">{label}</span>
                  <Toggle
                    checked={pw[key]}
                    label={label}
                    onChange={(checked) =>
                      change({ ...options, password: { ...pw, [key]: checked } })
                    }
                  />
                </div>
              ))}
              <fieldset className="generator-mins">
                <legend>{t('Mindestens')}</legend>
                {(
                  [
                    ['uppercase', 'A–Z'],
                    ['lowercase', 'a–z'],
                    ['digits', '0–9'],
                    ['symbols', '!@#$%^&*'],
                  ] as [CharSet, string][]
                )
                  .filter(([key]) => pw[key])
                  .map(([key, label]) => (
                    <label key={key} className="mini-field">
                      <span className="mono">{label}</span>
                      <input
                        type="number"
                        min={1}
                        max={MAX_LENGTH}
                        value={minimumOf(pw, key)}
                        aria-label={t('Mindestens {set}', { set: label })}
                        onChange={(e) =>
                          change({
                            ...options,
                            password: withMinimum(pw, key, Number(e.target.value)),
                          })
                        }
                      />
                    </label>
                  ))}
              </fieldset>
              <div className="setting-row">
                <span className="setting-label">{t('Verwechselbare Zeichen weglassen')}</span>
                <Toggle
                  checked={pw.avoidAmbiguous}
                  label={t('Verwechselbare Zeichen weglassen')}
                  onChange={(checked) =>
                    change({ ...options, password: { ...pw, avoidAmbiguous: checked } })
                  }
                />
              </div>
            </div>
          ) : (
            <div className="setting-list">
              <label className="field">
                <span>{t('Wörter: {n}', { n: pp.words })}</span>
                <input
                  type="range"
                  min={3}
                  max={20}
                  value={pp.words}
                  onChange={(e) =>
                    change({ ...options, passphrase: { ...pp, words: Number(e.target.value) } })
                  }
                />
              </label>
              <label className="field">
                <span>{t('Trennzeichen')}</span>
                <input
                  value={pp.separator}
                  maxLength={3}
                  onChange={(e) =>
                    change({ ...options, passphrase: { ...pp, separator: e.target.value } })
                  }
                />
              </label>
              <div className="setting-row">
                <span className="setting-label">{t('Großbuchstaben am Wortanfang')}</span>
                <Toggle
                  checked={pp.capitalize}
                  label={t('Großbuchstaben am Wortanfang')}
                  onChange={(checked) =>
                    change({ ...options, passphrase: { ...pp, capitalize: checked } })
                  }
                />
              </div>
              <div className="setting-row">
                <span className="setting-label">{t('Eine Zahl dazu')}</span>
                <Toggle
                  checked={pp.includeNumber}
                  label={t('Eine Zahl dazu')}
                  onChange={(checked) =>
                    change({ ...options, passphrase: { ...pp, includeNumber: checked } })
                  }
                />
              </div>
            </div>
          )}
        </>
      )}

      <button
        type="button"
        className="history-toggle quiet"
        onClick={() => setShowHistory(!showHistory)}
        aria-expanded={showHistory}
      >
        <Icon name="history" size={13} /> {t('Zuletzt generiert ({n})', { n: history.length })}
      </button>
      {showHistory && (
        <ul className="history-list">
          {history.map((entry, index) => (
            <li key={index}>
              <span className="history-text">
                <Colored text={entry.password} />
                <small className="muted">{when(new Date(entry.date).toISOString())}</small>
              </span>
              <button
                type="button"
                className="icon-button"
                onClick={() => void copy(entry.password)}
                aria-label={t('Kopieren')}
                title={t('Kopieren')}
              >
                <Icon name="copy" size={14} />
              </button>
            </li>
          ))}
          {history.length > 0 && (
            <li>
              <button
                type="button"
                className="quiet"
                onClick={() => void clearGeneratorHistory().then(() => setHistory([]))}
              >
                {t('Verlauf leeren')}
              </button>
            </li>
          )}
        </ul>
      )}
    </div>
  );
}
