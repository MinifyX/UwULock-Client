import { Icon, ICONS, TitleBar as SuiteTitleBar, TitleBarAction, Wordmark } from '@uwusuite/design';
import { useTauriWindow } from '@uwusuite/design/tauri';
import type { MouseEvent, ReactNode } from 'react';
import { t, useLanguage } from '../lib/i18n';
import { isMobile } from '../lib/platform';
import { desktop, withKeys } from '../lib/shortcuts';

type Props = {
  onSettings: () => void;
  /** The app's actions before the gear: travel mode, the generator, the lock. */
  children?: ReactNode;
};

/**
 * Tauri maximises on a double-click of a drag region by itself (its drag
 * script), and the package's title bar does it again in React — the two would
 * cancel out. The capture phase runs first and keeps the second one away; the
 * window buttons are not drag regions, so they are never touched by this.
 */
function leaveDoubleClickToTauri(event: MouseEvent) {
  if ((event.target as HTMLElement).hasAttribute('data-tauri-drag-region')) {
    event.stopPropagation();
  }
}

function Brand() {
  return <Wordmark product="Lock" shell="lock" className="text-body" />;
}

function SettingsAction({ onSettings }: { onSettings: () => void }) {
  return (
    <TitleBarAction label={withKeys(t('Einstellungen'), 'CmdOrCtrl+,')} onClick={onSettings}>
      <Icon icon={ICONS.settings} size="md" />
    </TitleBarAction>
  );
}

/**
 * The window's own title bar on Windows and Linux (@uwusuite/design's
 * TitleBar): the window has no system frame there (tauri.conf.json), so
 * moving, minimizing, maximizing and closing all happen here. Double-clicking
 * the empty bar maximizes, as everywhere on Windows.
 */
function WindowTitleBar({ onSettings, children }: Props) {
  const controls = useTauriWindow();
  return (
    <div onDoubleClickCapture={leaveDoubleClickToTauri}>
      <SuiteTitleBar
        platform={desktop}
        controls={controls}
        brand={<Brand />}
        actions={
          <>
            {children}
            <SettingsAction onSettings={onSettings} />
          </>
        }
      />
    </div>
  );
}

/** On a phone it is the app bar under the status bar: the same actions, no window to move or close. */
function AppBar({ onSettings, children }: Props) {
  return (
    <header className="uwu-titlebar app-bar">
      <span className="uwu-titlebar-brand">
        <Brand />
      </span>
      <span className="uwu-titlebar-spacer" />
      <span className="uwu-titlebar-actions">
        {children}
        <SettingsAction onSettings={onSettings} />
      </span>
    </header>
  );
}

/**
 * macOS draws the window's title bar itself (tauri.macos.conf.json), with the
 * traffic lights, and the actions live in the menu bar (App.tsx, setMacMenu),
 * so there is nothing to draw here.
 */
export function TitleBar(props: Props) {
  useLanguage();
  if (isMobile()) return <AppBar {...props} />;
  if (desktop === 'mac') return null;
  return <WindowTitleBar {...props} />;
}
