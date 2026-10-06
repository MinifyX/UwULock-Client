/**
 * Travel mode, as a pill in the title bar while it is on: the items in the
 * folders marked for it are gone from every device until it is switched off
 * again — in the web vault, with the second factor.
 */

import { Button, Icon, ICONS } from '@uwusuite/design';
import { useEffect, useState } from 'react';
import { toastError } from '../lib/errors';
import { t, useLanguage } from '../lib/i18n';
import { openWebVaultAt, useUwu, uwuTravel } from '../lib/uwu';
import { Modal } from './Modal';

export function TravelBadge() {
  useLanguage();
  const uwu = useUwu();
  const [open, setOpen] = useState(false);
  const enabled = uwu.travel.enabled;

  // The count comes with the sync; asking once more keeps it current.
  useEffect(() => {
    if (enabled) void uwuTravel().catch(() => undefined);
  }, [enabled]);

  if (!enabled) return null;
  const hidden = uwu.travel.hiddenCount;
  return (
    <>
      <button
        type="button"
        className="titlebar-travel"
        onClick={() => setOpen(true)}
        title={t('Reisemodus ist an')}
      >
        <Icon icon={ICONS.travelMode} size="xs" />
        {hidden === null
          ? t('Reisemodus')
          : hidden === 1
            ? t('Reisemodus · 1 verborgen')
            : t('Reisemodus · {n} verborgen', { n: hidden })}
      </button>
      {open && (
        <Modal
          title={t('Reisemodus ist an')}
          onCancel={() => setOpen(false)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setOpen(false)}>
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
            {t(
              'Ausschalten geht im Web-Tresor, mit deinem Master-Passwort und dem zweiten Faktor.',
            )}
          </p>
        </Modal>
      )}
    </>
  );
}
