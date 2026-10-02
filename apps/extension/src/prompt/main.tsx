import '@fontsource-variable/dm-sans';
import '@fontsource-variable/manrope';
import '@fontsource-variable/rubik';
import React from 'react';
import ReactDOM from 'react-dom/client';
import '@desktop/styles/tokens.css';
import '@desktop/styles/fonts.css';
import '@desktop/styles/app.css';
import '@desktop/styles/vault.css';
import '@desktop/components/nyu/nyu.css';
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
