import { Button, Icon, IconButton, ICONS, Segmented, Switch } from '@uwusuite/design';
import { useCallback, useEffect, useState } from 'react';
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
  useSettings,
  uwuFeature,
  when,
  WIDE_SEGMENTED,
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
      <Segmented
        className={WIDE_SEGMENTED}
        label={t('Art')}
        value={masked ? 'masked' : options.mode}
        onChange={(mode) => {
          if (mode === 'masked') {
            setMasked(true);
            return;
          }
          setMasked(false);
          change({ ...options, mode });
        }}
        options={[
          { value: 'password' as const, label: t('Passwort') },
          { value: 'passphrase' as const, label: t('Passphrase') },
          ...(maskable ? [{ value: 'masked' as const, label: t('Maskierte Adresse') }] : []),
        ]}
      />

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
            <Button
              icon={ICONS.generate}
              onClick={() => {
                void run(options);
                playNyu('generated');
              }}
            >
              {t('Neu')}
            </Button>
            <span className="spacer" />
            <Button
              variant="primary"
              icon={ICONS.copy}
              onClick={() => result && void copy(result.password)}
            >
              {t('Kopieren')}
            </Button>
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
                  <Switch
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
                <Switch
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
                <Switch
                  checked={pp.capitalize}
                  label={t('Großbuchstaben am Wortanfang')}
                  onChange={(checked) =>
                    change({ ...options, passphrase: { ...pp, capitalize: checked } })
                  }
                />
              </div>
              <div className="setting-row">
                <span className="setting-label">{t('Eine Zahl dazu')}</span>
                <Switch
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

      <Button
        variant="ghost"
        size="sm"
        icon={ICONS.history}
        className="history-toggle text-muted!"
        onClick={() => setShowHistory(!showHistory)}
        aria-expanded={showHistory}
      >
        {t('Zuletzt generiert ({n})', { n: history.length })}
        <Icon
          icon={ICONS.expand}
          size="xs"
          className={showHistory ? 'transition-transform' : '-rotate-90 transition-transform'}
        />
      </Button>
      {showHistory && (
        <ul className="history-list">
          {history.map((entry, index) => (
            <li key={index}>
              <span className="history-text">
                <Colored text={entry.password} />
                <small className="muted">{when(new Date(entry.date).toISOString())}</small>
              </span>
              <IconButton
                icon={ICONS.copy}
                size="sm"
                label={t('Kopieren')}
                onClick={() => void copy(entry.password)}
              />
            </li>
          ))}
          {history.length > 0 && (
            <li>
              <Button
                variant="ghost"
                size="sm"
                icon={ICONS.delete}
                onClick={() => void clearGeneratorHistory().then(() => setHistory([]))}
              >
                {t('Verlauf leeren')}
              </Button>
            </li>
          )}
        </ul>
      )}
    </div>
  );
}
