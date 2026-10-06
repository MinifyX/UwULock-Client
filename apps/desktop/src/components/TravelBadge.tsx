/**
 * Travel mode, as a pill in the title bar while it is on: the items in the
 * folders marked for it are gone from every device until it is switched off
 * again — in the web vault, with the second factor. macOS has no title bar of
 * the app's own: there the window's title says it, and the menu bar opens the
 * dialog (App.tsx).
 */

import { Button, Icon, ICONS } from '@uwusuite/design';
import { useEffect, useState } from 'react';
import { toastError } from '../lib/errors';
import { t, useLanguage } from '../lib/i18n';
import { openWebVaultAt, useUwu, uwuTravel } from '../lib/uwu';
import { Modal } from './Modal';

/** Whether travel mode is on, and how many items it hides (null while unknown). */
export function useTravel(): { enabled: boolean; hidden: number | null } {
  const uwu = useUwu();
  const enabled = uwu.travel.enabled;
  // The count comes with the sync; asking once more keeps it current.
  useEffect(() => {
    if (enabled) void uwuTravel().catch(() => undefined);
  }, [enabled]);
  return { enabled, hidden: uwu.travel.hiddenCount };
}

/** The pill's words: "Reisemodus · 3 verborgen". */
export function travelLabel(hidden: number | null): string {
  return hidden === null
    ? t('Reisemodus')
    : hidden === 1
      ? t('Reisemodus · 1 verborgen')
      : t('Reisemodus · {n} verborgen', { n: hidden });
}

/** The pill in the title bar (Windows, Linux) or the app bar (phones). */
export function TravelBadge() {
  useLanguage();
  const { enabled, hidden } = useTravel();
  const [open, setOpen] = useState(false);
  if (!enabled) return null;
  return (
    <>
      <button
        type="button"
        className="titlebar-travel"
        onClick={() => setOpen(true)}
        title={t('Reisemodus ist an')}
      >
        <Icon icon={ICONS.travelMode} size="xs" />
        {travelLabel(hidden)}
      </button>
      {open && <TravelDialog hidden={hidden} onClose={() => setOpen(false)} />}
    </>
  );
}

/** What travel mode does, and where it is switched off. On macOS it opens from the menu bar. */
export function TravelDialog({ hidden, onClose }: { hidden: number | null; onClose: () => void }) {
  useLanguage();
  return (
    <Modal
      title={t('Reisemodus ist an')}
      onCancel={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('Schließen')}
          </Button>
          <span className="spacer" />
          <Button
            variant="primary"
            onClick={() => void openWebVaultAt('travel').catch((e) => toastError(e))}
          >
            {t('Im Web-Tresor ausschalten')}
            <Icon icon={ICONS.openExternal} size="xs" />
          </Button>
        </>
      }
    >
      <p className="dialog-lead">
        {hidden
          ? t(
              '{n} Einträge aus den Ordnern, die du fürs Reisen markiert hast, sind auf allen Geräten ausgeblendet – auch hier. Sie sind nicht weg: Nach dem Ausschalten kommen sie mit der nächsten Synchronisierung zurück.',
              { n: hidden },
            )
          : t(
              'Die Einträge aus den Ordnern, die du fürs Reisen markiert hast, sind auf allen Geräten ausgeblendet – auch hier. Nach dem Ausschalten kommen sie mit der nächsten Synchronisierung zurück.',
            )}
      </p>
      <p className="muted small">
        {t('Ausschalten geht im Web-Tresor, mit deinem Master-Passwort und dem zweiten Faktor.')}
      </p>
    </Modal>
  );
}
