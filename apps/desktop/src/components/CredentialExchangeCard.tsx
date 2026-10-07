/**
 * iOS 26: credentials Apple Passwords handed over ("Export data to another app" → UwULock) wait in
 * the plugin (CredentialExchange.swift). Once the vault is open, Nyu asks: "Ansehen" takes them
 * from the system (only now, only once), reads them (lib/import/cxf.ts) and opens the import with
 * them, which shows everything before anything is saved; "Verwerfen" drops them. On iPhone and
 * iPad that is Settings → Importieren (lib/credentialExchange.ts), elsewhere the import dialog.
 */

import { Button } from '@uwusuite/design';
import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { t, useLanguage } from '../lib/i18n';
import { platform } from '../lib/platform';
import { handOver } from '../lib/credentialExchange';
import { readCredentialExchange } from '../lib/import/cxf';
import { importErrorText, parsedFromCollected } from '../lib/import/run';
import type { Parsed } from '../lib/import/types';
import { ImportDialog } from './ImportDialog';
import { Nyu } from './nyu/Nyu';

export function CredentialExchangeCard({
  unlocked,
  mobile = false,
}: {
  unlocked: boolean;
  /** The phone and iPad layout: the import opens as its settings page. */
  mobile?: boolean;
}) {
  useLanguage();
  const [pending, setPending] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [parsed, setParsed] = useState<Parsed | null>(null);

  useEffect(() => {
    if (!unlocked || platform() !== 'ios') return;
    let alive = true;
    const check = () => {
      if (document.visibilityState !== 'visible') return;
      invoke<boolean>('credential_exchange_pending')
        .then((waiting) => alive && setPending(waiting))
        .catch(() => {});
    };
    check();
    document.addEventListener('visibilitychange', check);
    return () => {
      alive = false;
      document.removeEventListener('visibilitychange', check);
    };
  }, [unlocked]);

  if (!unlocked) return null;
  if (parsed) return <ImportDialog initial={parsed} onClose={() => setParsed(null)} />;
  if (!pending && !error) return null;

  const take = async (discard: boolean) => {
    setBusy(true);
    setError(null);
    try {
      const json = await invoke<string>('credential_exchange_import', { discard });
      setPending(false);
      if (discard) return;
      const { data, warnings, format } = readCredentialExchange(json);
      const parsed = parsedFromCollected(data, { source: 'credential-exchange', format, warnings });
      if (mobile) handOver(parsed);
      else setParsed(parsed);
    } catch (e) {
      setPending(false);
      setError(importErrorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <aside className="update-hint autofill-card exchange-card" aria-live="polite">
      <div className="update-hint-head">
        <Nyu size={40} mood="sparkle" title="Nyu" />
        <div>
          <p className="update-hint-title">{t('Daten aus Apple Passwörter übernehmen?')}</p>
          <p className="update-hint-meta">
            {t('Du siehst alle Einträge, bevor etwas in deinem Tresor gespeichert wird.')}
          </p>
        </div>
      </div>
      {error && (
        <p className="update-hint-warning" role="alert">
          {error}
        </p>
      )}
      <div className="update-hint-actions">
        {error ? (
          <Button variant="primary" size="sm" onClick={() => setError(null)}>
            {t('Schließen')}
          </Button>
        ) : (
          <>
            <Button variant="ghost" size="sm" disabled={busy} onClick={() => void take(true)}>
              {t('Verwerfen')}
            </Button>
            <Button variant="primary" size="sm" busy={busy} onClick={() => void take(false)}>
              {t('Ansehen')}
            </Button>
          </>
        )}
      </div>
    </aside>
  );
}
