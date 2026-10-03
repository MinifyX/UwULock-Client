import { listen } from '@tauri-apps/api/event';
import { useEffect, useState, type FormEvent } from 'react';
import { errorText } from '../lib/errors';
import { t, useLanguage } from '../lib/i18n';
import { passkeyAnswer, passkeyRequest, type PasskeyRequest } from '../lib/passkeys';
import { Modal } from './Modal';
import { PasswordInput } from './PasswordInput';

/**
 * A browser or app asks for a passkey through UwULock's provider (the Linux
 * security key, the Windows plugin): nothing happens without a yes here.
 * Signing in picks the passkey, making one picks the login it goes into, and
 * a site that wants verification gets the master password typed again.
 *
 * While the vault is locked the request waits in a notice, so the lock
 * screen stays usable; it opens once the vault does.
 *
 * Any program can make a request turn up, also while the person is typing
 * elsewhere: the dialog starts on "Ablehnen", its yes only works after a
 * moment, the site's rpId comes first (a site picks its own name), and a
 * caller UwULock doesn't know as a browser gets a warning.
 */

/** How long the yes stays off after a request turns up, in ms. */
const ARM_AFTER = 700;
export function PasskeyRequestDialog() {
  useLanguage();
  const [request, setRequest] = useState<PasskeyRequest | null>(null);
  const [choice, setChoice] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [armed, setArmed] = useState(false);

  useEffect(() => {
    const load = () =>
      void passkeyRequest()
        .then((next) => {
          setRequest((current) => {
            if (next?.id !== current?.id) {
              setPassword('');
              setError(null);
              setBusy(false);
              setArmed(false);
              setChoice(
                next?.kind === 'get'
                  ? (next.passkeys[0]?.credentialId ?? '')
                  : (next?.logins.find((login) => !login.hasPasskey)?.itemId ?? ''),
              );
            }
            return next;
          });
        })
        .catch(() => setRequest(null));
    load();
    const stops = [listen('passkey-request', load), listen('vault-status', load)];
    return () => stops.forEach((stop) => void stop.then((unlisten) => unlisten()));
  }, []);

  const requestId = request?.id;
  useEffect(() => {
    if (requestId === undefined) return;
    const timer = window.setTimeout(() => setArmed(true), ARM_AFTER);
    return () => window.clearTimeout(timer);
  }, [requestId]);

  if (!request) return null;

  // The rpId is what the passkey is bound to; the name is the site's own
  // words and comes second.
  const site = request.rpName ? `${request.rpId} („${request.rpName}“)` : (request.rpId ?? '');
  const client = request.client || t('Ein Programm');
  const decline = () => {
    void passkeyAnswer({ id: request.id, allow: false }).catch(() => undefined);
    setRequest(null);
  };

  if (request.locked) {
    return (
      <div className="notice passkey-notice" role="alert">
        <span>
          {t('{client} möchte einen Passkey von UwULock. Entsperre den Tresor, um fortzufahren.', {
            client,
          })}
        </span>
        <button onClick={decline}>{t('Ablehnen')}</button>
      </div>
    );
  }

  const submit = async (event?: FormEvent) => {
    event?.preventDefault();
    if (blocked) return;
    setBusy(true);
    setError(null);
    try {
      const passkey = request.passkeys.find((p) => p.credentialId === choice);
      await passkeyAnswer({
        id: request.id,
        allow: true,
        itemId: request.kind === 'get' ? passkey?.itemId : choice || null,
        credentialId: request.kind === 'get' ? (passkey?.credentialId ?? null) : null,
        password: request.verify ? password : null,
      });
      setRequest(null);
    } catch (failed) {
      setError(errorText(failed));
      setBusy(false);
    }
  };

  const title =
    request.kind === 'create'
      ? t('Passkey sichern')
      : request.kind === 'get'
        ? t('Mit Passkey anmelden')
        : t('Sicherheitsschlüssel auswählen');
  const nothingToSign = request.kind === 'get' && request.passkeys.length === 0;
  const blocked =
    !armed || busy || nothingToSign || request.excluded || (request.verify && !password);

  return (
    <Modal
      title={title}
      onCancel={decline}
      footer={
        <>
          <span className="spacer" />
          <button data-secondary data-autofocus onClick={decline} disabled={busy}>
            {t('Ablehnen')}
          </button>
          <button className="primary" onClick={() => void submit()} disabled={blocked}>
            {request.kind === 'create'
              ? t('Sichern')
              : request.kind === 'get'
                ? t('Anmelden')
                : t('Diesen nehmen')}
          </button>
        </>
      }
    >
      <form className="passkey-request" onSubmit={(event) => void submit(event)}>
        {request.kind === 'select' ? (
          <p>{t('{client} sucht einen Sicherheitsschlüssel. UwULock ist einer.', { client })}</p>
        ) : (
          <>
            <p className="passkey-site">
              <strong>{request.rpId}</strong>
              {request.rpName && <small> · {request.rpName}</small>}
            </p>
            <p>
              {request.kind === 'create'
                ? t('{client} möchte für {site} einen Passkey in UwULock sichern.', {
                    client,
                    site,
                  })
                : t('{client} möchte dich mit einem Passkey bei {site} anmelden.', {
                    client,
                    site,
                  })}
            </p>
          </>
        )}

        {!request.trusted && (
          <p className="notice" data-tone="error">
            {t(
              'Das ist kein Browser, den UwULock kennt. Lehne ab, wenn du nicht gerade selbst einen Passkey benutzt.',
            )}
          </p>
        )}

        {request.trusted && (
          // Who asks is what the system says: a hint, not proof (docs/passkeys.md).
          <p className="setting-description">
            {t(
              'Wer fragt, sagt dein System. Das ist ein Hinweis, kein Beweis: Lehne ab, wenn du nicht gerade selbst eine Anmeldung gestartet hast.',
            )}
          </p>
        )}

        {request.kind === 'create' && (request.userName || request.userDisplayName) && (
          <p className="setting-description">
            {t('Konto: {name}', { name: request.userName ?? request.userDisplayName ?? '' })}
          </p>
        )}

        {request.excluded && (
          <p className="notice" data-tone="error">
            {t('Für dieses Konto ist schon ein Passkey im Tresor.')}
          </p>
        )}

        {request.kind === 'create' && !request.excluded && (
          <label className="field">
            <span>{t('Speichern in')}</span>
            <select value={choice} onChange={(event) => setChoice(event.target.value)}>
              <option value="">{t('Neues Login')}</option>
              {request.logins.map((login) => (
                <option key={login.itemId} value={login.itemId}>
                  {login.name}
                  {login.userName ? ` · ${login.userName}` : ''}
                  {login.hasPasskey ? ` (${t('ersetzt den Passkey')})` : ''}
                </option>
              ))}
            </select>
          </label>
        )}

        {request.kind === 'get' &&
          (nothingToSign ? (
            <p className="notice">{t('Im Tresor ist kein Passkey für diese Seite.')}</p>
          ) : (
            <fieldset className="passkey-choices">
              <legend>{t('Passkey')}</legend>
              {request.passkeys.map((passkey) => (
                <label key={passkey.credentialId}>
                  <input
                    type="radio"
                    name="passkey"
                    value={passkey.credentialId}
                    checked={choice === passkey.credentialId}
                    onChange={() => setChoice(passkey.credentialId)}
                  />
                  <span>
                    {passkey.userName ?? passkey.userDisplayName ?? passkey.itemName}
                    <small> · {passkey.itemName}</small>
                  </span>
                </label>
              ))}
            </fieldset>
          ))}

        {request.verify && request.kind !== 'select' && !nothingToSign && !request.excluded && (
          <label className="field">
            <span>{t('Die Seite möchte, dass du es bist: Master-Passwort')}</span>
            <PasswordInput value={password} onChange={setPassword} disabled={busy} />
          </label>
        )}

        {error && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
}
