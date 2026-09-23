import { listen } from '@tauri-apps/api/event';
import { useEffect, useRef, useState } from 'react';
import { GeneratorDialog } from './components/GeneratorDialog';
import { Icon } from './components/Icon';
import { LockScreen } from './components/LockScreen';
import { LoginScreen } from './components/LoginScreen';
import { SettingsDialog, type SettingsSection } from './components/SettingsDialog';
import { TitleBar } from './components/TitleBar';
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
import { t, useLanguage } from './lib/i18n';
import { useSettings } from './lib/settings';
import { useToast } from './lib/toast';

export function App() {
  useLanguage();
  const settings = useSettings();
  const [status, setStatus] = useState<Status | null>(null);
  const [settingsOpen, setSettingsOpen] = useState<SettingsSection | null>(null);
  const [generator, setGenerator] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateDismissed, setUpdateDismissed] = useState(false);
  /** The login screen, for a second account next to the one already here. */
  const [adding, setAdding] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const backgroundRef = useRef<HTMLDivElement>(null);
  const current = useToast();

  // ── Vault state ──────────────────────────────────────────
  useEffect(() => {
    void vaultStatus().then(setStatus);
    const stop = listen<Status>('vault-status', ({ payload }) => setStatus(payload));
    return () => void stop.then((unlisten) => unlisten());
  }, []);

  useEffect(() => {
    void setSecurity(settings.autoLock || null, settings.clipboardClear || null).catch(
      () => undefined,
    );
  }, [settings.autoLock, settings.clipboardClear]);

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
  const modalOpen = Boolean(settingsOpen || generator);

  useEffect(() => {
    if (backgroundRef.current) backgroundRef.current.inert = modalOpen;
  }, [modalOpen]);

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
    <div className="shell">
      <div className="background" ref={backgroundRef}>
        <TitleBar onSettings={() => setSettingsOpen('appearance')}>
          <button
            className="titlebar-action"
            onClick={() => setGenerator(true)}
            title={t('Passwort-Generator (Strg+G)')}
            aria-label={t('Passwort-Generator')}
          >
            <Icon name="dice" size={17} />
          </button>
          {unlocked && (
            <button
              className="titlebar-action"
              onClick={() => void lock()}
              title={t('Sperren (Strg+L)')}
              aria-label={t('Sperren')}
            >
              <Icon name="lock" size={17} />
            </button>
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
              onUnlocked={setStatus}
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

      {current && (
        <div className="toast" data-tone={current.tone} role="status" key={current.id}>
          {current.text}
        </div>
      )}

      {update && !updateDismissed && !settingsOpen && (
        <UpdateHint
          update={update}
          onLater={() => setUpdateDismissed(true)}
          onRestart={installUpdate}
        />
      )}

      {generator && <GeneratorDialog onClose={() => setGenerator(false)} />}

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
  );
}
