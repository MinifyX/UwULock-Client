import React from 'react';
import ReactDOM from 'react-dom/client';
import '../popup/index.css';
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
