import { Button } from '@uwusuite/design';
import { copyGenerated } from '../lib/api';
import { autofillLogClear, useAutofillLog } from '../lib/autofill';
import { toastError } from '../lib/errors';
import { t, useLanguage } from '../lib/i18n';
import { toast } from '../lib/toast';
import { Modal } from './Modal';

/**
 * The AutoFill protocol (lib/autofill.ts): what UwULock's AutoFill extension did on this device,
 * for finding out why a sign-in didn't work — to read, copy into a bug report, or empty. Nothing
 * secret is in it, so copying it is a plain copy.
 */

/** The lines as they are, newest at the bottom; selectable, so a part can be copied too. */
export function AutofillLogText({ lines, problem }: { lines: string[]; problem?: string }) {
  useLanguage();
  if (lines.length === 0)
    return (
      <p className="autofill-log-empty">
        {problem
          ? t('Das Protokoll ließ sich nicht lesen: {problem}', { problem })
          : t('Noch keine Einträge. Fülle einmal ein Passwort oder einen Passkey mit UwULock aus.')}
      </p>
    );
  return <pre className="autofill-log">{lines.join('\n')}</pre>;
}

/** Copying and emptying, with their toasts; `cleared` reloads the lines. */
export function autofillLogActions(lines: string[], cleared: () => void) {
  return {
    copy: () =>
      void copyGenerated(lines.join('\n'))
        .then(() => toast(t('AutoFill-Protokoll kopiert')))
        .catch(toastError),
    clear: () =>
      void autofillLogClear()
        .then(() => {
          toast(t('AutoFill-Protokoll gelöscht'));
          cleared();
        })
        .catch(toastError),
  };
}

/** The desktop's dialog (the Mac builds with the extension). */
export function AutofillLogDialog({ onClose }: { onClose: () => void }) {
  useLanguage();
  const [log, reload] = useAutofillLog();
  const lines = log?.lines ?? [];
  const { copy, clear } = autofillLogActions(lines, reload);
  return (
    <Modal
      title={t('AutoFill-Protokoll')}
      size="wide"
      onCancel={onClose}
      footer={
        <>
          <Button variant="ghost" data-secondary disabled={!lines.length} onClick={clear}>
            {t('Löschen')}
          </Button>
          <Button variant="ghost" disabled={!lines.length} onClick={copy}>
            {t('Kopieren')}
          </Button>
          <Button variant="primary" onClick={onClose}>
            {t('Schließen')}
          </Button>
        </>
      }
    >
      <p className="autofill-log-note">{autofillLogNote()}</p>
      {log ? <AutofillLogText lines={lines} problem={log.problem} /> : <p>{t('Lädt …')}</p>}
    </Modal>
  );
}

export const autofillLogNote = () =>
  t(
    'Was UwULocks AutoFill-Erweiterung auf diesem Gerät getan hat: welcher Weg, welcher Schritt, welcher Fehlercode. Ohne Passwörter, Benutzernamen und vollständige Adressen – du kannst es für eine Fehlermeldung kopieren.',
  );
