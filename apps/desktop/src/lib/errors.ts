/**
 * Rust says what went wrong in English, for the log; the page says it in the
 * user's language, by kind. Security errors stay plain: no kaomoji, no Nyu.
 */

import { failure, syncNow, type Failure } from './api';
import { t } from './i18n';
import { toast } from './toast';

const RECHECK_MS = 10_000;
let lastRecheck = 0;

/**
 * The server answered that an extra is switched off, though the app still
 * showed it: an admin switched it off meanwhile. A sync asks `/uwu/v1/info`
 * again, and whatever belongs to the extra goes away. At most every ten
 * seconds, so a list of failing calls doesn't become a loop.
 */
function recheckFeatures() {
  const now = Date.now();
  if (now - lastRecheck < RECHECK_MS) return;
  lastRecheck = now;
  void syncNow().catch(() => undefined);
}

/**
 * A failure as a toast. An extra that the server switched off meanwhile is no
 * error the person made or can fix: it gets a calm note, and the app catches
 * up with the server.
 */
export function toastError(error: unknown) {
  const off = failure(error).kind === 'feature-off';
  toast(errorText(error), off ? 'info' : 'error');
}

export function errorText(error: unknown): string {
  const f: Failure = failure(error);
  const m = f.message;
  switch (f.kind) {
    case 'network':
      return t('Der Server ist nicht erreichbar: {reason}', { reason: m });
    case 'refused':
      if (m.includes('https://'))
        return t(
          'Der Server muss per https:// erreichbar sein. Unverschlüsseltes http:// geht nur zu diesem Rechner (localhost).',
        );
      if (m.includes('empty')) return t('Bitte gib die Adresse deines Servers ein.');
      if (m.includes('web address') || m.includes('host name'))
        return t('Das ist keine gültige Server-Adresse.');
      if (/incorrect|wrong|invalid/i.test(m) || m === 'Email or master password is wrong.')
        return t('E-Mail-Adresse oder Master-Passwort stimmt nicht.');
      return t('Der Server hat die Anmeldung abgelehnt: {reason}', { reason: m });
    case 'wrong-password':
      return t('Das Master-Passwort ist falsch.');
    case 'session-expired':
      return t('Die Sitzung ist abgelaufen. Bitte melde dich neu an.');
    case 'conflict':
      return t(
        'Dieser Eintrag wurde woanders geändert. UwULock hat nichts überschrieben – synchronisiere und bearbeite ihn noch einmal.',
      );
    case 'reprompt':
      return t('Dieser Eintrag fragt zuerst nach deinem Master-Passwort.');
    case 'server':
      return t('Der Server hat mit einem Fehler geantwortet: {reason}', { reason: m });
    case 'unsupported':
      return t('Das kann diese Beta noch nicht: {reason}', {
        reason: m.replace(/^not supported yet: /, ''),
      });
    case 'hello':
      if (m.includes('older UwULock'))
        return t(
          'Windows Hello ist in dieser Version sicherer eingerichtet, die alte Einrichtung gilt nicht mehr. Entsperre einmal mit dem Master-Passwort und schalte Windows Hello in den Einstellungen wieder ein.',
        );
      if (m.includes('key has changed'))
        return t(
          'Der Schlüssel von Windows Hello hat sich geändert. Entsperre mit dem Master-Passwort und schalte Windows Hello wieder ein.',
        );
      return m;
    case 'weaker-kdf':
      return t(
        'Der Server verlangt für dieses Konto eine schwächere Schlüsselableitung als bei der letzten Anmeldung, deshalb hat UwULock nichts gesendet. Wenn du sie selbst gesenkt hast, melde das Konto auf diesem Gerät ab und füge es neu hinzu.',
      );
    case 'crypto':
      return t('Etwas ließ sich nicht entschlüsseln: {reason}', { reason: m });
    case 'clipboard':
      return t('Kopieren hat nicht geklappt: {reason}', { reason: m });
    case 'locked':
      return t('Der Tresor ist gesperrt.');
    case 'not-found':
      return t('Das gibt es in diesem Eintrag nicht (mehr).');
    // UwULock Server's extras.
    case 'hidden-by-org':
      return t(
        'Die Organisation verbirgt die Passwörter dieses Eintrags vor dir. Sie können nicht in ein Send.',
      );
    case 'feature-off':
      recheckFeatures();
      return t('Das bietet dieser Server nicht (mehr) an.');
    case 'not-connected':
      return t('Dein Konto ist noch nicht mit UwUMail verbunden. Das geht im Web-Tresor.');
    case 'revoked':
      return t('UwUMail hat die Verbindung beendet. Verbinde dein Konto im Web-Tresor neu.');
    case 'quota':
      return t('Damit wäre dein Kontingent auf dem Server überschritten.');
    case 'too-large':
      return t('Das ist zu groß für den Server.');
    case 'rate-limited':
      return t('Zu viele Versuche in kurzer Zeit. Warte einen Moment.');
    case 'upstream':
      return t('Ein Dienst hinter dem Server antwortet gerade nicht: {reason}', { reason: m });
    case 'travel-active':
      return t('Das geht nicht, solange der Reisemodus an ist.');
    case 'extras-lost':
      return t(
        'Der Schlüssel für UwULocks Extras lässt sich nicht mehr öffnen – das Schlüsselpaar deines Kontos hat sich geändert. Im Web-Tresor kannst du neu anfangen.',
      );
    case 'no-key-pair':
      return t('Deinem Konto fehlt ein Schlüsselpaar. Melde dich einmal im Web-Tresor an.');
    case 'password-again':
      return t(
        'Ein neuer Link braucht das Passwort noch einmal – oder entferne es. Das Passwort hängt am Link.',
      );
    case 'new-link-needed':
      return t('Der Link dieser Anfrage lässt sich nicht mehr öffnen. Erstelle einen neuen Link.');
    case 'version-conflict':
      return t(
        'Der Eintrag wurde inzwischen woanders geändert. UwULock hat neu synchronisiert – sieh ihn dir an und stelle die Version dann noch einmal her.',
      );
    case 'not-local':
      return t('Dieser Eintrag hat keine Adresse im lokalen Netz.');
    case 'device-icon':
      return t('Das Gerät hat kein Symbol geliefert.');
    case 'invalid':
      if (m.startsWith("That doesn't look like an email"))
        return t('Das sieht nicht nach einer E-Mail-Adresse aus.');
      return m;
    default:
      return m;
  }
}
