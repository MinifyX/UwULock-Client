import { detectDeviceKind, DeviceKindProvider } from '@uwusuite/design';
import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import { prepareDocument } from './lib/appearance';
import { isIosAppOnMac } from './lib/platform';
import './styles/index.css';

async function start() {
  // `pnpm dev` with `?mock`: a pretend Rust side for looking at the UI in a browser.
  if (import.meta.env.DEV && new URLSearchParams(window.location.search).has('mock')) {
    await import('./dev/mock');
  }
  prepareDocument();
  const root = document.getElementById('root');
  if (!root) throw new Error('#root missing from index.html');
  const phone = phoneKind();
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      {phone ? (
        <DeviceKindProvider kind={phone}>
          <App />
        </DeviceKindProvider>
      ) : (
        <App />
      )}
    </React.StrictMode>,
  );
}

/**
 * A phone stays a phone when it is turned sideways. The package decides by the
 * window's width, so a landscape iPhone would become an iPad and a landscape
 * Android phone the desktop — the whole UI swapped mid-edit, with its open
 * sheets and page stacks gone. Decided once by the screen's short side; an
 * iPad or a tablet keeps following its window (Split View, Slide Over).
 */
function phoneKind() {
  // The iPhone/iPad app on a Mac is the iPad app, whatever its window's width
  // (lib/platform.ts): macOS draws it at 77 %, which suits the iPad's sizes.
  if (isIosAppOnMac()) return 'ipad' as const;
  const kind = detectDeviceKind({
    userAgent: navigator.userAgent,
    width: Math.min(window.screen.width, window.screen.height),
    maxTouchPoints: navigator.maxTouchPoints ?? 0,
    coarsePointer: window.matchMedia?.('(pointer: coarse)').matches,
  });
  return kind === 'phone-ios' || kind === 'phone-android' ? kind : null;
}

void start();
