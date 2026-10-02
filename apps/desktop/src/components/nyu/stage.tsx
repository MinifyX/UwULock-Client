/**
 * Nyu's little appearances, as in UwUMail: a short scene (about a second and
 * a half) in a corner of the window when something happens — an item saved,
 * a value copied, a Send shared. `playNyu()` from anywhere, `<NyuStage/>`
 * once per page shows it. They never take clicks, never move the layout, and
 * a new one replaces the one playing instead of queueing up.
 *
 * Reduced motion (the app's setting, or the system's where nothing says
 * otherwise): the frequent ones (copied, generated, unlocked) don't show at
 * all, the others show their key pose, still, for a moment.
 *
 * No words in here: the extension shows the same stage.
 */

import { useEffect, useSyncExternalStore, type ReactNode } from 'react';
import { NYU, NyuFigure, Paw, Sticker } from './Nyu';
import { Heart, Key, Shadow, Star, VaultCard } from './scenes';

export type CameoName =
  /** An item was saved: a tick is stamped on its card. */
  | 'saved'
  /** A value went to the clipboard: Nyu holds up a card, sparkles rise. */
  | 'copied'
  /** An item went into the trash: it drops into the bin, Nyu waves. */
  | 'trashed'
  /** A Send was made: a paper plane flies off, Nyu waves after it. */
  | 'shared'
  /** The vault opened: the shackle springs up. */
  | 'unlocked'
  /** The password check is done: hearts and a star. */
  | 'checked'
  /** A password was generated: the die tumbles. */
  | 'generated';

type Spec = {
  /** Milliseconds it plays with full movement. */
  duration: number;
  /** Shown still when motion is reduced (else not at all). */
  still: boolean;
  /** The same one doesn't come again before this many milliseconds. */
  cooldown: number;
};

export const CAMEOS: Record<CameoName, Spec> = {
  saved: { duration: 1500, still: true, cooldown: 0 },
  copied: { duration: 1200, still: false, cooldown: 6_000 },
  trashed: { duration: 1500, still: true, cooldown: 0 },
  shared: { duration: 1700, still: true, cooldown: 0 },
  unlocked: { duration: 1400, still: false, cooldown: 30_000 },
  checked: { duration: 1800, still: true, cooldown: 0 },
  generated: { duration: 1100, still: false, cooldown: 8_000 },
};

/** A still picture stays this long. */
export const STILL_DURATION = 1200;

export type Cameo = { name: CameoName; key: number; still: boolean; duration: number };

let current: Cameo | null = null;
let counter = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
const last = new Map<CameoName, number>();
const listeners = new Set<() => void>();

function publish(next: Cameo | null) {
  current = next;
  for (const listener of listeners) listener();
}

/** Whether animations are reduced right now: the app's data-motion, else the system's. */
export function motionReduced(root: HTMLElement = document.documentElement): boolean {
  const set = root.dataset.motion;
  if (set === 'reduced') return true;
  if (set === 'on') return false;
  return typeof window.matchMedia === 'function'
    ? window.matchMedia('(prefers-reduced-motion: reduce)').matches
    : false;
}

/**
 * Plays a cameo; answers what is playing now (null when it was skipped:
 * cooling down, or reduced motion and no still picture).
 */
export function playNyu(name: CameoName, now: number = Date.now()): Cameo | null {
  const spec = CAMEOS[name];
  const before = last.get(name);
  if (before !== undefined && now - before < spec.cooldown) return current;
  const still = motionReduced();
  if (still && !spec.still) return current;
  last.set(name, now);
  const cameo = {
    name,
    key: ++counter,
    still,
    duration: still ? STILL_DURATION : spec.duration,
  };
  clearTimeout(timer);
  publish(cameo);
  timer = setTimeout(() => publish(null), cameo.duration);
  return cameo;
}

/** For tests: forget what played. */
export function resetNyu() {
  clearTimeout(timer);
  last.clear();
  publish(null);
}

export function useCameo(): Cameo | null {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current,
  );
}

// ── The scenes ────────────────────────────────────────────

// The scenes' 320 × 220 canvas, Nyu at 0.5 scale; props with a 6 px outline.
const S = { stroke: NYU.outline, strokeWidth: 6 } as const;
const EDGE = 16;
const NYU_EDGE = 32;

function Tick({ x, y }: { x: number; y: number }) {
  return (
    <g transform={`translate(${x} ${y})`}>
      <circle r="22" fill={NYU.mint} {...S} />
      <path d="M-10 0 L-3 8 L11 -8" fill="none" {...S} strokeWidth={7} />
    </g>
  );
}

function Saved() {
  return (
    <>
      <Shadow cx={150} rx={92} />
      <NyuFigure mood="happy" x={112} y={128} scale={0.5} tilt={-4} edge={NYU_EDGE} />
      <g className="nyu-c-rise">
        <Sticker edge={EDGE}>
          <VaultCard x={226} y={128} rotate={6} />
        </Sticker>
      </g>
      <g className="nyu-c-stamp">
        <Sticker edge={EDGE}>
          <Tick x={248} y={100} />
        </Sticker>
      </g>
      <g className="nyu-c-twinkle">
        <Sticker edge={10}>
          <Star x={196} y={52} r={9} />
          <Star x={290} y={70} r={6} />
        </Sticker>
      </g>
    </>
  );
}

function Copied() {
  return (
    <>
      <Shadow cx={160} rx={80} />
      <g className="nyu-c-hop">
        <NyuFigure
          mood="cheer"
          x={160}
          y={132}
          scale={0.5}
          edge={NYU_EDGE}
          front={
            <>
              <Paw x={40} y={112} />
              <Paw x={216} y={112} />
            </>
          }
        />
      </g>
      <g className="nyu-c-float">
        <Sticker edge={10}>
          <Star x={96} y={58} r={8} />
          <Star x={226} y={46} r={10} />
          <Star x={250} y={92} r={6} />
        </Sticker>
      </g>
    </>
  );
}

function Trashed() {
  return (
    <>
      <Shadow cx={170} rx={104} />
      <NyuFigure
        mood="uwu"
        x={104}
        y={132}
        scale={0.5}
        tilt={-4}
        edge={NYU_EDGE}
        front={<Paw x={-8} y={96} className="nyu-wave" />}
      />
      <g className="nyu-c-drop-in">
        <Sticker edge={EDGE}>
          <VaultCard x={232} y={92} rotate={-14} />
        </Sticker>
      </g>
      <Sticker edge={EDGE}>
        <g transform="translate(232 166)">
          <path d="M-34 -30 H34 L27 34 H-27 Z" fill={NYU.lilac} {...S} />
          <path d="M-40 -32 H40" fill="none" {...S} strokeWidth={8} />
          <path d="M-12 -18 V22 M12 -18 V22" fill="none" {...S} strokeWidth={5} />
        </g>
      </Sticker>
    </>
  );
}

function Shared() {
  return (
    <>
      <Shadow cx={112} rx={74} />
      <path
        className="nyu-c-trail"
        d="M176 118 Q226 96 248 58 T300 12"
        fill="none"
        stroke={NYU.outline}
        strokeWidth={4}
        strokeDasharray="4 12"
        opacity="0.35"
      />
      <NyuFigure
        mood="happy"
        x={110}
        y={130}
        scale={0.5}
        tilt={-6}
        edge={NYU_EDGE}
        front={<Paw x={236} y={96} className="nyu-wave" />}
      />
      <g className="nyu-c-fly">
        <Sticker edge={EDGE}>
          <g transform="translate(206 104) rotate(-18)">
            <path d="M-30 6 L34 -18 L6 30 Z" fill={NYU.paper} {...S} strokeWidth={5} />
            <path d="M34 -18 L-2 12 L6 30" fill="none" {...S} strokeWidth={4} />
          </g>
        </Sticker>
      </g>
      <g className="nyu-c-twinkle">
        <Sticker edge={10}>
          <Star x={286} y={96} r={9} />
          <Star x={240} y={30} r={6} />
        </Sticker>
      </g>
    </>
  );
}

function Unlocked() {
  return (
    <>
      <Shadow cx={160} rx={80} />
      <g className="nyu-c-unlock">
        <NyuFigure mood="sparkle" x={160} y={132} scale={0.5} edge={NYU_EDGE} />
      </g>
      <g className="nyu-c-twinkle">
        <Sticker edge={EDGE}>
          <Key x={250} y={110} rotate={24} size={0.9} />
          <Star x={86} y={60} r={8} />
        </Sticker>
      </g>
    </>
  );
}

function Checked() {
  return (
    <>
      <Shadow cx={160} rx={90} />
      <g className="nyu-c-hop">
        <NyuFigure
          mood="cheer"
          x={160}
          y={132}
          scale={0.5}
          edge={NYU_EDGE}
          front={
            <>
              <Paw x={30} y={106} />
              <Paw x={226} y={106} />
            </>
          }
        />
      </g>
      <g className="nyu-c-float">
        <Sticker edge={10}>
          <Heart x={84} y={66} size={0.9} />
          <Heart x={240} y={56} size={1.1} fill={NYU.violet} />
          <Star x={160} y={26} r={10} />
        </Sticker>
      </g>
    </>
  );
}

function Generated() {
  return (
    <>
      <Shadow cx={160} rx={90} />
      <NyuFigure mood="happy" x={118} y={132} scale={0.5} tilt={-6} edge={NYU_EDGE} />
      <g className="nyu-c-tumble">
        <Sticker edge={EDGE}>
          <g transform="translate(236 140)">
            <rect x="-26" y="-26" width="52" height="52" rx="12" fill={NYU.paper} {...S} />
            <g className="no-edge" fill={NYU.outline}>
              <circle cx="-11" cy="-11" r="5" />
              <circle cx="0" cy="0" r="5" />
              <circle cx="11" cy="11" r="5" />
            </g>
          </g>
        </Sticker>
      </g>
    </>
  );
}

const SCENES: Record<CameoName, () => ReactNode> = {
  saved: Saved,
  copied: Copied,
  trashed: Trashed,
  shared: Shared,
  unlocked: Unlocked,
  checked: Checked,
  generated: Generated,
};

/** Where Nyu's cameos appear; once per page, anywhere in it (it is fixed). */
export function NyuStage() {
  const cameo = useCameo();
  // Gone with the page: nothing keeps playing after it unmounts.
  useEffect(() => () => clearTimeout(timer), []);
  if (!cameo) return null;
  const Scene = SCENES[cameo.name];
  return (
    <div
      key={cameo.key}
      className={cameo.still ? 'nyu-stage nyu-stage-still' : 'nyu-stage'}
      data-cameo={cameo.name}
      style={{ ['--nyu-cameo-ms' as string]: `${cameo.duration}ms` }}
      aria-hidden
    >
      <svg
        viewBox="-10 -10 340 230"
        className="nyu-host"
        style={{ overflow: 'visible' }}
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <Scene />
      </svg>
    </div>
  );
}

/**
 * Waiting for something ("Einen Moment …"): Nyu bobs, three dots blink.
 * Still when motion is reduced. `label` is read out; the extension passes
 * its own words.
 */
export function NyuBusy({ label, size = 40 }: { label: string; size?: number }) {
  return (
    <div className="nyu-busy" role="status">
      <svg
        viewBox="0 0 256 256"
        width={size}
        height={size}
        className="nyu-host nyu-blink"
        style={{ overflow: 'visible' }}
        aria-hidden
      >
        <g className="nyu-busy-figure">
          <NyuFigure mood="happy" />
        </g>
      </svg>
      <span className="nyu-busy-label">
        {label}
        <span className="nyu-busy-dots" aria-hidden>
          <i />
          <i />
          <i />
        </span>
      </span>
    </div>
  );
}
