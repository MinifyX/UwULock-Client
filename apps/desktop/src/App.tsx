import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Icon, ICONS, TitleBarAction, Toaster, useDeviceKind, UwuLabels } from '@uwusuite/design';
import { hideWindowOnClose, onMacQuit, setMacMenu } from '@uwusuite/design/tauri';
import { useEffect, useRef, useState } from 'react';
import { AutofillCard } from './components/AutofillCard';
import { ExtrasKeyNotice } from './components/ExtrasKeyNotice';
import { GeneratorDialog } from './components/GeneratorDialog';
import { ImportDialog } from './components/ImportDialog';
import { LockScreen } from './components/LockScreen';
import { LoginScreen } from './components/LoginScreen';
import { NyuStage, playNyu } from './components/nyu/stage';
import { PasskeyRequestDialog } from './components/PasskeyRequestDialog';
import { SettingsDialog, type SettingsSection } from './components/SettingsDialog';
import { TitleBar } from './components/TitleBar';
import { TravelBadge, TravelDialog, travelLabel, useTravel } from './components/TravelBadge';
import { UpdateHint } from './components/UpdateHint';
import { VaultScreen } from './components/VaultScreen';
import { MobileApp } from './mobile/MobileApp';
import {
  installUpdate,
  lock,
  openProjectPage,
  setSecurity,
  setUpdateChannel,
  touch,
  updateStatus,
  vaultStatus,
  type ProjectPage,
  type Status,
  type UpdateInfo,
} from './lib/api';
import { useAppAppearance } from './lib/appearance';
import { t, useLanguage } from './lib/i18n';
import { isMobile } from './lib/platform';
import { useSettings } from './lib/settings';
import { desktop, withKeys } from './lib/shortcuts';
import {
  onPasskeyProviderWarning,
  passkeyProviderStatus,
  warningText,
  type PasskeyProviderWarning,
} from './lib/passkeys';
import { toast, toasts } from './lib/toast';

export function App() {
  const language = useLanguage();
  const settings = useSettings();
  useAppAppearance(settings);
  const [status, setStatus] = useState<Status | null>(null);
  const [settingsOpen, setSettingsOpen] = useState<SettingsSection | null>(null);
  const [generator, setGenerator] = useState(false);
  const [importing, setImporting] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateDismissed, setUpdateDismissed] = useState(false);
  /** The login screen, for a second account next to the one already here. */
  const [adding, setAdding] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const travel = useTravel();
  // Phones and iPads get their own layout once the vault is open (src/mobile);
  // locking, logging in and the desktop keep the window's.
  const touchLayout = useDeviceKind() !== 'desktop';
  /** macOS: the travel mode dialog, opened from the menu bar. */
  const [travelOpen, setTravelOpen] = useState(false);

  // ── Vault state ──────────────────────────────────────────
  useEffect(() => {
    void vaultStatus().then(setStatus);
    const stop = listen<Status>('vault-status', ({ payload }) => setStatus(payload));
    return () => void stop.then((unlisten) => unlisten());
  }, []);

  useEffect(() => {
    void setSecurity(
      settings.autoLock || null,
      settings.clipboardClear || null,
      settings.lockWithSystem,
    ).catch(() => undefined);
  }, [settings.autoLock, settings.clipboardClear, settings.lockWithSystem]);

  // What counts as activity for auto-lock: keys, clicks, the wheel. Told to
  // Rust at most every 20 seconds.
  useEffect(() => {
    let last = 0;
    const active = () => {
      const now = Date.now();
      if (now - last < 20_000) return;
      last = now;
      void touch().catch(() => undefined);
    };
    for (const type of ['keydown', 'pointerdown', 'wheel'] as const)
      window.addEventListener(type, active, { passive: true, capture: true });
    return () => {
      for (const type of ['keydown', 'pointerdown', 'wheel'] as const)
        window.removeEventListener(type, active, { capture: true });
    };
  }, []);

  // ── Passkeys: another program stands in for UwULock ─────
  useEffect(() => {
    const show = (warning: PasskeyProviderWarning | null) => {
      if (warning) toast(warningText(warning), 'error');
    };
    // A warning from before the page listened (at start) shows once too.
    void passkeyProviderStatus()
      .then((status) => show(status.warning))
      .catch(() => undefined);
    const stop = onPasskeyProviderWarning(show);
    return () => void stop.then((unlisten) => unlisten());
  }, []);

  // ── Updates ──────────────────────────────────────────────
  useEffect(() => {
    void setUpdateChannel(settings.updateChannel).catch(() => undefined);
  }, [settings.updateChannel]);

  useEffect(() => {
    void updateStatus()
      .then((ready) => ready && setUpdate(ready))
      .catch(() => undefined);
    const stop = listen<UpdateInfo>('update:ready', (event) => {
      setUpdate(event.payload);
      setUpdateDismissed(false);
    });
    return () => void stop.then((unlisten) => unlisten());
  }, []);

  const unlocked = status?.state === 'unlocked';
  const mobileVault = touchLayout && unlocked && !adding && !status.sessionExpired;
  // The dialogs are native <dialog>s: the page behind them is inert while they are open.
  const modalOpen = Boolean(settingsOpen || generator || importing);
  // A locked vault ends the import dialog: the file's contents go with it.
  useEffect(() => {
    if (!unlocked) setImporting(false);
  }, [unlocked]);

  const focusSearch = () => {
    searchRef.current?.focus();
    searchRef.current?.select();
  };
  const openSettings = () => setSettingsOpen((open) => open ?? 'appearance');

  // ── macOS ────────────────────────────────────────────────
  // ⌘W and the red light hide the window, a click on the Dock icon brings it
  // back (RunEvent::Reopen in lib.rs); auto-lock keeps running meanwhile. ⌘Q,
  // the Dock and logging out go through the quit guard (uwu-macos): there is
  // nothing to save — every change is written as it is made — so it only lets
  // the quit go ahead in order. Off macOS all three do nothing.
  useEffect(() => {
    if (desktop !== 'mac' || isMobile()) return;
    const stops = [hideWindowOnClose(), onMacQuit(() => true)];
    return () => void Promise.all(stops).then((list) => list.forEach((stop) => stop()));
  }, []);

  const help = (page: ProjectPage) => void openProjectPage(page).catch(() => undefined);

  // The title bar's actions, in the menu bar. It is set again whenever an
  // entry changes; the keyboard handler below stays for the other systems
  // (on a Mac both may see a shortcut, and every action here is idempotent).
  useEffect(() => {
    if (desktop !== 'mac' || isMobile()) return;
    void setMacMenu({
      appName: 'UwULock',
      lang: language,
      onSettings: openSettings,
      app: travel.enabled
        ? [{ text: `${travelLabel(travel.hidden)} …`, action: () => setTravelOpen(true) }]
        : [],
      edit: [
        {
          text: t('Tresor durchsuchen'),
          accelerator: 'CmdOrCtrl+F',
          enabled: unlocked && !modalOpen,
          action: focusSearch,
        },
      ],
      menus: [
        {
          text: t('Tresor'),
          items: [
            {
              text: `${t('Passwort-Generator')} …`,
              accelerator: 'CmdOrCtrl+G',
              enabled: !modalOpen,
              action: () => setGenerator(true),
            },
            {
              text: `${t('Importieren')} …`,
              enabled: unlocked && !modalOpen,
              action: () => setImporting(true),
            },
            'separator',
            {
              text: t('Sperren'),
              accelerator: 'CmdOrCtrl+L',
              enabled: unlocked && !modalOpen,
              action: () => void lock(),
            },
          ],
        },
      ],
      help: [
        { text: t('Versionen'), action: () => help('releases') },
        { text: t('Quellcode auf GitHub'), action: () => help('source') },
        { text: t('Problem melden'), action: () => help('issues') },
        'separator',
        { text: 'UwUSuite', action: () => help('suite') },
      ],
    }).catch(() => undefined);
    // The handlers only use setters and the search field's ref.
  }, [language, unlocked, modalOpen, travel.enabled, travel.hidden]);

  // Travel mode on a Mac: the native title bar says it, where the pill would be.
  useEffect(() => {
    if (desktop !== 'mac' || isMobile()) return;
    const title = travel.enabled ? `UwULock · ${travelLabel(travel.hidden)}` : 'UwULock';
    void getCurrentWindow()
      .setTitle(title)
      .catch(() => undefined);
  }, [language, travel.enabled, travel.hidden]);

  // ── Keyboard ─────────────────────────────────────────────
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const mod = event.ctrlKey || event.metaKey;
      if (!mod || event.altKey) return;
      const key = event.key.toLowerCase();
      // The phone and iPad layout has its own keys (⌘F, ⌘N) and its settings tab.
      if (mobileVault && key !== 'l') return;
      if (key === ',') {
        event.preventDefault();
        openSettings();
      } else if (modalOpen) {
        return;
      } else if (key === 'l' && unlocked) {
        event.preventDefault();
        void lock();
      } else if (key === 'f' && unlocked) {
        event.preventDefault();
        focusSearch();
      } else if (key === 'g') {
        event.preventDefault();
        setGenerator(true);
      } else if (key === 'r' && !event.shiftKey) {
        // A reload would throw away the page; the vault stays open in Rust, but nothing is gained.
        event.preventDefault();
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [modalOpen, unlocked, mobileVault]);

  if (mobileVault && status)
    return (
      <UwuLabels labels={language}>
        <div className="m-root">
          <MobileApp status={status} onAddAccount={() => setAdding(true)} />
        </div>
        <NyuStage />
        <AutofillCard unlocked />
        <ExtrasKeyNotice />
        <PasskeyRequestDialog />
      </UwuLabels>
    );

  return (
    <UwuLabels labels={language}>
      <div className="shell">
        <div className="background">
          <TitleBar onSettings={() => setSettingsOpen('appearance')}>
            {unlocked && <TravelBadge />}
            <TitleBarAction
              label={withKeys(t('Passwort-Generator'), 'CmdOrCtrl+G')}
              onClick={() => setGenerator(true)}
            >
              <Icon icon={ICONS.generate} size="md" />
            </TitleBarAction>
            {unlocked && (
              <TitleBarAction
                label={withKeys(t('Sperren'), 'CmdOrCtrl+L')}
                onClick={() => void lock()}
              >
                <Icon icon={ICONS.locked} size="md" />
              </TitleBarAction>
            )}
          </TitleBar>

          <main className="stage">
            {status === null ? null : status.state === 'logged-out' ? (
              <LoginScreen onDone={setStatus} />
            ) : adding ? (
              <LoginScreen
                adding
                onDone={(next) => {
                  setAdding(false);
                  setStatus(next);
                }}
                onCancel={() => setAdding(false)}
              />
            ) : status.state === 'locked' ? (
              <LockScreen
                status={status}
                onUnlocked={(next) => {
                  setStatus(next);
                  playNyu('unlocked');
                }}
                onLoggedOut={() => void vaultStatus().then(setStatus)}
                onAddAccount={() => setAdding(true)}
              />
            ) : status.sessionExpired ? (
              <LoginScreen again={status} onDone={setStatus} onCancel={() => void lock()} />
            ) : (
              <VaultScreen
                status={status}
                searchRef={searchRef}
                onAddAccount={() => setAdding(true)}
              />
            )}
          </main>
        </div>

        <Toaster store={toasts} />

        <NyuStage />

        {update && !updateDismissed && !settingsOpen && (
          <UpdateHint
            update={update}
            onLater={() => setUpdateDismissed(true)}
            onRestart={installUpdate}
          />
        )}

        {generator && <GeneratorDialog onClose={() => setGenerator(false)} />}

        {unlocked && importing && <ImportDialog onClose={() => setImporting(false)} />}

        {unlocked && <ExtrasKeyNotice />}

        {/* The Mac App Store build (with the AutoFill extension); nothing on Windows or Linux. */}
        {unlocked && !status?.sessionExpired && !update && !settingsOpen && (
          <AutofillCard unlocked />
        )}

        {unlocked && travelOpen && travel.enabled && (
          <TravelDialog hidden={travel.hidden} onClose={() => setTravelOpen(false)} />
        )}

        {status && status.state !== 'logged-out' && <PasskeyRequestDialog />}

        {settingsOpen && status && (
          <SettingsDialog
            initial={settingsOpen}
            status={status}
            onClose={() => setSettingsOpen(null)}
            update={update}
            onUpdateFound={(found) => {
              setUpdate(found);
              setUpdateDismissed(false);
            }}
            onInstallUpdate={() => {
              setSettingsOpen(null);
              setUpdateDismissed(false);
            }}
          />
        )}
      </div>
    </UwuLabels>
  );
}
