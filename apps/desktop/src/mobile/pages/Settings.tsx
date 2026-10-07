/**
 * Settings on a phone and an iPad: a list of pages (Darstellung, Sicherheit,
 * AutoFill where the system has a provider, Konto, Über UwULock; Android also
 * "Neue Versionen") instead of the desktop's dialog with sections. The same
 * settings and commands as SettingsDialog.tsx; only what a phone has.
 */

import {
  FONT_CHOICES,
  FONT_NAMES,
  ICONS,
  ListRow,
  ListSection,
  Nyu as SuiteNyu,
  setHapticsEnabled,
  type FontChoice,
} from '@uwusuite/design';
import { useEffect, useState } from 'react';
import pkg from '../../../package.json';
import {
  failure,
  lock,
  logout,
  openProjectPage,
  openWebVault,
  renameAccount,
  setHello,
  type ProjectPage,
} from '../../lib/api';
import { errorText, toastError } from '../../lib/errors';
import { ago } from '../../lib/format';
import {
  AUTOFILL_CHANGED,
  autofillProviderRequest,
  hasCredentialManager,
  providerSettingsPath,
  providerStateText,
  useProviderStatus,
} from '../../lib/autofill';
import { emailOptIn, setEmailOptIn, type EmailOptIn } from '../../lib/health';
import { t, useLanguage } from '../../lib/i18n';
import {
  onPasskeyProviderWarning,
  passkeyProviderStatus,
  setPasskeyProvider,
  warningText,
  type PasskeyProviderStatus,
} from '../../lib/passkeys';
import {
  updateSettings,
  useSettings,
  type AutoLock,
  type ClipboardClear,
  type ContrastSetting,
  type LanguageSetting,
  type MotionSetting,
  type TextSizeChoice,
} from '../../lib/settings';
import { textSizeOptions } from '../../lib/appearance';
import { useSafariStatus } from '../../lib/safari';
import { unlockDescription, unlockLabel, unlockPrompt } from '../../lib/unlock';
import { initialOf } from '../../components/AccountCard';
import { MoveDialog } from '../../components/MoveDialog';
import { SettingsImportPage } from './Import';
import type { SettingsPage as Section } from '../nav';
import { useMobile, useNav } from '../state';
import {
  ChoiceSheet,
  EditSurface,
  Empty,
  FieldInput,
  Page,
  Segmented,
  Toggle,
  useConfirm,
} from '../ui';

// ── Haptics ─────────────────────────────────────────────────────────────────

/**
 * "Haptisches Feedback" is a convenience of this device only, so it lives in
 * the page's storage next to the settings and is applied as soon as the app
 * loads (this module is part of the first bundle).
 */
const HAPTICS_KEY = 'uwulock.haptics';

function readHaptics(): boolean {
  try {
    return window.localStorage.getItem(HAPTICS_KEY) !== 'off';
  } catch {
    return true;
  }
}

setHapticsEnabled(readHaptics());

function useHaptics(): [boolean, (on: boolean) => void] {
  const [on, setOn] = useState(readHaptics);
  return [
    on,
    (next) => {
      setOn(next);
      setHapticsEnabled(next);
      try {
        window.localStorage.setItem(HAPTICS_KEY, next ? 'on' : 'off');
      } catch {
        // Holds for this run.
      }
    },
  ];
}

// ── Shared bits ─────────────────────────────────────────────────────────────

/** Tells the other pages (the iPad shows the list beside the page) that the switch changed. */
const PASSKEYS_CHANGED = 'uwulock-passkeys-changed';

/** The system's passkey provider, as the desktop's PasskeySettings reads it. */
function usePasskeyStatus(): PasskeyProviderStatus | null {
  const [status, setStatus] = useState<PasskeyProviderStatus | null>(null);
  useEffect(() => {
    const load = () =>
      void passkeyProviderStatus()
        .then(setStatus)
        .catch(() => setStatus(null));
    load();
    const stop = onPasskeyProviderWarning(load);
    window.addEventListener(PASSKEYS_CHANGED, load);
    return () => {
      window.removeEventListener(PASSKEYS_CHANGED, load);
      void stop.then((unlisten) => unlisten());
    };
  }, []);
  return status;
}

/** Only iOS (the AutoFill extension) and Android (Credential Manager) have one on a phone. */
const hasPasskeyPage = (status: PasskeyProviderStatus | null) =>
  status?.platform === 'apple' || status?.platform === 'android';

/** "Face ID", "Fingerabdruck", … — the short name for the row's value. */
function biometricName(kind: string | null): string {
  switch (kind) {
    case 'faceId':
      return 'Face ID';
    case 'touchId':
      return 'Touch ID';
    case 'opticId':
      return 'Optic ID';
    case 'face':
      return t('Gesicht');
    case 'fingerprint':
      return t('Fingerabdruck');
    default:
      return t('An');
  }
}

function autoLockLabel(n: AutoLock): string {
  if (n === 0) return t('Nie (nur beim Beenden)');
  if (n === 1) return t('1 Minute');
  if (n < 60) return t('{n} Minuten', { n });
  if (n === 60) return t('1 Stunde');
  return t('{n} Stunden', { n: n / 60 });
}

const clipboardLabel = (n: ClipboardClear) => (n ? t('nach {n} Sekunden', { n }) : t('Nie'));

const triLabel = (value: 'system' | string, on: string, off: string) =>
  value === 'system' ? t('System') : value === 'high' || value === 'on' ? on : off;

const fontLabel = (font: FontChoice) =>
  font === 'system' ? t('System') : FONT_NAMES[font as Exclude<FontChoice, 'system'>];

const LANGUAGES: { value: LanguageSetting; label: string }[] = [
  { value: 'system', label: 'System' },
  { value: 'de', label: 'Deutsch' },
  { value: 'en', label: 'English' },
];

// ── The list ────────────────────────────────────────────────────────────────

export function SettingsPage() {
  useLanguage();
  const { status, openSheet, android } = useMobile();
  const nav = useNav();
  const passkeys = usePasskeyStatus();
  const confirm = useConfirm();
  const selected = (section: Section) =>
    nav.column !== 'phone' &&
    nav.selected?.page === 'settings-page' &&
    nav.selected.section === section;
  const open = (section: Section) => nav.open({ page: 'settings-page', section });
  const name = status.name || status.label || status.email || '';

  return (
    <Page title={t('Einstellungen')} largeTitle>
      <ListSection>
        <button
          type="button"
          className="m-account"
          aria-label={t('Konto wechseln')}
          onClick={() => openSheet({ kind: 'accounts' })}
        >
          <span className="m-avatar" aria-hidden>
            {initialOf({
              name: status.name,
              label: status.label ?? '',
              email: status.email ?? '',
            })}
          </span>
          <span className="m-account-text">
            <b>{name}</b>
            {status.email && status.email !== name && <span>{status.email}</span>}
            {status.server && <span>{status.server}</span>}
          </span>
          <ICONS.next className="uwu-row-chevron" aria-hidden />
        </button>
      </ListSection>

      <ListSection>
        <ListRow
          icon={ICONS.appearance}
          iconTone="solid"
          title={t('Darstellung')}
          selected={selected('appearance')}
          onClick={() => open('appearance')}
        />
        <ListRow
          icon={ICONS.locked}
          iconTone="solid"
          title={t('Sicherheit')}
          value={
            status.hello === null
              ? undefined
              : status.hello
                ? biometricName(status.helloKind)
                : t('Aus')
          }
          selected={selected('security')}
          onClick={() => open('security')}
        />
        {hasPasskeyPage(passkeys) && (
          <ListRow
            icon={ICONS.passkey}
            iconTone="solid"
            title={t('AutoFill')}
            value={
              passkeys?.platform === 'apple'
                ? passkeys.settings.appleExtension
                  ? t('An')
                  : t('Aus')
                : undefined
            }
            selected={selected('autofill')}
            onClick={() => open('autofill')}
          />
        )}
        <ListRow
          icon={ICONS.profile}
          iconTone="solid"
          title={t('Konto')}
          selected={selected('account')}
          onClick={() => open('account')}
        />
      </ListSection>

      {status.state === 'unlocked' && (
        <ListSection>
          <ListRow
            icon={ICONS.import}
            iconTone="solid"
            title={t('Importieren')}
            selected={selected('import')}
            onClick={() => open('import')}
          />
        </ListSection>
      )}

      <ListSection>
        {android && (
          <ListRow
            icon={ICONS.download}
            iconTone="neutral"
            title={t('Neue Versionen')}
            selected={selected('updates')}
            onClick={() => open('updates')}
          />
        )}
        <ListRow
          icon={ICONS.info}
          iconTone="neutral"
          title={t('Über UwULock')}
          value={pkg.version}
          selected={selected('about')}
          onClick={() => open('about')}
        />
      </ListSection>

      <ListSection
        // iPhone and iPad: new versions come from the App Store (and TestFlight) —
        // nothing here points anywhere else.
        footer={
          android
            ? t(
                'Auf Android aktualisiert sich UwULock nicht selbst. Neue Versionen (APK) gibt es auf der Release-Seite.',
              )
            : undefined
        }
      >
        <ListRow
          icon={ICONS.locked}
          iconTone="neutral"
          title={status.accounts.length > 1 ? t('Alle sperren') : t('Jetzt sperren')}
          chevron={false}
          onClick={() => void lock().catch(toastError)}
        />
        <ListRow
          icon={ICONS.signOut}
          iconTone="danger"
          title={<span className="m-danger-text">{t('Abmelden')}</span>}
          chevron={false}
          onClick={() =>
            confirm.ask({
              title: t('Von diesem Gerät abmelden?'),
              text: t(
                'Die Anmeldung und die verschlüsselte Kopie des Tresors werden von diesem Gerät gelöscht. Dein Tresor auf dem Server bleibt, wie er ist.',
              ),
              confirm: t('Abmelden'),
              run: () => void logout().catch(toastError),
            })
          }
        />
      </ListSection>
      {confirm.element}
    </Page>
  );
}

// ── The pages ───────────────────────────────────────────────────────────────

export function SettingsSubPage({ section }: { section: Section }) {
  const { android } = useMobile();
  switch (section) {
    case 'appearance':
      return <AppearancePage />;
    case 'security':
      return <SecurityPage />;
    case 'autofill':
      return <PasskeysPage />;
    case 'account':
      return <AccountPage />;
    case 'import':
      return <SettingsImportPage />;
    case 'updates':
      return android ? <UpdatesPage /> : <AboutPage />;
    case 'about':
      return <AboutPage />;
  }
}

function AppearancePage() {
  useLanguage();
  const settings = useSettings();
  const { ios, android } = useMobile();
  const [haptics, setHaptics] = useHaptics();
  const [choice, setChoice] = useState<
    null | 'contrast' | 'font' | 'textSize' | 'motion' | 'language'
  >(null);
  const close = () => setChoice(null);
  const systemNote = !android
    ? t('„System“ folgt der Einstellung von {system}.', { system: 'iOS' })
    : t('„System“ folgt der Einstellung von {system}.', { system: 'Android' });

  return (
    <Page title={t('Darstellung')} largeTitle>
      <div className="uwu-list-header m-settings-head">
        <h2>{t('Design')}</h2>
      </div>
      <Segmented
        label={t('Farbschema')}
        value={settings.theme}
        onChange={(theme) => updateSettings({ theme })}
        options={[
          { value: 'light', label: t('Hell') },
          { value: 'dark', label: t('Dunkel') },
          { value: 'system', label: t('System') },
        ]}
      />

      <ListSection footer={systemNote}>
        <ListRow
          title={t('Kontrast')}
          value={triLabel(settings.contrast, t('Hoch'), t('Normal'))}
          onClick={() => setChoice('contrast')}
        />
        <ListRow
          title={t('Schrift')}
          value={fontLabel(settings.font)}
          onClick={() => setChoice('font')}
        />
        <ListRow
          title={t('Textgröße')}
          value={textSizeOptions().find((option) => option.value === settings.textSize)?.label}
          onClick={() => setChoice('textSize')}
        />
        <ListRow
          title={t('Animationen')}
          value={triLabel(settings.motion, t('An'), t('Aus'))}
          onClick={() => setChoice('motion')}
        />
      </ListSection>

      <ListSection
        footer={t(
          'Website-Icons gibt es nur mit UwULock Server: Er holt sie und erfährt dabei, welche Seiten in deinem Tresor sind. Eigene Icons bleiben verschlüsselt und erscheinen immer.',
        )}
      >
        <ListRow
          title={t('Website-Icons laden')}
          trailing={
            <Toggle
              label={t('Website-Icons laden')}
              checked={settings.siteIcons}
              onChange={(siteIcons) => updateSettings({ siteIcons })}
            />
          }
        />
        <ListRow
          title={t('Papierkorb zeigen')}
          trailing={
            <Toggle
              label={t('Papierkorb zeigen')}
              checked={settings.showTrash}
              onChange={(showTrash) => updateSettings({ showTrash })}
            />
          }
        />
        <ListRow
          title="Sprache · Language"
          value={LANGUAGES.find((l) => l.value === settings.language)?.label}
          onClick={() => setChoice('language')}
        />
      </ListSection>

      {(ios || android) && (
        <ListSection>
          <ListRow
            title={t('Haptisches Feedback')}
            trailing={
              <Toggle label={t('Haptisches Feedback')} checked={haptics} onChange={setHaptics} />
            }
          />
        </ListSection>
      )}

      <ChoiceSheet<ContrastSetting>
        open={choice === 'contrast'}
        onClose={close}
        title={t('Kontrast')}
        value={settings.contrast}
        onChange={(contrast) => updateSettings({ contrast })}
        options={[
          { value: 'system', label: t('System') },
          { value: 'normal', label: t('Normal') },
          { value: 'high', label: t('Hoch') },
        ]}
      />
      <ChoiceSheet<FontChoice>
        open={choice === 'font'}
        onClose={close}
        title={t('Schrift')}
        value={settings.font}
        onChange={(font) => updateSettings({ font })}
        options={FONT_CHOICES.map((font) => ({ value: font, label: fontLabel(font) }))}
        footer={t('Nur auf diesem Gerät. UwU Sans ist die Schrift aller UwU-Apps.')}
      />
      <ChoiceSheet<TextSizeChoice>
        open={choice === 'textSize'}
        onClose={close}
        title={t('Textgröße')}
        value={settings.textSize}
        onChange={(textSize) => updateSettings({ textSize })}
        options={textSizeOptions()}
        footer={t('„System“ folgt der Textgröße von {system}.', {
          system: android ? 'Android' : 'iOS',
        })}
      />
      <ChoiceSheet<MotionSetting>
        open={choice === 'motion'}
        onClose={close}
        title={t('Animationen')}
        value={settings.motion}
        onChange={(motion) => updateSettings({ motion })}
        options={[
          { value: 'system', label: t('System') },
          { value: 'on', label: t('An') },
          { value: 'off', label: t('Aus') },
        ]}
      />
      <ChoiceSheet<LanguageSetting>
        open={choice === 'language'}
        onClose={close}
        title="Sprache · Language"
        value={settings.language}
        onChange={(language) => updateSettings({ language })}
        options={LANGUAGES}
      />
    </Page>
  );
}

function SecurityPage() {
  useLanguage();
  const settings = useSettings();
  const { status } = useMobile();
  const [clipboard, setClipboard] = useState(false);

  return (
    <Page title={t('Sicherheit')} largeTitle>
      {status.hello !== null && (
        <ListSection footer={unlockDescription(status.helloKind)}>
          <ListRow
            icon={ICONS.fingerprint}
            iconTone="success"
            title={unlockLabel(status.helloKind)}
            trailing={
              <Toggle
                label={unlockLabel(status.helloKind)}
                checked={status.hello}
                onChange={(enabled) =>
                  void setHello(enabled, unlockPrompt()).catch((e) => {
                    if (failure(e).kind !== 'biometric-cancelled') toastError(e);
                  })
                }
              />
            }
          />
        </ListSection>
      )}

      <ListSection
        header={t('Automatisch sperren')}
        footer={t(
          'Nach so langer Zeit ohne Eingabe sperrt UwULock den Tresor und vergisst alles Entschlüsselte. Beim Beenden ist er immer gesperrt.',
        )}
      >
        {([1, 5, 15, 30, 60, 240, 0] as const).map((n) => (
          <ListRow
            key={n}
            title={autoLockLabel(n)}
            aria-pressed={settings.autoLock === n}
            chevron={false}
            trailing={
              settings.autoLock === n ? <ICONS.done className="m-check" aria-hidden /> : undefined
            }
            onClick={() => updateSettings({ autoLock: n })}
          />
        ))}
      </ListSection>

      <ListSection
        footer={t('Sperrt UwULock, wenn es eine Minute oder länger im Hintergrund war.')}
      >
        <ListRow
          title={t('Im Hintergrund sperren')}
          trailing={
            <Toggle
              label={t('Im Hintergrund sperren')}
              checked={settings.lockWithSystem}
              onChange={(lockWithSystem) => updateSettings({ lockWithSystem })}
            />
          }
        />
      </ListSection>

      <ListSection
        footer={t(
          'Kopierte Werte verschwinden danach wieder – aber nur, wenn inzwischen nichts anderes kopiert wurde.',
        )}
      >
        <ListRow
          icon={ICONS.copy}
          iconTone="neutral"
          title={t('Zwischenablage leeren')}
          value={clipboardLabel(settings.clipboardClear)}
          onClick={() => setClipboard(true)}
        />
      </ListSection>

      <ChoiceSheet<ClipboardClear>
        open={clipboard}
        onClose={() => setClipboard(false)}
        title={t('Zwischenablage leeren')}
        value={settings.clipboardClear}
        onChange={(clipboardClear) => updateSettings({ clipboardClear })}
        options={([10, 30, 60, 120, 0] as const).map((n) => ({
          value: n,
          label: clipboardLabel(n),
        }))}
      />
    </Page>
  );
}

/**
 * AutoFill in other apps: whether UwULock is the system's provider for passwords and passkeys
 * (with the button that asks the system), and on iOS UwULock's own switch that leaves the
 * extension its sealed list. Android has two providers: Credential Manager and the autofill
 * service.
 */
function PasskeysPage() {
  useLanguage();
  const status = usePasskeyStatus();
  const [provider, reload] = useProviderStatus();
  const [safari] = useSafariStatus();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  if (!status)
    return (
      <Page title={t('AutoFill')}>
        <Empty title={t('Lädt …')} />
      </Page>
    );

  const ask = (target: 'credentials' | 'autofill') => {
    setError(null);
    setBusy(target);
    void autofillProviderRequest(target)
      .then(() => {
        window.dispatchEvent(new Event(AUTOFILL_CHANGED));
        window.dispatchEvent(new Event(PASSKEYS_CHANGED));
        reload();
      })
      .catch((failed) => setError(errorText(failed)))
      .finally(() => setBusy(null));
  };

  /** The pink action row under a provider that isn't on yet. */
  const askRow = (target: 'credentials' | 'autofill', on: boolean | null) =>
    provider?.supported && on !== true ? (
      <ListRow
        tone="accent"
        title={provider.direct ? t('Als Standard festlegen') : t('Einstellungen öffnen')}
        disabled={busy !== null}
        onClick={() => ask(target)}
      />
    ) : null;

  const errors = (extra: (string | null)[] = []) =>
    [...extra, error].filter(Boolean).map((text) => (
      <p key={text} className="m-footnote m-error" role="alert">
        {text}
      </p>
    ));

  if (status.platform === 'android')
    return (
      <Page title={t('AutoFill')} largeTitle>
        <ListSection
          footer={t(
            'Passwörter und Passkeys über Androids Anmeldeverwaltung (ab Android 14). Jedes Ausfüllen fragt nach deiner Displaysperre oder deinem Fingerabdruck.',
          )}
        >
          <ListRow
            icon={ICONS.passkey}
            iconTone={provider?.enabled ? 'success' : 'neutral'}
            title={t('Passwörter und Passkeys')}
            value={provider ? providerStateText(provider) : undefined}
            wrap
          />
          {provider && hasCredentialManager(provider)
            ? askRow('credentials', provider.enabled ?? null)
            : null}
        </ListSection>
        <ListSection
          footer={t(
            'Für Apps und Browser, die Androids Anmeldeverwaltung nicht nutzen. In Chrome zusätzlich: Einstellungen → Autofill-Dienste → „Autofill über einen anderen Dienst“.',
          )}
        >
          <ListRow
            icon={ICONS.website}
            iconTone={provider?.autofill ? 'success' : 'neutral'}
            title={t('Autofill-Dienst')}
            value={
              provider?.autofill == null
                ? undefined
                : provider.autofill
                  ? t('Eingeschaltet')
                  : t('Ausgeschaltet')
            }
            wrap
          />
          {askRow('autofill', provider?.autofill ?? null)}
        </ListSection>
        {errors()}
      </Page>
    );

  if (status.platform !== 'apple') return <Page title={t('AutoFill')}>{null}</Page>;

  const on = status.settings.appleExtension;
  const problem = on && status.problem ? status.problem : null;
  const warning = on && status.warning ? warningText(status.warning) : null;
  const change = (appleExtension: boolean) => {
    setError(null);
    void setPasskeyProvider({ ...status.settings, appleExtension })
      .then(() => {
        window.dispatchEvent(new Event(PASSKEYS_CHANGED));
        window.dispatchEvent(new Event(AUTOFILL_CHANGED));
        reload();
      })
      .catch((failed) => setError(errorText(failed)));
  };

  return (
    <Page title={t('AutoFill')} largeTitle>
      {provider?.supported && (
        <ListSection
          footer={
            provider.enabled === true
              ? t('UwULock schlägt Passwörter und Passkeys beim Anmelden vor.')
              : t('UwULock dort einschalten: {path}', { path: providerSettingsPath(provider) })
          }
        >
          <ListRow
            icon={ICONS.passkey}
            iconTone={provider.enabled ? 'success' : 'neutral'}
            title={t('Standard für AutoFill')}
            value={providerStateText(provider)}
            wrap
          />
          {askRow('credentials', provider.enabled)}
        </ListSection>
      )}
      <ListSection
        footer={t(
          'Hinterlegt Passwörter und Passkeys versiegelt für UwULocks AutoFill-Erweiterung; die öffnet sie nur nach Face ID, Touch ID oder dem Gerätecode. Logins mit erneuter Master-Passwort-Abfrage bleiben draußen.',
        )}
      >
        <ListRow
          icon={ICONS.locked}
          iconTone={on ? 'success' : 'neutral'}
          title={t('Passwörter und Passkeys für AutoFill')}
          wrap
          trailing={
            <Toggle
              label={t('Passwörter und Passkeys für AutoFill')}
              checked={on}
              onChange={change}
            />
          }
        />
      </ListSection>
      {safari?.platform === 'ios' && safari.available && (
        <ListSection
          footer={t(
            'Dieselbe Erweiterung wie in Chrome und Firefox, mit eigener Anmeldung. Einschalten unter Einstellungen → Apps → Safari → Erweiterungen → UwULock. Passkeys und Passwörter kommen in Safari aus dieser App.',
          )}
        >
          <ListRow icon={ICONS.website} title={t('In Safari aktivieren')} wrap />
        </ListSection>
      )}
      {errors([warning, problem])}
    </Page>
  );
}

function AccountPage() {
  useLanguage();
  const { status, sync } = useMobile();
  const [renaming, setRenaming] = useState(false);
  const [moving, setMoving] = useState(false);
  const unlocked = status.state === 'unlocked';
  const current = status.accounts.find((account) => account.active) ?? null;

  return (
    <Page title={t('Konto')} largeTitle>
      <ListSection>
        {status.email && <ListRow label={t('E-Mail')} title={status.email} />}
        {status.server && <ListRow label={t('Server')} title={status.server} />}
        {status.name && <ListRow label={t('Name')} title={status.name} />}
      </ListSection>

      {current && (
        <ListSection
          footer={t('Nur hier auf diesem Gerät. Leer lassen: dann steht wieder {server} da.', {
            server: current.server,
          })}
        >
          <ListRow
            icon={ICONS.edit}
            iconTone="neutral"
            title={t('Konto umbenennen')}
            value={current.label}
            onClick={() => setRenaming(true)}
          />
        </ListSection>
      )}

      <ListSection>
        <ListRow
          icon={ICONS.sync}
          iconTone="neutral"
          title={status.syncing ? t('Synchronisiert …') : t('Jetzt synchronisieren')}
          subtitle={
            status.syncError
              ? t('Sync fehlgeschlagen')
              : t('Synchronisiert {when}', { when: ago(status.lastSync) })
          }
          chevron={false}
          disabled={status.syncing}
          onClick={() => void sync()}
        />
      </ListSection>

      <ListSection>
        <ListRow
          icon={ICONS.masterPassword}
          iconTone="neutral"
          title={t('Master-Passwort ändern')}
          value={t('im Web-Tresor')}
          onClick={() => void openWebVault().catch(toastError)}
        />
        <ListRow
          icon={ICONS.openExternal}
          iconTone="neutral"
          title={t('Web-Tresor öffnen')}
          chevron={false}
          onClick={() => void openWebVault().catch(toastError)}
        />
      </ListSection>

      {unlocked && <EmailBreachRow />}

      {unlocked && (
        <ListSection
          footer={t(
            'Einträge, Ordner, Anhänge, Sends und Organisationen aus Bitwarden oder Vaultwarden in dieses UwULock-Konto holen. Entschlüsselt wird nur auf diesem Gerät.',
          )}
        >
          <ListRow
            icon={ICONS.import}
            iconTone="neutral"
            title={t('Von Bitwarden umziehen')}
            onClick={() => setMoving(true)}
          />
        </ListSection>
      )}

      {current && (
        <RenameSheet
          open={renaming}
          onClose={() => setRenaming(false)}
          id={current.id}
          label={current.label}
          server={current.server}
        />
      )}
      {moving && <MoveDialog onClose={() => setMoving(false)} />}
    </Page>
  );
}

/** The account's name on this device (empty: the server's name again). */
function RenameSheet({
  open,
  onClose,
  id,
  label,
  server,
}: {
  open: boolean;
  onClose: () => void;
  id: string;
  label: string;
  server: string;
}) {
  useLanguage();
  const [value, setValue] = useState(label);
  useEffect(() => {
    if (open) setValue(label);
  }, [open, label]);
  const save = () => {
    onClose();
    void renameAccount(id, value.trim()).catch(toastError);
  };
  return (
    <EditSurface
      open={open}
      onClose={onClose}
      title={t('Konto umbenennen')}
      dirty={value !== label}
      action={{ label: t('Übernehmen'), onClick: save }}
    >
      <ListSection
        footer={t('Nur hier auf diesem Gerät. Leer lassen: dann steht wieder {server} da.', {
          server,
        })}
      >
        <FieldInput
          label={t('Name')}
          value={value}
          onChange={setValue}
          placeholder={server}
          maxLength={40}
          autoFocus
        />
      </ListSection>
    </EditSurface>
  );
}

/**
 * The check of addresses at XposedOrNot (UwULock-Server's docs/uwu-api.md
 * §15.4): only when the admin offers it, and only with this consent — the
 * server sends the addresses there in plain text, and the footer says so.
 */
function EmailBreachRow() {
  useLanguage();
  const [optIn, setOptIn] = useState<EmailOptIn | null>(null);
  const [busy, setBusy] = useState(false);
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
    try {
      setOptIn(await setEmailOptIn(on));
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <ListSection
      footer={t(
        'Die Passwortprüfung fragt dann auch, ob deine Kontoadresse und die Adressen, die in Logins als Benutzername stehen, in Datenlecks auftauchen. Dafür schickt dein Server jede dieser Adressen im Klartext an XposedOrNot (xposedornot.com) – deine Passwörter und deine anderen Daten nicht. Die Antworten merkt er sich eine Woche lang, nur unter einem Hash der Adresse.',
      )}
    >
      <ListRow
        icon={ICONS.mail}
        iconTone="neutral"
        title={t('Adressen in Datenlecks prüfen')}
        wrap
        trailing={
          <Toggle
            label={t('Adressen in Datenlecks prüfen')}
            checked={optIn.optedIn}
            disabled={busy}
            onChange={(on) => void change(on)}
          />
        }
      />
    </ListSection>
  );
}

/** Android has no updater: a new APK comes from the release page. */
function UpdatesPage() {
  useLanguage();
  return (
    <Page title={t('Neue Versionen')} largeTitle>
      <ListSection
        footer={t(
          'Auf Android aktualisiert sich UwULock nicht selbst. Neue Versionen (APK) gibt es auf der Release-Seite.',
        )}
      >
        <ListRow label={t('Installiert')} title={pkg.version} />
        <ListRow
          icon={ICONS.openExternal}
          iconTone="pink"
          title={t('Versionen')}
          chevron={false}
          onClick={() => void openProjectPage('releases').catch(toastError)}
        />
      </ListSection>
    </Page>
  );
}

function AboutPage() {
  useLanguage();
  const { android } = useMobile();
  const open = (page: ProjectPage) => void openProjectPage(page).catch(toastError);
  const link = (page: ProjectPage, title: string) => (
    <ListRow
      icon={ICONS.openExternal}
      iconTone="neutral"
      title={title}
      chevron={false}
      onClick={() => open(page)}
    />
  );
  return (
    <Page title={t('Über UwULock')} largeTitle>
      <div className="m-hero">
        <SuiteNyu shell="lock" size={76} mood="happy" title="Nyu" />
        <h2>UwULock</h2>
        <p className="m-hero-sub">{t('Version {version}', { version: pkg.version })}</p>
      </div>
      <ListSection
        footer={t(
          'Freie Software unter der GNU GPL v3.0. Nutzen, ändern, weitergeben – nur geänderte Versionen müssen offen bleiben. Kein Tracking, kein Konto.',
        )}
      >
        {link('source', t('Quellcode auf GitHub'))}
        {android && link('releases', t('Versionen'))}
        {link('issues', t('Fehler melden'))}
        {link('license', t('Lizenz'))}
        {link('suite', 'UwUSuite')}
      </ListSection>
      <p className="m-footnote">
        {t(
          'Spricht das Protokoll von Bitwarden und Vaultwarden, mit derselben Verschlüsselung wie die offiziellen Apps. Nicht verbunden mit Bitwarden Inc.',
        )}
      </p>
    </Page>
  );
}
