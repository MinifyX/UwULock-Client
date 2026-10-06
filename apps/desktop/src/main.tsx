import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import { prepareDocument } from './lib/appearance';
import './styles/index.css';

async function start() {
  // `pnpm dev` with `?mock`: a pretend Rust side for looking at the UI in a browser.
  if (import.meta.env.DEV && new URLSearchParams(window.location.search).has('mock')) {
    await import('./dev/mock');
  }
  prepareDocument();
  const root = document.getElementById('root');
  if (!root) throw new Error('#root missing from index.html');
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

void start();
