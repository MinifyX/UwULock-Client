import '@fontsource-variable/dm-sans';
import '@fontsource-variable/manrope';
import '@fontsource-variable/rubik';
import React from 'react';
import ReactDOM from 'react-dom/client';
import '../legacy/tokens.css';
import '../legacy/fonts.css';
import '../legacy/app.css';
import '../legacy/vault.css';
import '../legacy/nyu.css';
import './popup.css';
import { ext } from '../shared/browser';
import { App } from './App';
import { loadSettings } from './lib';

// A port that lives as long as the popup: "lock when the popup closes" listens for its end.
if (!new URLSearchParams(location.search).has('window')) ext.runtime.connect({ name: 'popup' });

void loadSettings()
  .catch(() => undefined)
  .finally(() => {
    ReactDOM.createRoot(document.getElementById('root')!).render(
      <React.StrictMode>
        <App />
      </React.StrictMode>,
    );
  });
