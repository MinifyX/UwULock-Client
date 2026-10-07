import { applyAppearance, resolveAppearance } from '@uwusuite/design';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { blockZoom } from '../../desktop/src/lib/zoom';
import { App } from './App';
import './styles.css';

document.documentElement.lang = navigator.language.toLowerCase().startsWith('de') ? 'de' : 'en';
// Installers are always light (the package's docs/window.md); contrast and motion follow the system.
applyAppearance(resolveAppearance({ theme: 'light' }));
// A fixed-size window: pinching or Ctrl + wheel would only cut it off (the app's lib/zoom.ts).
blockZoom();

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
