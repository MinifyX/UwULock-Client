// Builds the browser extension, for Chromium and for Firefox, from one source:
//
//   node scripts/build.mjs          (pnpm --filter @uwulock/extension build)
//
// Needs the WebAssembly in src/wasm/pkg first (node scripts/build-wasm.mjs, or `pnpm wasm`).
//
// One Vite build per part: the pages (popup, prompt window, offscreen document) as ES modules,
// the background as one ES module, and each content script as one self-contained script — a
// content script can't load modules. Both browsers get the same files and their own manifest:
//
//   dist/chromium/, dist/firefox/                  unpacked, for "Load unpacked" / about:debugging
//   ../../target/extension/UwULock-extension-chromium.zip
//   ../../target/extension/UwULock-extension-firefox.xpi   (a zip, unsigned)

import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deflateRawSync } from 'node:zlib';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { build } from 'vite';
import { manifest } from './manifest.mjs';

const app = join(dirname(fileURLToPath(import.meta.url)), '..');
const root = join(app, '../..');
const out = join(app, 'dist');
const common = join(out, 'common');
const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;

if (!existsSync(join(app, 'src/wasm/pkg/core.js'))) {
  console.error('src/wasm/pkg is missing: build the WebAssembly first (pnpm wasm).');
  process.exit(1);
}

rmSync(out, { recursive: true, force: true });

const quiet = {
  logLevel: 'warn',
  configFile: false,
  // The desktop app's design: tokens, styles, Nyu, icons — one source for both.
  resolve: { alias: { '@desktop': join(root, 'apps/desktop/src') } },
};

// The pages. Their HTML sits in src/, so the output has popup.html etc. at the top. menu.html is
// the inline menu's list, which content scripts show in a frame in web pages.
await build({
  ...quiet,
  root: join(app, 'src'),
  base: '/',
  publicDir: join(app, 'public'),
  // Tailwind and @uwusuite/design for the popup and the passkey window (popup/index.css). The
  // content scripts' UI never gets it: its sheet lives in a closed shadow root (content/ui.ts).
  plugins: [react(), tailwindcss()],
  build: {
    outDir: common,
    emptyOutDir: true,
    target: 'es2022',
    sourcemap: false,
    modulePreload: { polyfill: false },
    assetsInlineLimit: 0,
    chunkSizeWarningLimit: 1500,
    rollupOptions: {
      input: {
        popup: join(app, 'src/popup.html'),
        prompt: join(app, 'src/prompt.html'),
        offscreen: join(app, 'src/offscreen.html'),
        menu: join(app, 'src/menu.html'),
      },
    },
  },
});

// The background, and the scripts in pages: one file each.
const single = [
  { entry: 'src/background/index.ts', file: 'background.js', format: 'es' },
  { entry: 'src/content/autofill.ts', file: 'content.js', format: 'iife' },
  { entry: 'src/content/bridge.ts', file: 'bridge.js', format: 'iife' },
  { entry: 'src/page/webauthn.ts', file: 'page.js', format: 'iife' },
];
/**
 * wasm-bindgen's loader falls back to `new URL('core_bg.wasm', import.meta.url)`, which a
 * library build would inline as a data URL — 1.6 MB of base64 in the background for a path
 * that is never taken: the background passes the file's own address (core.wasm, copied below).
 */
const noWasmFallback = {
  name: 'uwulock-no-wasm-fallback',
  transform(code, id) {
    if (!id.endsWith('/wasm/pkg/core.js')) return null;
    return code.replace("new URL('core_bg.wasm', import.meta.url)", 'undefined');
  },
};

for (const part of single) {
  await build({
    ...quiet,
    root: app,
    publicDir: false,
    plugins: [noWasmFallback],
    build: {
      outDir: common,
      emptyOutDir: false,
      target: 'es2022',
      sourcemap: false,
      assetsInlineLimit: 0,
      // Content scripts run in pages: small, and nothing that looks like eval.
      minify: true,
      lib: {
        entry: join(app, part.entry),
        formats: [part.format],
        name: 'uwulock',
        fileName: () => part.file,
      },
      rollupOptions: {
        output: { inlineDynamicImports: true, assetFileNames: 'assets/[name]-[hash][extname]' },
      },
    },
  });
}

/** Every file below `dir`, relative to it, sorted: the same input makes the same zip. */
function files(dir, base = dir) {
  return readdirSync(dir)
    .sort()
    .flatMap((name) => {
      const path = join(dir, name);
      return statSync(path).isDirectory()
        ? files(path, base)
        : [relative(base, path).replaceAll('\\', '/')];
    });
}

// ── A zip, without a dependency: deflate from node:zlib, the rest by hand ───

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

/** Fixed timestamps (1 January 2026), so a build of the same source is the same file. */
const DOS_TIME = 0;
const DOS_DATE = ((2026 - 1980) << 9) | (1 << 5) | 1;

function zip(dir, target) {
  const local = [];
  const central = [];
  let offset = 0;
  for (const name of files(dir)) {
    const data = readFileSync(join(dir, name));
    const packed = deflateRawSync(data, { level: 9 });
    const deflate = packed.length < data.length;
    const body = deflate ? packed : data;
    const nameBytes = Buffer.from(name, 'utf8');
    const crc = crc32(data);
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50, 0);
    header.writeUInt16LE(20, 4);
    header.writeUInt16LE(0x0800, 6);
    header.writeUInt16LE(deflate ? 8 : 0, 8);
    header.writeUInt16LE(DOS_TIME, 10);
    header.writeUInt16LE(DOS_DATE, 12);
    header.writeUInt32LE(crc, 14);
    header.writeUInt32LE(body.length, 18);
    header.writeUInt32LE(data.length, 22);
    header.writeUInt16LE(nameBytes.length, 26);
    local.push(header, nameBytes, body);
    const entry = Buffer.alloc(46);
    entry.writeUInt32LE(0x02014b50, 0);
    entry.writeUInt16LE(20, 4);
    entry.writeUInt16LE(20, 6);
    entry.writeUInt16LE(0x0800, 8);
    entry.writeUInt16LE(deflate ? 8 : 0, 10);
    entry.writeUInt16LE(DOS_TIME, 12);
    entry.writeUInt16LE(DOS_DATE, 14);
    entry.writeUInt32LE(crc, 16);
    entry.writeUInt32LE(body.length, 20);
    entry.writeUInt32LE(data.length, 24);
    entry.writeUInt16LE(nameBytes.length, 28);
    entry.writeUInt32LE(offset, 42);
    central.push(entry, nameBytes);
    offset += header.length + nameBytes.length + body.length;
  }
  const centralSize = central.reduce((sum, part) => sum + part.length, 0);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(central.length / 2, 8);
  end.writeUInt16LE(central.length / 2, 10);
  end.writeUInt32LE(centralSize, 12);
  end.writeUInt32LE(offset, 16);
  writeFileSync(target, Buffer.concat([...local, ...central, end]));
}

cpSync(join(app, 'src/wasm/pkg/core_bg.wasm'), join(common, 'core.wasm'));

const packages = join(root, 'target/extension');
mkdirSync(packages, { recursive: true });
for (const [browser, file] of [
  ['chromium', 'UwULock-extension-chromium.zip'],
  ['firefox', 'UwULock-extension-firefox.xpi'],
]) {
  const dir = join(out, browser);
  cpSync(common, dir, { recursive: true });
  // The offscreen document is Chromium's alone.
  if (browser === 'firefox') {
    rmSync(join(dir, 'offscreen.html'), { force: true });
  }
  writeFileSync(
    join(dir, 'manifest.json'),
    `${JSON.stringify(manifest(browser, version), null, 2)}\n`,
  );
  zip(dir, join(packages, file));
  console.log(
    `✓ ${relative(root, join(packages, file))} (${Math.round(statSync(join(packages, file)).size / 1024)} KiB)`,
  );
}
rmSync(common, { recursive: true, force: true });
