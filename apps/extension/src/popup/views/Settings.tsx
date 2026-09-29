import { useState, type ReactNode } from 'react';
import { Icon } from '@desktop/components/Icon';
import { ext } from '../../shared/browser';
import { N_, t } from '../../shared/i18n';
import type { LockTimeout, Settings, Status } from '../../shared/protocol';
import { logout, setPin, setSettings, switchAccount, syncNow } from '../api';
import {
  ago,
  errorText,
  PasswordInput,
  publishSettings,
  toast,
  Toggle,
  useSettings,
  uwuFeature,
} from '../lib';

const TIMEOUTS: { value: LockTimeout; label: string }[] = [
  { value: 0, label: N_('Sobald das Fenster zugeht') },
  { value: 1, label: N_('Nach 1 Minute') },
  { value: 5, label: N_('Nach 5 Minuten') },
  { value: 15, label: N_('Nach 15 Minuten') },
  { value: 30, label: N_('Nach 30 Minuten') },
  { value: 60, label: N_('Nach 1 Stunde') },
  { value: 240, label: N_('Nach 4 Stunden') },
  { value: -1, label: N_('Erst beim Neustart des Browsers') },
];

const CLIPBOARD: { value: number; label: string }[] = [
  { value: 10, label: N_('Nach 10 Sekunden') },
  { value: 30, label: N_('Nach 30 Sekunden') },
  { value: 60, label: N_('Nach 1 Minute') },
  { value: 120, label: N_('Nach 2 Minuten') },
  { value: 0, label: N_('Nie') },
];

const MATCHES: { value: number; label: string }[] = [
  { value: 0, label: N_('Domain') },
  { value: 1, label: N_('Host') },
  { value: 2, label: N_('Beginnt mit') },
  { value: 3, label: N_('Genau') },
  { value: 5, label: N_('Nie') },
];

function Row({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div className="setting-row">
      <span className="setting-text">
        <span className="setting-label">{label}</span>
        {description && <span className="setting-description">{description}</span>}
      </span>
      <span className="setting-control">{children}</span>
    </div>
  );
}

/** Accounts, locking, filling, looks, and what this is. */
export function SettingsView({
  status,
  onAddAccount,
  onFileRequests,
}: {
  status: Status;
  onAddAccount: () => void;
  onFileRequests: () => void;
}) {
  const settings = useSettings();
  const [pinOpen, setPinOpen] = useState(false);
  if (!settings) return null;

  const change = async (patch: Partial<Settings>) => {
    try {
      publishSettings(await setSettings(patch));
    } catch (e) {
      toast(errorText(e), 'error');
    }
  };

  const manifest = ext.runtime.getManifest() as chrome.runtime.Manifest & { version_name?: string };
  const version = manifest.version_name ?? manifest.version;

  return (
    <div className="popup-scroll settings-view">
      <h2 className="section-title">{t('Konten')}</h2>
      <div className="setting-list">
        {status.accounts.map((account) => (
          <div className="setting-row" key={account.id}>
            <span className="setting-text">
              <span className="setting-label">{account.email}</span>
              <span className="setting-description">
                {account.server}
                {account.active &&
                  ` · ${t('Letzte Synchronisierung: {when}', { when: ago(status.lastSync) })}`}
              </span>
            </span>
            <span className="setting-control">
              {!account.active && (
                <button
                  type="button"
                  onClick={() =>
                    void switchAccount(account.id).catch((e) => toast(errorText(e), 'error'))
                  }
                >
                  {t('Wechseln')}
                </button>
              )}
              {account.active && (
                <button
                  type="button"
                  className="icon-button"
                  onClick={() =>
                    void syncNow().then(
                      () => toast(t('Synchronisiert ✧')),
                      (e) => toast(errorText(e), 'error'),
                    )
                  }
                  aria-label={t('Jetzt synchronisieren')}
                  title={t('Jetzt synchronisieren')}
                >
                  <Icon name="refresh" size={15} />
                </button>
              )}
              <button
                type="button"
                className="icon-button"
                onClick={() => {
                  if (
                    window.confirm(
                      t('{email} auf diesem Browser abmelden?', { email: account.email }),
                    )
                  )
                    void logout(account.id).catch((e) => toast(errorText(e), 'error'));
                }}
                aria-label={t('Abmelden')}
                title={t('Abmelden')}
              >
                <Icon name="logout" size={15} />
              </button>
            </span>
          </div>
        ))}
        <button type="button" className="quiet add-line" onClick={onAddAccount}>
          <Icon name="plus" size={13} /> {t('Konto hinzufügen')}
        </button>
        {status.webVault && (
          <a className="add-line" href={status.webVault} target="_blank" rel="noreferrer">
            <Icon name="external" size={13} /> {t('Web-Tresor öffnen')}
          </a>
        )}
      </div>

      <h2 className="section-title">{t('Sperren')}</h2>
      <div className="setting-list">
        <Row label={t('Automatisch sperren')}>
          <select
            className="select"
            value={settings.lockTimeout}
            onChange={(e) => void change({ lockTimeout: Number(e.target.value) as LockTimeout })}
          >
            {TIMEOUTS.map((option) => (
              <option key={option.value} value={option.value}>
                {t(option.label)}
              </option>
            ))}
          </select>
        </Row>
        <Row
          label={t('Mit dem Computer sperren')}
          description={t('Sperrt UwULock, sobald der Bildschirm gesperrt wird.')}
        >
          <Toggle
            checked={settings.lockWithSystem}
            label={t('Mit dem Computer sperren')}
            onChange={(v) => void change({ lockWithSystem: v })}
          />
        </Row>
        <Row
          label={t('Mit PIN entsperren')}
          description={
            status.pinSet
              ? t('Eingerichtet. Nach fünf falschen Versuchen wird sie gelöscht.')
              : t('Statt des Master-Passworts, in diesem Browser.')
          }
        >
          {status.pinSet ? (
            <button
              type="button"
              onClick={() => void setPin(null, false).catch((e) => toast(errorText(e), 'error'))}
            >
              {t('Entfernen')}
            </button>
          ) : (
            <button type="button" onClick={() => setPinOpen(!pinOpen)}>
              {t('Einrichten')}
            </button>
          )}
        </Row>
        {pinOpen && !status.pinSet && <PinForm onDone={() => setPinOpen(false)} />}
        <Row label={t('Zwischenablage leeren')}>
          <select
            className="select"
            value={settings.clipboardClear}
            onChange={(e) => void change({ clipboardClear: Number(e.target.value) })}
          >
            {CLIPBOARD.map((option) => (
              <option key={option.value} value={option.value}>
                {t(option.label)}
              </option>
            ))}
          </select>
        </Row>
      </div>

      <h2 className="section-title">{t('Ausfüllen')}</h2>
      <div className="setting-list">
        <Row
          label={t('Menü in Anmeldefeldern')}
          description={t('Ein kleiner Knopf in Feldern, die UwULock ausfüllen kann.')}
        >
          <Toggle
            checked={settings.inlineMenu}
            label={t('Menü in Anmeldefeldern')}
            onChange={(v) => void change({ inlineMenu: v })}
          />
        </Row>
        <Row label={t('Anbieten, Logins zu speichern')}>
          <Toggle
            checked={settings.savePrompt}
            label={t('Anbieten, Logins zu speichern')}
            onChange={(v) => void change({ savePrompt: v })}
          />
        </Row>
        <Row label={t('Einmal-Code nach dem Ausfüllen kopieren')}>
          <Toggle
            checked={settings.copyTotp}
            label={t('Einmal-Code nach dem Ausfüllen kopieren')}
            onChange={(v) => void change({ copyTotp: v })}
          />
        </Row>
        <Row
          label={t('Passkeys in UwULock speichern')}
          description={t('Aus: Der Browser fragt wie sonst.')}
        >
          <Toggle
            checked={settings.passkeys}
            label={t('Passkeys in UwULock speichern')}
            onChange={(v) => void change({ passkeys: v })}
          />
        </Row>
        <Row label={t('Standard-Erkennung von Adressen')}>
          <select
            className="select"
            value={settings.defaultMatch}
            onChange={(e) => void change({ defaultMatch: Number(e.target.value) })}
          >
            {MATCHES.map((option) => (
              <option key={option.value} value={option.value}>
                {t(option.label)}
              </option>
            ))}
          </select>
        </Row>
        {settings.neverSave.length > 0 && (
          <Row label={t('Nie speichern für')} description={settings.neverSave.join(', ')}>
            <button type="button" onClick={() => void change({ neverSave: [] })}>
              {t('Zurücksetzen')}
            </button>
          </Row>
        )}
        <p className="field-hint">
          {t('Tastenkürzel: Strg+Umschalt+L füllt den Login dieser Seite aus.')}
        </p>
      </div>

      {status.uwu && (
        <>
          <h2 className="section-title">UwULock Server</h2>
          <div className="setting-list">
            {(uwuFeature(status, 'own-icons') || status.uwu.icons?.automatic) && (
              <Row
                label={t('Icons in der Liste')}
                description={t('Eigene Icons deiner Einträge und die Icons, die dein Server lädt')}
              >
                <Toggle
                  checked={settings.showIcons}
                  label={t('Icons in der Liste')}
                  onChange={(checked) => void change({ showIcons: checked })}
                />
              </Row>
            )}
            {uwuFeature(status, 'file-requests') && (
              <Row
                label={t('Dateianfragen')}
                description={t('Deine Links, über die dir jemand Dateien schickt')}
              >
                <button type="button" className="quiet" onClick={onFileRequests}>
                  {t('Anzeigen')}
                </button>
              </Row>
            )}
          </div>
        </>
      )}

      <h2 className="section-title">{t('Aussehen')}</h2>
      <div className="setting-list">
        <Row label={t('Sprache')}>
          <select
            className="select"
            value={settings.language}
            onChange={(e) => void change({ language: e.target.value as Settings['language'] })}
          >
            <option value="system">{t('Wie der Browser')}</option>
            <option value="de">Deutsch</option>
            <option value="en">English</option>
          </select>
        </Row>
        <Row label={t('Design')}>
          <select
            className="select"
            value={settings.theme}
            onChange={(e) => void change({ theme: e.target.value as Settings['theme'] })}
          >
            <option value="system">{t('Wie das System')}</option>
            <option value="light">{t('Hell')}</option>
            <option value="dark">{t('Dunkel')}</option>
          </select>
        </Row>
      </div>

      <p className="about-line muted">
        UwULock {version} ·{' '}
        <a href="https://github.com/MinifyX/UwULock-Client" target="_blank" rel="noreferrer">
          {t('Quellcode')}
        </a>{' '}
        · GPL-3.0
      </p>
    </div>
  );
}

function PinForm({ onDone }: { onDone: () => void }) {
  const [pin, setPinValue] = useState('');
  const [afterRestart, setAfterRestart] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Kept on disk across restarts, a short PIN is guessed from a copy of the profile in minutes.
  const min = afterRestart ? 6 : 4;
  return (
    <form
      className="form pin-form"
      onSubmit={async (event) => {
        event.preventDefault();
        try {
          await setPin(pin, afterRestart);
          toast(t('PIN eingerichtet ✧'));
          onDone();
        } catch (e) {
          setError(errorText(e));
        }
      }}
    >
      <PasswordInput
        value={pin}
        onChange={setPinValue}
        autoFocus
        autoComplete="off"
        label={t('PIN')}
        placeholder={afterRestart ? t('Mindestens sechs Zeichen') : t('Mindestens vier Zeichen')}
      />
      <label className="check">
        <input
          type="checkbox"
          checked={afterRestart}
          onChange={(e) => setAfterRestart(e.target.checked)}
        />
        <span>{t('Auch nach einem Neustart des Browsers (sonst dann das Master-Passwort)')}</span>
      </label>
      {afterRestart && (
        <p className="notice" data-tone="error" role="note">
          {t(
            'Dann liegt dein Tresorschlüssel, nur mit der PIN verschlüsselt, auf der Festplatte. Wer eine Kopie deines Browser-Profils hat, kann eine kurze PIN in wenigen Minuten erraten. Nimm mindestens sechs Zeichen, besser Wörter oder Buchstaben mit Ziffern.',
          )}
        </p>
      )}
      {error && <p className="form-error">{error}</p>}
      <div className="form-actions">
        <span className="spacer" />
        <button className="primary" type="submit" disabled={[...pin].length < min}>
          {t('Speichern')}
        </button>
      </div>
    </form>
  );
}
