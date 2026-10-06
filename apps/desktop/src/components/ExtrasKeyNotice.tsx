/**
 * The account's extras key is a different one than this device took before.
 * That happens when somebody started over in the web vault (the old key was
 * lost), or when the server lost it. Whatever was under the old key — own
 * icons, file-request links, UwUSSH and UwURDP hosts — doesn't open any more,
 * so the person hears about it once, instead of finding things missing.
 */

import { Button, Icon, ICONS } from '@uwusuite/design';
import { toastError } from '../lib/errors';
import { t, useLanguage } from '../lib/i18n';
import { extrasKeySeen, openWebVaultAt, useUwu } from '../lib/uwu';
import { Modal } from './Modal';

export function ExtrasKeyNotice() {
  useLanguage();
  const uwu = useUwu();
  if (!uwu.extrasKeyChanged) return null;
  const seen = () => void extrasKeySeen().catch((e) => toastError(e));
  return (
    <Modal
      title={t('Neuer Schlüssel für die Extras')}
      tone="warning"
      onCancel={seen}
      footer={
        <>
          <Button
            variant="ghost"
            onClick={() => void openWebVaultAt('keys').catch((e) => toastError(e))}
          >
            {t('Im Web-Tresor öffnen')}
            <Icon icon={ICONS.openExternal} size="xs" />
          </Button>
          <span className="spacer" />
          <Button variant="primary" onClick={seen}>
            {t('Verstanden')}
          </Button>
        </>
      }
    >
      <p className="dialog-lead">
        {t(
          'Der Schlüssel für UwULocks Extras ist ein anderer als beim letzten Mal auf diesem Gerät. Was unter dem alten lag – eigene Icons, Links von Dateianfragen, Hosts von UwUSSH und UwURDP – lässt sich nicht mehr öffnen.',
        )}
      </p>
      <p className="muted small">
        {t(
          'Das ist in Ordnung, wenn du (oder jemand mit deinem Master-Passwort) im Web-Tresor neu angefangen hast. Wenn nicht, hat der Server den alten Schlüssel verloren oder entfernt – sag dem Betreiber Bescheid.',
        )}
      </p>
    </Modal>
  );
}
