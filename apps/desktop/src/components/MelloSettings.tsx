import { Button } from '@uwusuite/design';
import { useEffect, useState, type ReactElement, type ReactNode } from 'react';
import { copyGenerated } from '../lib/api';
import { errorText, toastError } from '../lib/errors';
import { copiedText } from '../lib/format';
import { t, useLanguage } from '../lib/i18n';
import { isMobile } from '../lib/platform';
import { getSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import { melloLink, renewMelloLink, setMelloLink, type MelloLink } from '../lib/mello';

type Row = (props: { label: string; description?: ReactNode; children: ReactNode }) => ReactElement;
type Toggle = (props: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) => ReactElement;

/**
 * The switch for Mello's read-only link (docs/mello.md) and, while it is on,
 * the address and token to pair Mello with. Desktop only.
 */
export function MelloSettings({ Row, Toggle }: { Row: Row; Toggle: Toggle }) {
  useLanguage();
  const [link, setLink] = useState<MelloLink | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isMobile()) return;
    void melloLink()
      .then(setLink)
      .catch(() => setLink(null));
  }, []);

  if (!link) return null;

  const change = (next: Promise<MelloLink>) => {
    setError(null);
    void next.then(setLink).catch((failed) => setError(errorText(failed)));
  };

  const copy = (text: string) =>
    void copyGenerated(text)
      .then(() => toast(copiedText('text', getSettings().clipboardClear)))
      .catch(toastError);

  const label = t('Mello verbinden');
  return (
    <>
      <Row
        label={label}
        description={
          <>
            {t(
              'Mello sieht dann nur, ob der Tresor gesperrt ist, wie viele Einträge er hat und wie viele Passwörter schwach oder mehrfach benutzt sind – nie Namen, Adressen, Benutzernamen oder Passwörter. Nur auf diesem Computer erreichbar.',
            )}
            {link.enabled && link.problem && (
              <>
                <br />
                <span className="form-error">{link.problem}</span>
              </>
            )}
            {error && (
              <>
                <br />
                <span className="form-error">{error}</span>
              </>
            )}
          </>
        }
      >
        <Toggle
          label={label}
          checked={link.enabled}
          onChange={(enabled) => change(setMelloLink(enabled))}
        />
      </Row>
      {link.enabled && link.token && (
        <Row
          label={t('Token für Mello')}
          description={t('In Mello unter Integrationen → UwULock eintragen. Adresse: {url}', {
            url: link.url,
          })}
        >
          <Button size="sm" onClick={() => copy(link.token ?? '')}>
            {t('Kopieren')}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => change(renewMelloLink())}>
            {t('Neuer Token')}
          </Button>
        </Row>
      )}
    </>
  );
}
