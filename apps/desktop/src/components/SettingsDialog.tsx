import { useState, type ReactNode } from 'react';
import pkg from '../../package.json';
import {
  checkForUpdates,
  lock,
  logout,
  openProjectPage,
  openWebVault,
  syncNow,
  type ProjectPage,
  type Status,
  type UpdateInfo,
} from '../lib/api';
import { errorText } from '../lib/errors';
import { ago } from '../lib/format';
import { N_, t, useLanguage } from '../lib/i18n';
import { systemName } from '../lib/platform';
import { updateSettings, useSettings, type AutoLock, type ClipboardClear } from '../lib/settings';
import { Modal } from './Modal';
import { Nyu } from './nyu/Nyu';

export type SettingsSection = 'appearance' | 'security' | 'account' | 'updates' | 'about';

const SECTIONS: { id: SettingsSection; label: string }[] = [
  { id: 'appearance', label: N_('Darstellung') },
  { id: 'security', label: N_('Sicherheit') },
  { id: 'account', label: N_('Konto') },
  { id: 'updates', label: N_('Updates') },
  { id: 'about', label: N_('Über UwULock') },
];

type Props = {
  initial?: SettingsSection;
  status: Status;
  onClose: () => void;
  /** A downloaded update, if one is waiting. */
  update: UpdateInfo | null;
  onUpdateFound: (update: UpdateInfo) => void;
  onInstallUpdate: () => void;
};

/** One setting: a label, an optional explanation and its control. */
function Row({
  label,
  description,
  children,
}: {
  label: string;
  description?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="setting-row">
      <div className="setting-text">
        <p className="setting-label">{label}</p>
        {description && <p className="setting-description">{description}</p>}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}

function Segmented<T extends string | number>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="segmented" role="radiogroup" aria-label={label}>
      {options.map((option) => (
        <button
          key={String(option.value)}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      className="toggle"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
    >
      <span className="toggle-thumb" />
    </button>
  );
}

function Appearance() {
  const settings = useSettings();
  return (
    <>
      <Row
        label="Sprache · Language"
        description={`„System“ folgt der Sprache von ${systemName()}. · “System” follows ${systemName()}.`}
      >
        <Segmented
          label="Sprache · Language"
          value={settings.language}
          onChange={(language) => updateSettings({ language })}
          options={[
            { value: 'system', label: 'System' },
            { value: 'de', label: 'Deutsch' },
            { value: 'en', label: 'English' },
          ]}
        />
      </Row>
      <Row label={t('Farbschema')}>
        <Segmented
          label={t('Farbschema')}
          value={settings.theme}
          onChange={(theme) => updateSettings({ theme })}
          options={[
            { value: 'system', label: t('System') },
            { value: 'light', label: t('Hell') },
            { value: 'dark', label: t('Dunkel') },
          ]}
        />
      </Row>
      <Row
        label={t('Animationen')}
        description={t('„System“ folgt der Einstellung von {system}.', { system: systemName() })}
      >
        <Segmented
          label={t('Animationen')}
          value={settings.motion}
          onChange={(motion) => updateSettings({ motion })}
          options={[
            { value: 'system', label: t('System') },
            { value: 'on', label: t('An') },
            { value: 'off', label: t('Aus') },
          ]}
        />
      </Row>
      <Row
        label={t('Papierkorb zeigen')}
        description={t('Gelöschte Einträge in einem eigenen Bereich der Seitenleiste.')}
      >
        <Toggle
          label={t('Papierkorb zeigen')}
          checked={settings.showTrash}
          onChange={(showTrash) => updateSettings({ showTrash })}
        />
      </Row>
    </>
  );
}

function Security({ onClose }: { onClose: () => void }) {
  const settings = useSettings();
  const minutes = (n: number) => (n === 1 ? t('1 Minute') : t('{n} Minuten', { n }));
  return (
    <>
      <Row
        label={t('Automatisch sperren')}
        description={t(
          'Nach so langer Zeit ohne Eingabe sperrt UwULock den Tresor und vergisst alles Entschlüsselte. Beim Beenden ist er immer gesperrt.',
        )}
      >
        <select
          className="select"
          value={settings.autoLock}
          aria-label={t('Automatisch sperren')}
          onChange={(e) => updateSettings({ autoLock: Number(e.target.value) as AutoLock })}
        >
          {([1, 5, 15, 30, 60, 240] as const).map((n) => (
            <option key={n} value={n}>
              {n < 60 ? minutes(n) : n === 60 ? t('1 Stunde') : t('{n} Stunden', { n: n / 60 })}
            </option>
          ))}
          <option value={0}>{t('Nie (nur beim Beenden)')}</option>
        </select>
      </Row>
      <Row
        label={t('Zwischenablage leeren')}
        description={t(
          'Kopierte Werte verschwinden danach wieder – aber nur, wenn inzwischen nichts anderes kopiert wurde. Unter Windows landen sie nie im Zwischenablage-Verlauf.',
        )}
      >
        <select
          className="select"
          value={settings.clipboardClear}
          aria-label={t('Zwischenablage leeren')}
          onChange={(e) =>
            updateSettings({ clipboardClear: Number(e.target.value) as ClipboardClear })
          }
        >
          {([10, 30, 60, 120] as const).map((n) => (
            <option key={n} value={n}>
              {t('nach {n} Sekunden', { n })}
            </option>
          ))}
          <option value={0}>{t('Nie')}</option>
        </select>
      </Row>
      <Row label={t('Jetzt sperren')} description={t('Auch mit Strg+L, von überall in UwULock.')}>
        <button
          onClick={() => {
            onClose();
            void lock();
          }}
        >
          {t('Sperren')}
        </button>
      </Row>
    </>
  );
}

function Account({ status, onClose }: { status: Status; onClose: () => void }) {
  useLanguage();
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ tone: 'info' | 'error'; text: string } | null>(null);
  const [confirm, setConfirm] = useState(false);
  return (
    <>
      <Row label={t('Angemeldet als')} description={status.server ?? undefined}>
        <span className="setting-value">{status.email}</span>
      </Row>
      <Row
        label={t('Synchronisieren')}
        description={t(
          'Zuletzt {when} – UwULock holt Änderungen beim Entsperren und danach alle fünf Minuten.',
          { when: ago(status.lastSync) },
        )}
      >
        <button
          disabled={busy || status.syncing}
          onClick={async () => {
            setBusy(true);
            setResult(null);
            try {
              await syncNow();
              setResult({ tone: 'info', text: t('Synchronisiert ✧') });
            } catch (e) {
              setResult({ tone: 'error', text: errorText(e) });
            } finally {
              setBusy(false);
            }
          }}
        >
          {busy || status.syncing ? t('Synchronisiert …') : t('Jetzt synchronisieren')}
        </button>
      </Row>
      {result && (
        <p className="setting-result" data-tone={result.tone} role="status">
          {result.text}
        </p>
      )}
      <Row
        label={t('Web-Tresor')}
        description={t(
          'Für alles, was diese Beta noch nicht kann: Anhänge, Sends, Organisationen verwalten.',
        )}
      >
        <button onClick={() => void openWebVault().catch(() => undefined)}>{t('Öffnen')}</button>
      </Row>
      <Row
        label={t('Abmelden')}
        description={t(
          'Löscht die Anmeldung und die verschlüsselte Kopie des Tresors von diesem Gerät. Auf dem Server bleibt alles, wie es ist.',
        )}
      >
        <button className="danger" onClick={() => setConfirm(true)}>
          {t('Abmelden …')}
        </button>
      </Row>
      {confirm && (
        <Modal
          title={t('Von diesem Gerät abmelden?')}
          onCancel={() => setConfirm(false)}
          footer={
            <>
              <span className="spacer" />
              <button
                className="danger"
                data-secondary
                onClick={async () => {
                  setConfirm(false);
                  try {
                    await logout();
                    onClose();
                  } catch (e) {
                    setResult({ tone: 'error', text: errorText(e) });
                  }
                }}
              >
                {t('Abmelden')}
              </button>
              <button className="primary" data-autofocus onClick={() => setConfirm(false)}>
                {t('Abbrechen')}
              </button>
            </>
          }
        >
          <p className="dialog-lead">
            {t(
              'Die Anmeldung und die verschlüsselte Kopie des Tresors werden von diesem Gerät gelöscht. Dein Tresor auf dem Server bleibt, wie er ist.',
            )}
          </p>
        </Modal>
      )}
    </>
  );
}

function Updates({
  update,
  onUpdateFound,
  onInstallUpdate,
}: Pick<Props, 'update' | 'onUpdateFound' | 'onInstallUpdate'>) {
  const settings = useSettings();
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<{ tone: 'info' | 'error'; text: string } | null>(null);

  return (
    <>
      <Row
        label={t('Update-Kanal')}
        description={
          settings.updateChannel === 'beta'
            ? t('Beta bekommt neue Versionen früher. Es kann mal etwas wackeln.')
            : t('Stabil bekommt nur fertige Versionen.')
        }
      >
        <Segmented
          label={t('Update-Kanal')}
          value={settings.updateChannel}
          onChange={(updateChannel) => {
            setResult(null);
            updateSettings({ updateChannel });
          }}
          options={[
            { value: 'stable', label: t('Stabil') },
            { value: 'beta', label: t('Beta') },
          ]}
        />
      </Row>
      <Row
        label={t('Version {version}', { version: pkg.version })}
        description={t(
          'UwULock lädt neue Versionen still herunter und installiert sie beim nächsten Start. Jedes Update ist signiert und wird vor dem Start geprüft.',
        )}
      >
        {update ? (
          <button className="primary" onClick={onInstallUpdate}>
            {t('{version} installieren', { version: update.version })}
          </button>
        ) : (
          <button
            disabled={checking}
            onClick={async () => {
              setChecking(true);
              setResult(null);
              try {
                const found = await checkForUpdates();
                if (found) onUpdateFound(found);
                else setResult({ tone: 'info', text: t('UwULock ist auf dem neuesten Stand. ✧') });
              } catch (e) {
                setResult({
                  tone: 'error',
                  text: t('Suche fehlgeschlagen: {error}', { error: String(e) }),
                });
              } finally {
                setChecking(false);
              }
            }}
          >
            {checking ? t('Sucht …') : t('Nach Updates suchen')}
          </button>
        )}
      </Row>
      {result && (
        <p className="setting-result" data-tone={result.tone} role="status">
          {result.text}
        </p>
      )}
    </>
  );
}

function About() {
  useLanguage();
  const open = (page: ProjectPage) => void openProjectPage(page).catch(() => undefined);
  return (
    <div className="about">
      <Nyu size={88} mood="happy" title="Nyu" />
      <p className="about-name">
        <span>UwU</span>Lock
      </p>
      <p className="about-version">{t('Version {version}', { version: pkg.version })}</p>
      <p className="about-text">
        {t(
          'Freie Software unter der GNU GPL v3.0. Nutzen, ändern, weitergeben – nur geänderte Versionen müssen offen bleiben. Kein Tracking, kein Konto.',
        )}
      </p>
      <p className="about-text">
        {t(
          'Spricht das Protokoll von Bitwarden und Vaultwarden, mit derselben Verschlüsselung wie die offiziellen Apps. Nicht verbunden mit Bitwarden Inc.',
        )}
      </p>
      <div className="about-actions">
        <button onClick={() => open('source')}>{t('Quellcode auf GitHub')}</button>
        <button onClick={() => open('releases')}>{t('Versionen')}</button>
        <button onClick={() => open('license')}>{t('Lizenz')}</button>
        <button onClick={() => open('suite')}>UwUSuite</button>
      </div>
    </div>
  );
}

export function SettingsDialog({
  initial = 'appearance',
  status,
  onClose,
  update,
  onUpdateFound,
  onInstallUpdate,
}: Props) {
  useLanguage();
  const [section, setSection] = useState<SettingsSection>(initial);
  const loggedIn = status.state !== 'logged-out';
  const sections = SECTIONS.filter((s) => loggedIn || (s.id !== 'account' && s.id !== 'security'));
  return (
    <Modal title={t('Einstellungen')} size="wide" onCancel={onClose}>
      <div className="settings">
        <nav className="settings-nav" aria-label={t('Bereiche')}>
          {sections.map(({ id, label }) => (
            <button
              key={id}
              type="button"
              aria-current={section === id ? 'page' : undefined}
              onClick={() => setSection(id)}
            >
              {t(label)}
            </button>
          ))}
        </nav>
        <div className="settings-content">
          {section === 'appearance' && <Appearance />}
          {section === 'security' && loggedIn && <Security onClose={onClose} />}
          {section === 'account' && loggedIn && <Account status={status} onClose={onClose} />}
          {section === 'updates' && (
            <Updates
              update={update}
              onUpdateFound={onUpdateFound}
              onInstallUpdate={onInstallUpdate}
            />
          )}
          {section === 'about' && <About />}
        </div>
      </div>
      <button className="settings-close icon-button" onClick={onClose} aria-label={t('Schließen')}>
        ×
      </button>
    </Modal>
  );
}
