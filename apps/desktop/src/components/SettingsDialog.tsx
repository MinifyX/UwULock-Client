import {
  Button,
  Nyu as SuiteNyu,
  Segmented,
  Select,
  SettingRow,
  Switch,
  Wordmark,
} from '@uwusuite/design';
import { useEffect, useState, type ReactNode } from 'react';
import pkg from '../../package.json';
import {
  checkForUpdates,
  distribution,
  failure,
  lock,
  logout,
  openProjectPage,
  openWebVault,
  setHello,
  syncNow,
  type ProjectPage,
  type Status,
  type UpdateInfo,
} from '../lib/api';
import { errorText, toastError } from '../lib/errors';
import { ago } from '../lib/format';
import { emailOptIn, setEmailOptIn, type EmailOptIn } from '../lib/health';
import { N_, t, useLanguage } from '../lib/i18n';
import { isMobile, systemName } from '../lib/platform';
import { keys } from '../lib/shortcuts';
import { updateSettings, useSettings, type AutoLock, type ClipboardClear } from '../lib/settings';
import { unlockDescription, unlockLabel, unlockPrompt } from '../lib/unlock';
import { FontPicker } from './FontPicker';
import { Modal } from './Modal';
import { MoveSetting } from './MoveDialog';
import { PasskeySettings } from './PasskeySettings';

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

/** One setting: a label, an optional explanation and its control (the package's SettingRow). */
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
    <SettingRow label={label} description={description}>
      {children}
    </SettingRow>
  );
}

/** An on/off setting: the package's Switch, named by its row. */
function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return <Switch label={label} checked={checked} onChange={onChange} />;
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
        label={t('Kontrast')}
        description={t('„System“ folgt der Einstellung von {system}.', { system: systemName() })}
      >
        <Segmented
          label={t('Kontrast')}
          value={settings.contrast}
          onChange={(contrast) => updateSettings({ contrast })}
          options={[
            { value: 'system', label: t('System') },
            { value: 'normal', label: t('Normal') },
            { value: 'high', label: t('Hoch') },
          ]}
        />
      </Row>
      <div className="flex flex-col gap-3 border-b border-hairline py-3.5">
        <div className="flex flex-col gap-0.5">
          <p className="text-body font-semibold">{t('Schrift')}</p>
          <p className="text-caption text-muted">
            {t('Nur auf diesem Gerät. UwU Sans ist die Schrift aller UwU-Apps.')}
          </p>
        </div>
        <FontPicker
          label={t('Schrift')}
          value={settings.font}
          onChange={(font) => updateSettings({ font })}
          systemName={t('System')}
          sample={t('Tresor 0123 Il1 O0')}
        />
      </div>
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
      <Row
        label={t('Website-Symbole')}
        description={t(
          'Nur mit UwULock Server: Er holt die Symbole der Websites und erfährt dabei, welche Seiten in deinem Tresor sind. Eigene Symbole bleiben verschlüsselt und erscheinen immer.',
        )}
      >
        <Toggle
          label={t('Website-Symbole')}
          checked={settings.siteIcons}
          onChange={(siteIcons) => updateSettings({ siteIcons })}
        />
      </Row>
    </>
  );
}

function Security({ status, onClose }: { status: Status; onClose: () => void }) {
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
        <Select
          className="w-52"
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
        </Select>
      </Row>
      <Row
        label={t('Zwischenablage leeren')}
        description={t(
          'Kopierte Werte verschwinden danach wieder – aber nur, wenn inzwischen nichts anderes kopiert wurde. Unter Windows landen sie nie im Zwischenablage-Verlauf.',
        )}
      >
        <Select
          className="w-52"
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
        </Select>
      </Row>
      {status.hello !== null && (
        <Row
          label={unlockLabel(status.helloKind)}
          description={unlockDescription(status.helloKind)}
        >
          <Toggle
            label={unlockLabel(status.helloKind)}
            checked={status.hello}
            onChange={(enabled) =>
              void setHello(enabled, unlockPrompt()).catch((e) => {
                if (failure(e).kind !== 'biometric-cancelled') toastError(e);
              })
            }
          />
        </Row>
      )}
      {isMobile() ? (
        <Row
          label={t('Im Hintergrund sperren')}
          description={t('Sperrt UwULock, wenn es eine Minute oder länger im Hintergrund war.')}
        >
          <Toggle
            label={t('Im Hintergrund sperren')}
            checked={settings.lockWithSystem}
            onChange={(lockWithSystem) => updateSettings({ lockWithSystem })}
          />
        </Row>
      ) : (
        <Row
          label={t('Mit dem Computer sperren')}
          description={t(
            'Sperrt UwULock, sobald der Bildschirm gesperrt wird oder der Computer in den Ruhezustand geht.',
          )}
        >
          <Toggle
            label={t('Mit dem Computer sperren')}
            checked={settings.lockWithSystem}
            onChange={(lockWithSystem) => updateSettings({ lockWithSystem })}
          />
        </Row>
      )}
      <PasskeySettings Row={Row} Toggle={Toggle} />
      <Row
        label={t('Jetzt sperren')}
        description={
          isMobile()
            ? undefined
            : t('Auch mit {keys}, von überall in UwULock.', { keys: keys('CmdOrCtrl+L') })
        }
      >
        <Button
          size="sm"
          onClick={() => {
            onClose();
            void lock();
          }}
        >
          {t('Sperren')}
        </Button>
      </Row>
    </>
  );
}

/**
 * The check of addresses at XposedOrNot (UwULock-Server's docs/uwu-api.md
 * §15.4): only when the admin offers it, and only with this consent — the
 * server sends the addresses there in plain text, and this says so.
 */
function EmailBreachSetting() {
  useLanguage();
  const [optIn, setOptIn] = useState<EmailOptIn | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let current = true;
    emailOptIn().then(
      (answer) => current && setOptIn(answer),
      () => undefined,
    );
    return () => {
      current = false;
    };
  }, []);
  if (!optIn) return null;
  const change = async (on: boolean) => {
    setBusy(true);
    setError(null);
    try {
      setOptIn(await setEmailOptIn(on));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <Row
        label={t('Adressen in Datenlecks prüfen')}
        description={t(
          'Die Passwortprüfung fragt dann auch, ob deine Kontoadresse und die Adressen, die in Logins als Benutzername stehen, in Datenlecks auftauchen. Dafür schickt dein Server jede dieser Adressen im Klartext an XposedOrNot (xposedornot.com) – deine Passwörter und deine anderen Daten nicht. Die Antworten merkt er sich eine Woche lang, nur unter einem Hash der Adresse.',
        )}
      >
        <Toggle
          label={t('Adressen in Datenlecks prüfen')}
          checked={optIn.optedIn}
          onChange={(on) => !busy && void change(on)}
        />
      </Row>
      {error && (
        <p className="setting-result" data-tone="error" role="alert">
          {error}
        </p>
      )}
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
        description={
          status.live
            ? t('Zuletzt {when} – Änderungen von deinen anderen Geräten kommen sofort an.', {
                when: ago(status.lastSync),
              })
            : t(
                'Zuletzt {when} – UwULock holt Änderungen beim Entsperren und danach alle fünf Minuten.',
                { when: ago(status.lastSync) },
              )
        }
      >
        <Button
          size="sm"
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
        </Button>
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
        <Button size="sm" onClick={() => void openWebVault().catch(() => undefined)}>
          {t('Öffnen')}
        </Button>
      </Row>
      {status.state === 'unlocked' && <EmailBreachSetting />}
      {status.state === 'unlocked' && <MoveSetting />}
      <Row
        label={t('Abmelden')}
        description={t(
          'Löscht die Anmeldung und die verschlüsselte Kopie des Tresors von diesem Gerät. Auf dem Server bleibt alles, wie es ist.',
        )}
      >
        <Button size="sm" variant="danger" onClick={() => setConfirm(true)}>
          {t('Abmelden …')}
        </Button>
      </Row>
      {confirm && (
        <Modal
          title={t('Von diesem Gerät abmelden?')}
          size="small"
          onCancel={() => setConfirm(false)}
          footer={
            <>
              <Button
                variant="danger"
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
              </Button>
              <Button variant="primary" data-autofocus onClick={() => setConfirm(false)}>
                {t('Abbrechen')}
              </Button>
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
  const [store, setStore] = useState(false);
  useEffect(() => {
    if (isMobile()) return;
    void distribution()
      .then((from) => setStore(from === 'store'))
      .catch(() => undefined);
  }, []);

  // A phone has no updater: a new APK or IPA comes from the release page.
  if (isMobile())
    return (
      <Row
        label={t('Neue Versionen')}
        description={t(
          'Auf dem Handy aktualisiert sich UwULock nicht selbst. Neue Versionen (APK für Android, IPA für iOS) gibt es auf der Release-Seite.',
        )}
      >
        <Button size="sm" onClick={() => void openProjectPage('releases').catch(() => undefined)}>
          {t('Versionen')}
        </Button>
      </Row>
    );

  // The Mac App Store build has none either: the store updates it.
  if (store)
    return (
      <Row
        label={t('Neue Versionen')}
        description={t(
          'Diese Ausgabe kommt aus dem App Store und bekommt neue Versionen von dort.',
        )}
      >
        <span className="text-caption text-muted">
          {t('Version {version}', { version: pkg.version })}
        </span>
      </Row>
    );

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
          <Button size="sm" variant="primary" onClick={onInstallUpdate}>
            {t('{version} installieren', { version: update.version })}
          </Button>
        ) : (
          <Button
            size="sm"
            busy={checking}
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
          </Button>
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
      <SuiteNyu shell="lock" size={88} mood="happy" title="Nyu" />
      <Wordmark product="Lock" className="mt-1.5 text-title" />
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
        <Button size="sm" onClick={() => open('source')}>
          {t('Quellcode auf GitHub')}
        </Button>
        <Button size="sm" onClick={() => open('releases')}>
          {t('Versionen')}
        </Button>
        <Button size="sm" onClick={() => open('license')}>
          {t('Lizenz')}
        </Button>
        <Button size="sm" onClick={() => open('suite')}>
          UwUSuite
        </Button>
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
          {section === 'security' && loggedIn && <Security status={status} onClose={onClose} />}
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
    </Modal>
  );
}
