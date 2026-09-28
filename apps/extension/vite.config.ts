/// <reference types="vitest/config" />
import react from '@vitejs/plugin-react';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';

// The extension is built by scripts/build.mjs, one Vite build per part (pages, background,
// content scripts). This file is for the tests.
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@desktop': fileURLToPath(new URL('../desktop/src', import.meta.url)) },
  },
  test: {
    environment: 'jsdom',
    include: ['test/**/*.test.ts'],
  },
});
