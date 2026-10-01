import { useEffect, useRef } from 'react';
import { isMobile } from './platform';

/**
 * Android's back button (and back gesture) closes the top-most open layer: a
 * dialog, the item on screen, the menu. Only on phones; a computer has Escape.
 *
 * While any layer is open, exactly one history entry marked `uwuLayer` sits on
 * top of the page. Back leaves it — Android's web view goes back in its history
 * before the app sees the button — which closes the top layer; if more layers
 * are still open, the marker comes back. When the UI closes the last layer on
 * its own, the marker is removed again, so back never needs two presses. With
 * no layer open, back puts UwULock in the background (MainActivity).
 * Changes within one tick are settled together, so a layer that closes and
 * opens again right away doesn't touch the history at all. (Taken from
 * UwUMail.)
 */

interface Layer {
  close: () => void;
}

const stack: Layer[] = [];
let leaving = false;
let scheduled = false;

const onMarker = () => (window.history.state as { uwuLayer?: boolean } | null)?.uwuLayer === true;

function settle() {
  scheduled = false;
  if (leaving) return;
  if (stack.length > 0 && !onMarker()) {
    window.history.pushState({ uwuLayer: true }, '');
  } else if (stack.length === 0 && onMarker()) {
    leaving = true;
    window.history.back();
  }
}

function schedule() {
  if (scheduled) return;
  scheduled = true;
  queueMicrotask(settle);
}

if (typeof window !== 'undefined' && isMobile()) {
  window.addEventListener('popstate', () => {
    if (leaving) {
      // Our own step back after the UI closed the last layer.
      leaving = false;
    } else if (!onMarker()) {
      // The user went back: close the top layer.
      stack.pop()?.close();
    }
    schedule();
  });
}

/** While `open`, back calls `close`. */
export function useBackLayer(open: boolean, close: () => void) {
  const latest = useRef(close);
  useEffect(() => {
    latest.current = close;
  });

  const active = open && isMobile();
  useEffect(() => {
    if (!active) return;
    const layer: Layer = { close: () => latest.current() };
    stack.push(layer);
    schedule();
    return () => {
      const index = stack.indexOf(layer);
      if (index !== -1) stack.splice(index, 1);
      schedule();
    };
  }, [active]);
}
