import { useEffect, useRef, useState, type FormEvent } from 'react';
import { Icon } from '../../legacy/Icon';
import { NyuScene } from '@desktop/components/nyu/scenes';
import { normalizeServerUrl } from '../../background/server';
import { N_, t } from '../../shared/i18n';
import type {
  LoginStep,
  PendingLoginInfo,
  ServerChoice,
  ServerKind,
  Status,
  TwoFactorMethod,
} from '../../shared/protocol';
import {
  forgetKdf,
  login,
  loginCancel,
  loginNewDevice,
  loginSendEmail,
  loginTwoFactor,
  loginWebAuthn,
  requestServerPermission,
} from '../api';
import { RequestFailed } from '../../shared/messages';
import { errorText, PasswordInput, serverUrlError } from '../lib';

const METHOD_LABEL: Record<TwoFactorMethod['kind'], string> = {
  authenticator: N_('Authenticator-App'),
  email: N_('E-Mail'),
  yubikey: 'YubiKey OTP',
  duo: 'Duo',
  webauthn: N_('Sicherheitsschlüssel'),
  u2f: 'FIDO U2F',
  other: '?',
};

const LAST = 'uwulock.lastLogin';

type Last = { kind: ServerKind; url: string; email: string };

function lastLogin(): Last {
  try {
    const saved = JSON.parse(localStorage.getItem(LAST) ?? 'null') as Last | null;
    if (saved) return saved;
  } catch {
    // Nothing saved.
  }
  return { kind: 'self-hosted', url: '', email: '' };
}

/**
 * Which server, which account; then, if the account wants it, the second step. The master
 * password goes to the background, which turns it into the master key and its hash; only the
 * hash goes to the server.
 */
export function LoginView({
  status,
  adding,
  onDone,
  onCancel,
}: {
  status: Status;
  adding: boolean;
  onDone: () => void;
  onCancel?: () => void;
}) {
  const last = lastLogin();
  const [kind, setKind] = useState<ServerKind>(last.kind);
  const [url, setUrl] = useState(adding ? '' : last.url);
  const [email, setEmail] = useState(adding ? '' : last.email);
  const [password, setPassword] = useState('');
  const [step, setStep] = useState<LoginStep | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(
    status.sessionExpired
      ? t('Der Server hat dieses Gerät abgemeldet. Bitte melde dich neu an.')
      : null,
  );
  /** The server asked for a weaker KDF than the last login: where to forget what was stored. */
  const [weaker, setWeaker] = useState<{ server: ServerChoice; email: string } | null>(null);
  const pending: PendingLoginInfo | null = status.login;

  const finish = (next: LoginStep) => {
    setPassword('');
    if (next.step === 'done') {
      localStorage.setItem(LAST, JSON.stringify({ kind, url: url.trim(), email: email.trim() }));
      onDone();
      return;
    }
    setStep(next);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setError(null);
    setWeaker(null);
    let choice: ServerChoice;
    try {
      choice = kind === 'self-hosted' ? { kind, url: normalizeServerUrl(url) } : { kind };
    } catch (e) {
      setError(serverUrlError(e instanceof Error ? e.message : ''));
      return;
    }
    // Asked right away, while the click still counts: the browser only asks during one.
    const allowed = requestServerPermission(choice);
    void (async () => {
      if (!(await allowed)) {
        setError(
          t(
            'Ohne die Erlaubnis kann UwULock deinen Server nicht erreichen. Versuche es noch einmal und erlaube den Zugriff.',
          ),
        );
        return;
      }
      setBusy(t('Nyu leitet deinen Schlüssel ab …'));
      try {
        finish(await login(choice, email.trim(), password));
      } catch (e) {
        setError(errorText(e));
        if (e instanceof RequestFailed && e.kind === 'weaker-kdf') {
          setWeaker({ server: choice, email: email.trim() });
        }
      } finally {
        setBusy(null);
      }
    })();
  };

  // Only after a refusal, and only by hand: whoever lowered the KDF on purpose accepts it here.
  const forget = () => {
    if (!weaker) return;
    void forgetKdf(weaker.server, weaker.email)
      .then(() => {
        setWeaker(null);
        setError(t('Vergessen. Die nächste Anmeldung übernimmt die Einstellung des Servers.'));
      })
      .catch((e: unknown) => setError(errorText(e)));
  };

  const back = () => {
    void loginCancel();
    setStep(null);
    setError(null);
  };

  // A second step that was waiting when the popup closed (somebody fetched their code).
  const resumed: LoginStep | null =
    step ??
    (pending
      ? pending.step === 'new-device'
        ? { step: 'new-device' }
        : { step: 'two-factor', methods: pending.methods, message: pending.message }
      : null);

  const serverOptions: { value: ServerKind; label: string }[] = [
    { value: 'self-hosted', label: t('Selbst gehostet') },
    { value: 'bitwarden-us', label: 'bitwarden.com' },
    { value: 'bitwarden-eu', label: 'bitwarden.eu' },
  ];

  return (
    <div className="popup-scroll login">
      <div className="login-head" aria-hidden>
        <NyuScene name={resumed ? 'keys' : 'welcome'} className="login-scene" />
      </div>
      {!resumed && (
        <form className="form" onSubmit={submit} aria-busy={Boolean(busy)}>
          <h1 className="card-title">{adding ? t('Noch ein Konto ✧') : t('Anmelden')}</h1>
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
                  disabled={Boolean(busy)}
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
                disabled={Boolean(busy)}
              />
              <small className="field-hint">
                {t('UwULock Server, Vaultwarden oder Bitwarden: die Adresse deines Web-Tresors.')}
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
              disabled={Boolean(busy)}
            />
          </label>
          <label className="field">
            <span>{t('Master-Passwort')}</span>
            <PasswordInput
              value={password}
              onChange={setPassword}
              autoFocus={Boolean(email && (kind !== 'self-hosted' || url))}
              disabled={Boolean(busy)}
            />
          </label>
          {error && (
            <p className="form-error" role="alert">
              {error}
            </p>
          )}
          {weaker && (
            <button type="button" className="quiet" onClick={forget} disabled={Boolean(busy)}>
              {t('Ich habe sie selbst gesenkt: gespeicherte Einstellung vergessen')}
            </button>
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
          <p className="field-hint">
            {t(
              'Dein Master-Passwort verlässt diesen Browser nie – der Server bekommt nur einen Hash davon.',
            )}
          </p>
        </form>
      )}
      {resumed?.step === 'two-factor' && (
        <TwoFactor
          methods={resumed.methods}
          message={resumed.message}
          webVault={status.webVault}
          onBack={back}
          onDone={finish}
        />
      )}
      {resumed?.step === 'new-device' && <NewDevice onBack={back} onDone={finish} />}
    </div>
  );
}

function TwoFactor({
  methods,
  message,
  webVault,
  onBack,
  onDone,
}: {
  methods: TwoFactorMethod[];
  message: string | null;
  webVault: string | null;
  onBack: () => void;
  onDone: (step: LoginStep) => void;
}) {
  const usable = methods.filter((m) => m.supported);
  const [provider, setProvider] = useState<number | null>(usable[0]?.provider ?? null);
  const [code, setCode] = useState('');
  const [remember, setRemember] = useState(true);
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

  const recovery = (
    <p className="field-hint">
      {t(
        'Kein Zugriff mehr? Mit deinem Wiederherstellungscode schaltest du die zweistufige Anmeldung im Web-Tresor ab.',
      )}{' '}
      {webVault && (
        <a href={webVault} target="_blank" rel="noreferrer">
          {t('Web-Tresor öffnen')}
        </a>
      )}
    </p>
  );

  if (!method) {
    return (
      <div className="form">
        <h1 className="card-title">{t('Zweistufige Anmeldung')}</h1>
        <p className="dialog-lead">
          {t(
            'Dein Konto verlangt eine Methode, die UwULock noch nicht kann ({methods}). Richte im Web-Tresor zusätzlich eine Authenticator-App oder E-Mail-Codes ein.',
            { methods: methods.map((m) => t(METHOD_LABEL[m.kind])).join(', ') },
          )}
        </p>
        {recovery}
        <div className="form-actions">
          <button type="button" onClick={onBack}>
            {t('Zurück')}
          </button>
        </div>
      </div>
    );
  }

  const picker = usable.length > 1 && (
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
  );

  if (method.kind === 'webauthn') {
    return (
      <div className="form">
        <h1 className="card-title">{t('Zweistufige Anmeldung')}</h1>
        {picker}
        <p className="dialog-lead">
          {t(
            'Dein Server öffnet eine Seite in einem neuen Tab. Dort bestätigst du mit deinem Sicherheitsschlüssel; danach öffnest du UwULock wieder.',
          )}
        </p>
        <label className="check">
          <input
            type="checkbox"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
          />
          <span>{t('Auf diesem Gerät merken')}</span>
        </label>
        {error && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
        {recovery}
        <div className="form-actions">
          <button type="button" className="quiet" onClick={onBack}>
            {t('Zurück')}
          </button>
          <span className="spacer" />
          <button
            type="button"
            className="primary"
            onClick={() => void loginWebAuthn(remember).catch((e) => setError(errorText(e)))}
          >
            <Icon name="key" size={14} /> {t('Sicherheitsschlüssel verwenden')}
          </button>
        </div>
      </div>
    );
  }

  return (
    <form className="form" onSubmit={submit}>
      <h1 className="card-title">{t('Zweistufige Anmeldung')}</h1>
      {picker}
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
      {recovery}
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
  const [code, setCode] = useState('');
  const [busy, setBusy] = useState(false);
  const [sent, setSent] = useState(false);
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
        {t(
          'Der Server kennt diesen Browser noch nicht und hat dir einen Code per E-Mail geschickt.',
        )}
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
        <button
          type="button"
          onClick={() =>
            void loginSendEmail()
              .then(() => setSent(true))
              .catch((e) => setError(errorText(e)))
          }
          disabled={busy}
        >
          {sent ? t('Gesendet ✧') : t('Nochmal senden')}
        </button>
        <button className="primary" type="submit" disabled={busy || !code.trim()}>
          {busy ? t('Prüft …') : t('Weiter')}
        </button>
      </div>
    </form>
  );
}
