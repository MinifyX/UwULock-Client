/**
 * The popup: logging in, unlocking, and the vault — this page's logins first, then everything,
 * the generator and the settings, in a window 380 pixels wide.
 */

import { Icon, IconButton, ICONS, Wordmark } from '@uwusuite/design';
import type { LucideIcon } from 'lucide-react';
import { useEffect, useState } from 'react';
import { playNyu } from '@desktop/components/nyu/stage';
import { t } from '../shared/i18n';
import { lock, syncNow } from './api';
import { setIconsEnabled } from './icons';
import { toast, toastError, ToastView, useSettings, useStatus, uwuFeature } from './lib';
import { Detail } from './views/Detail';
import { Editor, type EditorTarget } from './views/Editor';
import { FileRequestsView } from './views/FileRequests';
import { Generator } from './views/Generator';
import { LockView } from './views/Lock';
import { LoginView } from './views/Login';
import { SettingsView } from './views/Settings';
import { ShareView } from './views/Share';
import { TabView } from './views/TabView';
import { VaultView } from './views/VaultView';

export type Tab = 'page' | 'vault' | 'generator' | 'settings';

/** What is on top of the tabs: an item, the editor, or nothing. */
export type Layer =
  | { kind: 'item'; id: string }
  | { kind: 'edit'; target: EditorTarget }
  | { kind: 'share'; id: string }
  | { kind: 'file-requests' }
  | null;

export function App() {
  const settings = useSettings();
  const [status, refresh] = useStatus();
  const [tab, setTab] = useState<Tab>('page');
  const [layer, setLayer] = useState<Layer>(null);
  const [adding, setAdding] = useState(false);

  // Locked or logged out: whatever was open closes.
  useEffect(() => {
    if (status?.state !== 'unlocked') setLayer(null);
    if (status?.state === 'unlocked') setAdding(false);
  }, [status?.state]);

  // An admin switched file requests off while their list was open: back to where it came from.
  const fileRequestsOn = uwuFeature(status, 'file-requests');
  useEffect(() => {
    if (!fileRequestsOn && layer?.kind === 'file-requests') setLayer(null);
  }, [fileRequestsOn, layer?.kind]);

  // Icons come from UwULock Server only: own ones, or its automatic ones.
  const icons = Boolean(
    settings?.showIcons &&
    (uwuFeature(status, 'own-icons') || status?.uwu?.icons?.automatic === true),
  );
  useEffect(() => setIconsEnabled(icons), [icons]);

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
          onDone={() => {
            playNyu('unlocked');
            void refresh();
          }}
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
      toastError(e);
    }
  };

  const tabs: { id: Tab; label: string; icon: LucideIcon }[] = [
    { id: 'page', label: t('Diese Seite'), icon: ICONS.website },
    { id: 'vault', label: t('Tresor'), icon: ICONS.vault },
    { id: 'generator', label: t('Generator'), icon: ICONS.generate },
    { id: 'settings', label: t('Einstellungen'), icon: ICONS.settings },
  ];

  return (
    <div className="popup">
      <header className="popup-bar">
        <Wordmark product="Lock" shell="lock" className="text-body" />
        <span className="spacer" />
        <IconButton
          icon={ICONS.add}
          label={t('Neuer Eintrag')}
          onClick={() => setLayer({ kind: 'edit', target: { id: null, kind: 'login' } })}
        />
        <IconButton
          icon={ICONS.refresh}
          label={t('Jetzt synchronisieren')}
          className={status.syncing ? '[&>svg]:animate-spin' : undefined}
          onClick={() => void sync()}
          disabled={status.syncing}
        />
        <IconButton
          icon={ICONS.locked}
          label={t('Sperren')}
          onClick={() => void lock().then(refresh)}
        />
      </header>

      <main className="popup-main">
        {layer?.kind === 'item' && (
          <Detail
            id={layer.id}
            onBack={() => setLayer(null)}
            onEdit={(id, kind) => setLayer({ kind: 'edit', target: { id, kind } })}
            onShare={(id) => setLayer({ kind: 'share', id })}
          />
        )}
        {layer?.kind === 'share' && (
          <ShareView
            id={layer.id}
            entry={Boolean(status.uwu)}
            onBack={() => setLayer({ kind: 'item', id: layer.id })}
          />
        )}
        {layer?.kind === 'file-requests' && <FileRequestsView onBack={() => setLayer(null)} />}
        {layer?.kind === 'edit' && (
          <Editor
            status={status}
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
        {!layer && tab === 'generator' && <Generator status={status} />}
        {!layer && tab === 'settings' && (
          <SettingsView
            status={status}
            onAddAccount={() => setAdding(true)}
            onFileRequests={() => setLayer({ kind: 'file-requests' })}
          />
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
              <Icon icon={entry.icon} size="md" />
              <span>{entry.label}</span>
            </button>
          ))}
        </nav>
      )}
      <ToastView raised={!layer} />
    </div>
  );
}
