/**
 * The passkey window: a site asked to create a passkey, or to sign in with one. The background
 * opened this window and waits for the answer; closing it lets the browser's own authenticator
 * take over. Locked first? Then it unlocks here.
 */

import { useCallback, useEffect, useState, type FormEvent } from 'react';
import { Icon } from '../legacy/Icon';
import { t } from '../shared/i18n';
import type { PasskeyDecision, PasskeyPrompt } from '../shared/protocol';
import { passkeyDecide, passkeyPrompt } from '../popup/api';
import { errorText, PasswordInput, useSettings, useStatus } from '../popup/lib';
import { LockView } from '../popup/views/Lock';
import { LoginView } from '../popup/views/Login';
import { useArmed } from './armed';
import { passkeyLoginName } from './names';

export function Prompt({ id }: { id: string }) {
  useSettings();
  const [status, refresh] = useStatus();
  const [prompt, setPrompt] = useState<PasskeyPrompt | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [choice, setChoice] = useState<string>('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  // The main button only once the window was focused and visible for a moment (R4-5).
  const armed = useArmed();

  const load = useCallback(async () => {
    try {
      const next = await passkeyPrompt(id);
      setPrompt(next);
      if (next.kind === 'get' && next.choices[0]) {
        setChoice(`${next.choices[0].itemId}|${next.choices[0].credentialId}`);
      }
    } catch (e) {
      setError(errorText(e));
    }
  }, [id]);

  useEffect(() => {
    void load();
  }, [load, status?.state]);

  const decide = async (decision: PasskeyDecision) => {
    setBusy(true);
    setError(null);
    try {
      await passkeyDecide(decision);
      window.close();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (error && !prompt) {
    return (
      <div className="popup prompt">
        <p className="form-error">{error}</p>
      </div>
    );
  }
  if (!prompt || !status) return <div className="popup prompt" aria-busy />;

  if (status.state === 'logged-out') {
    return (
      <div className="popup prompt">
        <LoginView status={status} adding={false} onDone={() => void refresh()} />
      </div>
    );
  }
  if (status.state === 'locked') {
    return (
      <div className="popup prompt">
        <Header prompt={prompt} />
        <LockView status={status} compact onDone={() => void refresh()} />
        <div className="form-actions prompt-actions">
          <button
            type="button"
            className="quiet"
            onClick={() => void decide({ id, choice: 'browser' })}
          >
            {t('Browser verwenden')}
          </button>
        </div>
      </div>
    );
  }

  const needsPassword =
    prompt.userVerification === 'required' ||
    (prompt.kind === 'get' &&
      prompt.choices.find((c) => `${c.itemId}|${c.credentialId}` === choice)?.reprompt) ||
    (prompt.kind === 'create' && prompt.candidates.find((c) => c.id === choice)?.reprompt);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!armed) return;
    const secret = needsPassword ? password : null;
    if (prompt.kind === 'create') {
      void decide({ id, choice: 'create', itemId: choice || null, password: secret });
    } else {
      const [itemId, credentialId] = choice.split('|');
      if (itemId && credentialId) {
        void decide({ id, choice: 'use', itemId, credentialId, password: secret });
      }
    }
  };

  return (
    <form className="popup prompt popup-scroll form" onSubmit={submit}>
      <Header prompt={prompt} />

      {prompt.kind === 'create' && prompt.excluded && (
        <p className="notice" data-tone="error">
          {t('Für dieses Konto liegt schon ein Passkey in deinem Tresor.')}
        </p>
      )}

      {prompt.kind === 'create' && !prompt.excluded && (
        <div className="field">
          <span>{t('Speichern in')}</span>
          <label className="check">
            <input
              type="radio"
              name="target"
              checked={choice === ''}
              onChange={() => setChoice('')}
            />
            <span>
              {t('Neuer Login „{name}“', { name: passkeyLoginName(prompt.rpId, prompt.rpName) })}
            </span>
          </label>
          {prompt.candidates.map((candidate) => (
            <label className="check" key={candidate.id}>
              <input
                type="radio"
                name="target"
                checked={choice === candidate.id}
                onChange={() => setChoice(candidate.id)}
              />
              <span>
                {candidate.name}
                {candidate.subtitle && <span className="muted"> · {candidate.subtitle}</span>}
                {candidate.hasPasskey && (
                  <small className="muted">
                    {' '}
                    ({t('behält seine Passkeys, ersetzt nur einen für dasselbe Konto')})
                  </small>
                )}
              </span>
            </label>
          ))}
        </div>
      )}

      {prompt.kind === 'get' && prompt.choices.length === 0 && (
        <p className="dialog-lead">{t('In deinem Tresor ist kein Passkey für diese Seite.')}</p>
      )}
      {prompt.kind === 'get' && prompt.choices.length > 0 && (
        <div className="field">
          <span>{t('Anmelden als')}</span>
          {prompt.choices.map((c) => {
            const value = `${c.itemId}|${c.credentialId}`;
            return (
              <label className="check" key={value}>
                <input
                  type="radio"
                  name="passkey"
                  checked={choice === value}
                  onChange={() => setChoice(value)}
                />
                <span>
                  {c.userName ?? c.name}
                  <span className="muted"> · {c.name}</span>
                </span>
              </label>
            );
          })}
        </div>
      )}

      {needsPassword && (
        <label className="field">
          <span>{t('Master-Passwort')}</span>
          <PasswordInput value={password} onChange={setPassword} autoFocus disabled={busy} />
          <small className="field-hint">
            {t('Die Seite oder der Eintrag verlangt, dass du es bestätigst.')}
          </small>
        </label>
      )}

      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}

      <div className="form-actions prompt-actions">
        <button
          type="button"
          className="quiet"
          onClick={() => void decide({ id, choice: 'browser' })}
          disabled={busy}
        >
          {t('Browser verwenden')}
        </button>
        <span className="spacer" />
        {!(prompt.kind === 'get' && prompt.choices.length === 0) && (
          <button
            className="primary"
            type="submit"
            disabled={
              busy ||
              !armed ||
              (needsPassword ? !password : false) ||
              (prompt.kind === 'create' && prompt.excluded)
            }
          >
            {prompt.kind === 'create' ? t('Passkey speichern') : t('Anmelden')}
          </button>
        )}
      </div>
    </form>
  );
}

function Header({ prompt }: { prompt: PasskeyPrompt }) {
  return (
    <header className="prompt-head">
      <Icon name="key" size={22} />
      <div>
        <h1 className="card-title">
          {prompt.kind === 'create' ? t('Passkey erstellen') : t('Mit Passkey anmelden')}
        </h1>
        <p className="muted">
          {prompt.kind === 'create'
            ? t('{site} möchte einen Passkey für {user} anlegen.', {
                site: prompt.rpId,
                user: prompt.userName || '?',
              })
            : t('{site} möchte, dass du dich mit einem Passkey anmeldest.', { site: prompt.rpId })}
        </p>
        {prompt.kind === 'create' &&
          prompt.rpName &&
          prompt.rpName.toLowerCase() !== prompt.rpId.toLowerCase() && (
            <p className="muted">{t('Die Seite nennt sich „{name}“.', { name: prompt.rpName })}</p>
          )}
        <p className="prompt-origin mono">{prompt.origin}</p>
      </div>
    </header>
  );
}
