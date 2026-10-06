/**
 * Shortcuts are written once, as Tauri accelerators (`CmdOrCtrl+L`), and shown
 * the platform's way: `⌘L` on a Mac, `Strg+L` or `Ctrl+L` elsewhere. The same
 * accelerators go into the macOS menu bar (App.tsx), so menu and tooltip agree.
 */

import { detectPlatform, shortcutText, withShortcut } from '@uwusuite/design';
import { language } from './i18n';

/** The desktop platform for chrome decisions: title bar or menu bar. */
export const desktop = detectPlatform();

/** `CmdOrCtrl+L` as this platform writes it. */
export function keys(accelerator: string): string {
  return shortcutText(accelerator, desktop, language());
}

/** A tooltip with its shortcut: `Sperren (⌘L)`. */
export function withKeys(label: string, accelerator: string): string {
  return withShortcut(label, accelerator, desktop, language());
}
