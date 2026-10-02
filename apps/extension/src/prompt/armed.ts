/**
 * The passkey window's main button works only once the window has been focused and visible for
 * a moment (R4-5), like the browsers' own permission prompts: a click meant for the page — the
 * second click of a double-click the page asked for — can't land on _Anmelden_ or _Passkey
 * speichern_ as the window opens under it. Losing focus or visibility disarms it again.
 */

import { useEffect, useState } from 'react';

export const ARM_DELAY = 500;

/** Calls `onChange` with whether the window is armed; returns a function that stops it. */
export function watchArmed(
  win: Window,
  onChange: (armed: boolean) => void,
  delay = ARM_DELAY,
): () => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const doc = win.document;
  const stop = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  };
  const check = () => {
    stop();
    onChange(false);
    if (doc.visibilityState === 'visible' && doc.hasFocus()) {
      timer = setTimeout(() => {
        timer = null;
        onChange(true);
      }, delay);
    }
  };
  win.addEventListener('focus', check);
  win.addEventListener('blur', check);
  doc.addEventListener('visibilitychange', check);
  check();
  return () => {
    stop();
    win.removeEventListener('focus', check);
    win.removeEventListener('blur', check);
    doc.removeEventListener('visibilitychange', check);
  };
}

/** Whether the window's main button may be used now. */
export function useArmed(): boolean {
  const [armed, setArmed] = useState(false);
  useEffect(() => watchArmed(window, setArmed), []);
  return armed;
}
