import { listen } from '@tauri-apps/api/event';
import { Icon, ICONS, TitleBarAction, Toaster, UwuLabels } from '@uwusuite/design';
import { useEffect, useRef, useState } from 'react';
import { ExtrasKeyNotice } from './components/ExtrasKeyNotice';
import { GeneratorDialog } from './components/GeneratorDialog';
import { LockScreen } from './components/LockScreen';
import { LoginScreen } from './components/LoginScreen';
import { NyuStage, playNyu } from './components/nyu/stage';
import { PasskeyRequestDialog } from './components/PasskeyRequestDialog';
import { SettingsDialog, type SettingsSection } from './components/SettingsDialog';
import { TitleBar } from './components/TitleBar';
import { TravelBadge } from './components/TravelBadge';
import { UpdateHint } from './components/UpdateHint';
import { VaultScreen } from './components/VaultScreen';
import {
  installUpdate,
  lock,
  setSecurity,
  setUpdateChannel,
  touch,
  updateStatus,
  vaultStatus,
  type Status,
  type UpdateInfo,
} from './lib/api';
import { useAppAppearance } from './lib/appearance';
import { t, useLanguage } from './lib/i18n';
import { useSettings } from './lib/settings';
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
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateDismissed, setUpdateDismissed] = useState(false);
  /** The login screen, for a second account next to the one already here. */
  const [adding, setAdding] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);

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
  // The dialogs are native <dialog>s: the page behind them is inert while they are open.
  const modalOpen = Boolean(settingsOpen || generator);

  // ── Keyboard ─────────────────────────────────────────────
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const mod = event.ctrlKey || event.metaKey;
      if (!mod || event.altKey) return;
      const key = event.key.toLowerCase();
      if (key === ',') {
        event.preventDefault();
        setSettingsOpen((open) => open ?? 'appearance');
      } else if (modalOpen) {
        return;
      } else if (key === 'l' && unlocked) {
        event.preventDefault();
        void lock();
      } else if (key === 'f' && unlocked) {
        event.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
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
  }, [modalOpen, unlocked]);

  return (
    <UwuLabels labels={language}>
      <div className="shell">
        <div className="background">
          <TitleBar onSettings={() => setSettingsOpen('appearance')}>
            {unlocked && <TravelBadge />}
            {unlocked && <ExtrasKeyNotice />}
            <TitleBarAction
              label={t('Passwort-Generator (Strg+G)')}
              onClick={() => setGenerator(true)}
            >
              <Icon icon={ICONS.generate} size="md" />
            </TitleBarAction>
            {unlocked && (
              <TitleBarAction label={t('Sperren (Strg+L)')} onClick={() => void lock()}>
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
