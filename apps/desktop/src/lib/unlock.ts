/**
 * Unlocking without the master password: Windows Hello on a computer, a
 * fingerprint or face on a phone (`helloKind` in the status). The words for
 * each, and for the phone's own dialog.
 */

import type { UnlockPrompt } from './api';
import { t } from './i18n';

/** "Mit Windows Hello entsperren", "Mit Fingerabdruck entsperren", … */
export function unlockLabel(kind: string | null): string {
  switch (kind) {
    case 'windowsHello':
      return t('Mit Windows Hello entsperren');
    case 'fingerprint':
      return t('Mit Fingerabdruck entsperren');
    case 'face':
      return t('Mit Gesichtserkennung entsperren');
    case 'faceId':
      return t('Mit Face ID entsperren');
    case 'touchId':
      return t('Mit Touch ID entsperren');
    case 'opticId':
      return t('Mit Optic ID entsperren');
    default:
      return t('Mit Fingerabdruck oder Gesicht entsperren');
  }
}

/** What the setting says it does. */
export function unlockDescription(kind: string | null): string {
  if (kind === 'windowsHello')
    return t(
      'Gesicht, Finger oder PIN statt des Master-Passworts. Nach einem Neustart von UwULock geht das auch.',
    );
  return t(
    'Statt des Master-Passworts. Der Schlüssel dafür liegt geschützt auf diesem Gerät und gilt nicht mehr, sobald ein Finger oder Gesicht dazukommt.',
  );
}

/** The phone's biometric dialog. */
export function unlockPrompt(): UnlockPrompt {
  return {
    title: t('UwULock entsperren'),
    subtitle: t('Bestätige, dass du es bist.'),
    cancel: t('Master-Passwort'),
  };
}
