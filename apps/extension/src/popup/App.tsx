/**
 * The popup: logging in, unlocking, and the vault — this page's logins first, then everything,
 * the generator and the settings, in a window 380 pixels wide.
 */

import { useEffect, useState } from 'react';
import { Icon } from '@desktop/components/Icon';
import { t } from '../shared/i18n';
import { lock, syncNow } from './api';
import { errorText, ToastView, toast, useSettings, useStatus } from './lib';
import { Detail } from './views/Detail';
import { Editor, type EditorTarget } from './views/Editor';
import { Generator } from './views/Generator';
import { LockView } from './views/Lock';
import { LoginView } from './views/Login';
import { SettingsView } from './views/Settings';
import { TabView } from './views/TabView';
import { VaultView } from './views/VaultView';

export type Tab = 'page' | 'vault' | 'generator' | 'settings';

/** What is on top of the tabs: an item, the editor, or nothing. */
export type Layer = { kind: 'item'; id: string } | { kind: 'edit'; target: EditorTarget } | null;

export function App() {
  useSettings();
  const [status, refresh] = useStatus();
  const [tab, setTab] = useState<Tab>('page');
  const [layer, setLayer] = useState<Layer>(null);
  const [adding, setAdding] = useState(false);

  // Locked or logged out: whatever was open closes.
  useEffect(() => {
    if (status?.state !== 'unlocked') setLayer(null);
    if (status?.state === 'unlocked') setAdding(false);
  }, [status?.state]);

  if (!status) return <div className="popup popup-loading" aria-busy />;

  if (status.state === 'logged-out' || adding) {
    return (
      <div className="popup">
        <LoginView
          status={status}
          adding={adding}
          onCancel={adding ? () => setAdding(false) : undefined}
          onDone={() => void refresh()}
        />
        <ToastView />
      </div>
    );
  }

  if (status.state === 'locked') {
    return (
      <div className="popup">
        <LockView
          status={status}
          onAddAccount={() => setAdding(true)}
          onDone={() => void refresh()}
        />
        <ToastView />
      </div>
    );
  }

  const sync = async () => {
    try {
      await syncNow();
      toast(t('Synchronisiert ✧'));
    } catch (e) {
      toast(errorText(e), 'error');
    }
  };

  const tabs: { id: Tab; label: string; icon: Parameters<typeof Icon>[0]['name'] }[] = [
    { id: 'page', label: t('Diese Seite'), icon: 'globe' },
    { id: 'vault', label: t('Tresor'), icon: 'layers' },
    { id: 'generator', label: t('Generator'), icon: 'dice' },
    { id: 'settings', label: t('Einstellungen'), icon: 'more' },
  ];

  return (
    <div className="popup">
      <header className="popup-bar">
        <span className="popup-brand">
          <img src="/icons/icon-32.png" alt="" width="20" height="20" />
          UwU<span>Lock</span>
        </span>
        <span className="spacer" />
        <button
          type="button"
          className="icon-button"
          onClick={() => setLayer({ kind: 'edit', target: { id: null, kind: 'login' } })}
          aria-label={t('Neuer Eintrag')}
          title={t('Neuer Eintrag')}
        >
          <Icon name="plus" size={17} />
        </button>
        <button
          type="button"
          className="icon-button"
          onClick={() => void sync()}
          disabled={status.syncing}
          aria-label={t('Jetzt synchronisieren')}
          title={t('Jetzt synchronisieren')}
        >
          <Icon name="refresh" size={16} className={status.syncing ? 'spin' : undefined} />
        </button>
        <button
          type="button"
          className="icon-button"
          onClick={() => void lock().then(refresh)}
          aria-label={t('Sperren')}
          title={t('Sperren')}
        >
          <Icon name="lock" size={16} />
        </button>
      </header>

      <main className="popup-main">
        {layer?.kind === 'item' && (
          <Detail
            id={layer.id}
            onBack={() => setLayer(null)}
            onEdit={(id, kind) => setLayer({ kind: 'edit', target: { id, kind } })}
          />
        )}
        {layer?.kind === 'edit' && (
          <Editor
            target={layer.target}
            onDone={(id) => setLayer(id ? { kind: 'item', id } : null)}
            onCancel={() =>
              setLayer(layer.target.id ? { kind: 'item', id: layer.target.id } : null)
            }
          />
        )}
        {!layer && tab === 'page' && (
          <TabView
            onOpen={(id) => setLayer({ kind: 'item', id })}
            onNew={(target) => setLayer({ kind: 'edit', target })}
          />
        )}
        {!layer && tab === 'vault' && <VaultView onOpen={(id) => setLayer({ kind: 'item', id })} />}
        {!layer && tab === 'generator' && <Generator />}
        {!layer && tab === 'settings' && (
          <SettingsView status={status} onAddAccount={() => setAdding(true)} />
        )}
      </main>

      {!layer && (
        <nav className="popup-tabs" aria-label={t('Bereiche')}>
          {tabs.map((entry) => (
            <button
              key={entry.id}
              type="button"
              aria-current={tab === entry.id ? 'page' : undefined}
              onClick={() => setTab(entry.id)}
            >
              <Icon name={entry.icon} size={18} />
              <span>{entry.label}</span>
            </button>
          ))}
        </nav>
      )}
      <ToastView />
    </div>
  );
}
