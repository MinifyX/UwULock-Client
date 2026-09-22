// The whole end-to-end run, in one command:
//
//   node apps/desktop/e2e/run.mjs
//
// Builds and starts the toy Vaultwarden (`dev_vaultwarden`, with two-step
// login on), starts the app in dev mode against a throwaway data folder with
// WebView2's DevTools port open on 127.0.0.1, runs phase A — log in with an
// email code, browse and search the vault, reveal and copy, a one-time code,
// a re-prompted item, lock and unlock, log out — and stops everything again.
// Screenshots land in apps/desktop/e2e/shots/.
//
// Windows only: it drives WebView2 over the Chrome DevTools Protocol.

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, openSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const desktop = join(here, '..');
const repo = join(desktop, '..', '..');
const runDir = join(here, '.run');
const toyExe = join(repo, 'target', 'debug', 'examples', 'dev_vaultwarden.exe');
const TOY_ADDR = '127.0.0.1:8097';

rmSync(runDir, { recursive: true, force: true });
mkdirSync(runDir, { recursive: true });
mkdirSync(join(here, 'shots'), { recursive: true });

const children = [];

function start(name, command, args, options = {}) {
  const log = join(runDir, `${name}.log`);
  // Straight into the file: a pipe is only drained while this event loop runs.
  const fd = openSync(log, 'w');
  const child = spawn(command, args, {
    ...options,
    shell: command === 'pnpm',
    stdio: ['ignore', fd, fd],
  });
  children.push(child);
  return { child, log };
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function until(condition, what, timeout = 600_000) {
  const started = Date.now();
  while (!(await condition())) {
    if (Date.now() - started > timeout) throw new Error(`gave up waiting for ${what}`);
    await sleep(500);
  }
}

function phase(script, args) {
  console.log(`\n── ${script} ──`);
  return new Promise((resolve) => {
    spawn(process.execPath, [join(here, script), ...args], { stdio: 'inherit' }).on(
      'exit',
      (code) => resolve(code === 0),
    );
  });
}

let ok = false;
try {
  console.log('▸ building the toy Vaultwarden');
  const build = spawnSync(
    'cargo',
    ['build', '-p', 'uwulock-bitwarden', '--example', 'dev_vaultwarden'],
    { cwd: repo, stdio: 'inherit' },
  );
  if (build.status !== 0) throw new Error('cargo build failed');

  const toy = start('toy-vaultwarden', toyExe, [], {
    cwd: repo,
    env: { ...process.env, UWU_2FA: '1', UWU_TOY_ADDR: TOY_ADDR },
  });
  await until(
    () => existsSync(toy.log) && readFileSync(toy.log, 'utf8').includes('toy Vaultwarden on'),
    'the toy Vaultwarden',
  );

  console.log('▸ starting UwULock (pnpm tauri dev)');
  start('app', 'pnpm', ['tauri', 'dev'], {
    cwd: desktop,
    env: {
      ...process.env,
      UWULOCK_DATA_DIR: join(runDir, 'data'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:
        '--remote-debugging-port=9223 --remote-debugging-address=127.0.0.1',
      // A WebView2 folder of its own: an installed UwULock that is running
      // shares the default one, and its browser process would ignore the port.
      WEBVIEW2_USER_DATA_FOLDER: join(runDir, 'webview'),
    },
  });
  await until(async () => {
    try {
      const list = await (await fetch('http://127.0.0.1:9223/json/list')).json();
      return list.some((target) => target.url.startsWith('http://localhost:1420'));
    } catch {
      return false;
    }
  }, 'the app');

  ok = await phase('phase-a.mjs', [`http://${TOY_ADDR}`]);
} catch (error) {
  console.error(error);
} finally {
  for (const child of children) spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F']);
}
console.log(
  ok ? '\n✓ end to end: all passed' : '\n✗ end to end: something failed (logs in e2e/.run)',
);
process.exit(ok ? 0 : 1);
