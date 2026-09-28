/**
 * Failures of UwULock Server's own API in words. The background hands them on as the kind
 * `uwu:<code>`, with the contract's code (`not_connected`, `quota` …); the popup and the
 * inline menu say here what each means.
 */

import { t } from './i18n';

/** What went wrong, or null when it wasn't UwULock Server's own API. */
export function uwuErrorText(kind: string, message: string): string | null {
  if (!kind.startsWith('uwu:')) return null;
  switch (kind.slice(4)) {
    case 'not_connected':
      return t(
        'Dein Konto ist noch nicht mit UwUMail verbunden. Verbinde es im Web-Tresor unter Einstellungen → Maskierte Adressen.',
      );
    case 'revoked':
      return t(
        'Die Verbindung zu UwUMail wurde beendet. Verbinde dein Konto im Web-Tresor noch einmal.',
      );
    case 'upstream':
      return t('UwUMail antwortet gerade nicht. Versuche es gleich noch einmal.');
    case 'quota':
      return t('Ein Limit ist erreicht, mehr geht gerade nicht.');
    case 'rate_limited':
      return t(
        'Zu viele Anfragen auf einmal. Warte einen Moment und versuche es dann noch einmal.',
      );
    case 'feature_off':
      return t('Dein Server bietet das gerade nicht an.');
    case 'no_extras_key':
      return t(
        'Dein Konto hat noch keinen Schlüssel für UwULocks Extras. Öffne einmal den Web-Tresor oder UwULock am Rechner.',
      );
    case 'forbidden':
      return t('Das ist nicht erlaubt.');
    case 'invalid':
      return t('Der Server hat die Anfrage abgelehnt: {reason}', { reason: message });
    default:
      return t('Der Server hat mit einem Fehler geantwortet: {reason}', { reason: message });
  }
}
