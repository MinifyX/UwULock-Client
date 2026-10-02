/**
 * Whether a passkey request comes right after the person did something in the frame (R4-5).
 * Read in the content script's isolated world: the page can't fake `navigator.userActivation`
 * there.
 */

/** The person just did something in this frame (transient user activation). */
export function activated(): boolean {
  const activation = (navigator as Navigator & { userActivation?: { isActive?: boolean } })
    .userActivation;
  return activation?.isActive === true;
}

/** Whether a request needs the person's activation: everything but conditional mediation. */
export function needsActivation(kind: 'create' | 'get', options: object): boolean {
  return kind === 'create' || (options as { mediation?: unknown }).mediation !== 'conditional';
}
