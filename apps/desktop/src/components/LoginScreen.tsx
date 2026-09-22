import { useEffect, useRef, useState, type FormEvent } from 'react';
import {
  login,
  loginCancel,
  loginNewDevice,
  loginSendEmail,
  loginTwoFactor,
  type LoginStep,
  type ServerKind,
  type Status,
  type TwoFactorMethod,
} from '../lib/api';
import { errorText } from '../lib/errors';
import { N_, t, useLanguage } from '../lib/i18n';
import { updateSettings, useSettings } from '../lib/settings';
import { Icon } from './Icon';
import { NyuScene } from './nyu/scenes';
import { PasswordInput } from './PasswordInput';

type Props = {
  /** Logging in again after the session expired: server and email are known. */
  again?: Status | null;
  onDone: (status: Status) => void;
  onCancel?: () => void;
};

const METHOD_LABEL: Record<TwoFactorMethod['kind'], string> = {
  authenticator: N_('Authenticator-App'),
  email: N_('E-Mail'),
  yubikey: 'YubiKey OTP',
  duo: 'Duo',
  webauthn: 'Passkey / FIDO2',
  u2f: 'FIDO U2F',
  other: '?',
};

/**
 * The first screen: which server, which account. Then, if the account wants
 * it, the two-step code. The master password is turned into the master key
 * and its hash right in Rust; only the hash goes to the server.
 */
export function LoginScreen({ again, onDone, onCancel }: Props) {
  useLanguage();
  const settings = useSettings();
  const [kind, setKind] = useState<ServerKind>(again?.serverKind ?? settings.lastServerKind);
  const [url, setUrl] = useState(again?.serverUrl ?? settings.lastServerUrl);
  const [email, setEmail] = useState(again?.email ?? settings.lastEmail);
  const [password, setPassword] = useState('');
  const [step, setStep] = useState<LoginStep | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const finish = (next: LoginStep) => {
    setPassword('');
    if (next.step === 'done') {
      updateSettings({ lastServerKind: kind, lastServerUrl: url.trim(), lastEmail: email.trim() });
      onDone(next.status);
      return;
    }
    setStep(next);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setError(null);
    setBusy(t('Nyu leitet deinen Schlüssel ab …'));
    try {
      finish(
        await login(
          kind === 'self-hosted' ? { kind, url: url.trim() } : { kind },
          email.trim(),
          password,
        ),
      );
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const back = () => {
    void loginCancel();
    setStep(null);
    setError(null);
  };

  const serverOptions: { value: ServerKind; label: string }[] = [
    { value: 'self-hosted', label: t('Selbst gehostet') },
    { value: 'bitwarden-us', label: 'bitwarden.com' },
    { value: 'bitwarden-eu', label: 'bitwarden.eu' },
  ];

  return (
    <div className="welcome">
      <section className="welcome-art" aria-hidden>
        <NyuScene name={step ? 'keys' : 'welcome'} className="welcome-scene" />
        <p className="welcome-title">
          {again ? t('Einmal neu anmelden, bitte') : t('Hallo! Ich bin Nyu ✧')}
        </p>
        <p className="welcome-text">
          {again
            ? t(
                'Der Server kennt die Sitzung dieses Geräts nicht mehr. Nach der Anmeldung geht es weiter wie vorher.',
              )
            : t(
                'UwULock öffnet deinen Tresor von Vaultwarden oder Bitwarden. Dein Master-Passwort verlässt dieses Gerät nie – der Server bekommt nur einen Hash davon.',
              )}
        </p>
      </section>

      <section className="welcome-card">
        {!step && (
          <form className="form" onSubmit={submit} aria-busy={Boolean(busy)}>
            <h1 className="card-title">{t('Anmelden')}</h1>
            <div className="field">
              <span>{t('Server')}</span>
              <div className="segmented wide" role="radiogroup" aria-label={t('Server')}>
                {serverOptions.map((option) => (
                  <button
                    key={option.value}
                    type="button"
                    role="radio"
                    aria-checked={kind === option.value}
                    onClick={() => setKind(option.value)}
                    disabled={Boolean(busy) || Boolean(again)}
                  >
                    {option.label}
                  </button>
                ))}
              </div>
            </div>
            {kind === 'self-hosted' && (
              <label className="field">
                <span>{t('Server-Adresse')}</span>
                <input
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder="https://vault.example.org"
                  autoComplete="url"
                  spellCheck={false}
                  required
                  disabled={Boolean(busy) || Boolean(again)}
                />
                <small className="field-hint">
                  {t('Die Adresse, unter der du deinen Web-Tresor öffnest.')}
                </small>
              </label>
            )}
            <label className="field">
              <span>{t('E-Mail-Adresse')}</span>
              <input
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                autoComplete="username"
                spellCheck={false}
                required
                disabled={Boolean(busy) || Boolean(again)}
              />
            </label>
            <label className="field">
              <span>{t('Master-Passwort')}</span>
              <PasswordInput
                value={password}
                onChange={setPassword}
                autoFocus={Boolean(again || (email && (kind !== 'self-hosted' || url)))}
                disabled={Boolean(busy)}
                autoComplete="current-password"
              />
            </label>
            {error && (
              <p className="form-error" role="alert">
                {error}
              </p>
            )}
            <div className="form-actions">
              {onCancel && (
                <button type="button" className="quiet" onClick={onCancel} disabled={Boolean(busy)}>
                  {t('Abbrechen')}
                </button>
              )}
              <span className="spacer" />
              <button className="primary" type="submit" disabled={Boolean(busy) || !password}>
                {busy ?? t('Anmelden')}
              </button>
            </div>
            <p className="welcome-beta">
              <Icon name="sparkles" size={14} />
              {t(
                'Beta: Ansehen, Suchen, Kopieren und Einmal-Codes. Bearbeiten kommt in einer der nächsten Versionen.',
              )}
            </p>
          </form>
        )}

        {step?.step === 'two-factor' && (
          <TwoFactor methods={step.methods} message={step.message} onBack={back} onDone={finish} />
        )}

        {step?.step === 'new-device' && <NewDevice onBack={back} onDone={finish} />}
      </section>
    </div>
  );
}

function TwoFactor({
  methods,
  message,
  onBack,
  onDone,
}: {
  methods: TwoFactorMethod[];
  message: string | null;
  onBack: () => void;
  onDone: (step: LoginStep) => void;
}) {
  useLanguage();
  const usable = methods.filter((m) => m.supported);
  const [provider, setProvider] = useState<number | null>(usable[0]?.provider ?? null);
  const [code, setCode] = useState('');
  const [remember, setRemember] = useState(false);
  const [busy, setBusy] = useState(false);
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<string | null>(message);
  const input = useRef<HTMLInputElement>(null);
  const method = usable.find((m) => m.provider === provider);

  useEffect(() => setError(message), [message]);
  useEffect(() => input.current?.focus(), [provider]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (provider === null) return;
    setBusy(true);
    setError(null);
    try {
      const next = await loginTwoFactor(provider, code, remember);
      if (next.step === 'two-factor') {
        setError(t('Der Code wurde nicht angenommen.'));
        setCode('');
      } else onDone(next);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const sendEmail = async () => {
    setError(null);
    try {
      await loginSendEmail();
      setSent(true);
    } catch (e) {
      setError(errorText(e));
    }
  };

  if (!method) {
    return (
      <div className="form">
        <h1 className="card-title">{t('Zweistufige Anmeldung')}</h1>
        <p className="dialog-lead">
          {t(
            'Dein Konto verlangt eine Methode, die UwULock noch nicht kann ({methods}). Richte im Web-Tresor zusätzlich eine Authenticator-App oder E-Mail-Codes ein.',
            {
              methods: methods.map((m) => t(METHOD_LABEL[m.kind])).join(', '),
            },
          )}
        </p>
        <div className="form-actions">
          <button type="button" onClick={onBack}>
            {t('Zurück')}
          </button>
        </div>
      </div>
    );
  }

  return (
    <form className="form" onSubmit={submit}>
      <h1 className="card-title">{t('Zweistufige Anmeldung')}</h1>
      {usable.length > 1 && (
        <div className="segmented wide" role="radiogroup" aria-label={t('Methode')}>
          {usable.map((m) => (
            <button
              key={m.provider}
              type="button"
              role="radio"
              aria-checked={m.provider === provider}
              onClick={() => {
                setProvider(m.provider);
                setCode('');
                setError(null);
              }}
            >
              {t(METHOD_LABEL[m.kind])}
            </button>
          ))}
        </div>
      )}
      <p className="dialog-lead">
        {method.kind === 'authenticator' &&
          t('Gib den sechsstelligen Code aus deiner Authenticator-App ein.')}
        {method.kind === 'email' &&
          (sent
            ? t('Der Code ist unterwegs an {email}.', {
                email: method.hint ?? t('deine E-Mail-Adresse'),
              })
            : t('Lass dir einen Code an {email} schicken und gib ihn hier ein.', {
                email: method.hint ?? t('deine E-Mail-Adresse'),
              }))}
        {method.kind === 'yubikey' && t('Stecke deinen YubiKey ein und tippe ihn an.')}
      </p>
      {methods.some((m) => !m.supported) && (
        <p className="field-hint">
          {t('Noch nicht unterstützt: {methods}.', {
            methods: methods
              .filter((m) => !m.supported)
              .map((m) => t(METHOD_LABEL[m.kind]))
              .join(', '),
          })}
        </p>
      )}
      <label className="field">
        <span>{t('Code')}</span>
        <input
          ref={input}
          className="code-input"
          value={code}
          onChange={(e) => setCode(e.target.value)}
          inputMode={method.kind === 'yubikey' ? 'text' : 'numeric'}
          autoComplete="one-time-code"
          spellCheck={false}
          required
          disabled={busy}
        />
      </label>
      <label className="check">
        <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} />
        <span>{t('Auf diesem Gerät merken')}</span>
      </label>
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      <div className="form-actions">
        <button type="button" className="quiet" onClick={onBack} disabled={busy}>
          {t('Zurück')}
        </button>
        <span className="spacer" />
        {method.kind === 'email' && (
          <button type="button" onClick={() => void sendEmail()} disabled={busy}>
            {sent ? t('Nochmal senden') : t('Code senden')}
          </button>
        )}
        <button className="primary" type="submit" disabled={busy || !code.trim()}>
          {busy ? t('Prüft …') : t('Weiter')}
        </button>
      </div>
    </form>
  );
}

function NewDevice({ onBack, onDone }: { onBack: () => void; onDone: (step: LoginStep) => void }) {
  useLanguage();
  const [code, setCode] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return (
    <form
      className="form"
      onSubmit={async (event) => {
        event.preventDefault();
        setBusy(true);
        setError(null);
        try {
          const next = await loginNewDevice(code);
          if (next.step === 'new-device') setError(t('Der Code wurde nicht angenommen.'));
          else onDone(next);
        } catch (e) {
          setError(errorText(e));
        } finally {
          setBusy(false);
        }
      }}
    >
      <h1 className="card-title">{t('Neues Gerät bestätigen')}</h1>
      <p className="dialog-lead">
        {t('Bitwarden kennt dieses Gerät noch nicht und hat dir einen Code per E-Mail geschickt.')}
      </p>
      <label className="field">
        <span>{t('Code aus der E-Mail')}</span>
        <input
          className="code-input"
          value={code}
          onChange={(e) => setCode(e.target.value)}
          autoComplete="one-time-code"
          autoFocus
          required
          disabled={busy}
        />
      </label>
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      <div className="form-actions">
        <button type="button" className="quiet" onClick={onBack} disabled={busy}>
          {t('Zurück')}
        </button>
        <span className="spacer" />
        <button className="primary" type="submit" disabled={busy || !code.trim()}>
          {busy ? t('Prüft …') : t('Weiter')}
        </button>
      </div>
    </form>
  );
}
