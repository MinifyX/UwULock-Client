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
import '../popup/popup.css';
import { loadSettings } from '../popup/lib';
import { Prompt } from './Prompt';

void loadSettings()
  .catch(() => undefined)
  .finally(() => {
    ReactDOM.createRoot(document.getElementById('root')!).render(
      <React.StrictMode>
        <Prompt id={new URLSearchParams(location.search).get('id') ?? ''} />
      </React.StrictMode>,
    );
  });
