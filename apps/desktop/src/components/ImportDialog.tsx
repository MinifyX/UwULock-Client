import { Button, SettingRow } from '@uwusuite/design';
import { useRef, useState, type FormEvent } from 'react';
import { N_, t, useLanguage } from '../lib/i18n';
import { SOURCES, sourceLabel, summarize, type Parsed, type Source } from '../lib/import/index.ts';
import { checkFileSize } from '../lib/import/limits.ts';
import { importErrorText, skippedText, type ImportOutcome } from '../lib/import/run.ts';
import { ItemType } from '../lib/import/types.ts';
import { IMPORT_ACCEPT, useImportFlow, type ImportFlow } from '../lib/importFlow';
import { Modal } from './Modal';
import { PasswordInput } from './PasswordInput';

/** The preview lists this many items; more would only slow the dialog down. */
const LIST_LIMIT = 500;

export const TYPE_LABELS: Record<ItemType, string> = {
  [ItemType.Login]: N_('Login'),
  [ItemType.Note]: N_('Notiz'),
  [ItemType.Card]: N_('Karte'),
  [ItemType.Identity]: N_('Identität'),
  [ItemType.SshKey]: N_('Schlüssel (SSH)'),
};

/** What the preview counts, by kind; kinds with none left out. */
export function importCounts(parsed: Parsed): [number, string][] {
  const summary = summarize(parsed);
  return (
    [
      [summary.logins, t('Logins')],
      [summary.notes, t('Notizen')],
      [summary.cards, t('Karten')],
      [summary.identities, t('Identitäten')],
      [summary.sshKeys, t('SSH-Schlüssel')],
      [summary.wifi, t('WLANs')],
    ] as [number, string][]
  ).filter(([n]) => n > 0);
}

/** Items with passkeys, from Bitwarden's JSON or the system's hand-over. */
export const passkeyCount = (parsed: Parsed) =>
  parsed.data.items.reduce((n, item) => n + (item.login?.fido2Credentials?.length ?? 0), 0);

/** The line under a finished import. */
export function outcomeText(outcome: ImportOutcome): string {
  const n = outcome.imported;
  if (outcome.error)
    return n === 1
      ? t('Ein Eintrag importiert, dann ist der Import stehen geblieben.')
      : t('{n} Einträge importiert, dann ist der Import stehen geblieben.', { n });
  return n === 1 ? t('Ein Eintrag importiert ✧') : t('{n} Einträge importiert ✧', { n });
}

/** Above the items that stayed out. */
export const skippedHeader = (n: number) =>
  n === 1
    ? t('Ein Eintrag ist draußen geblieben:')
    : t('{n} Einträge sind draußen geblieben:', { n });

/**
 * Moving in from another password manager's export: Bitwarden (also
 * password-protected), KeePass, 1Password, Chrome, Firefox, Apple Passwörter,
 * Proton Pass, LastPass. The file is read on this device; Rust seals what is
 * in it before anything goes to the server. `initial`: items handed over by
 * the system (Credential Exchange), shown in the same preview.
 */
export function ImportDialog({ onClose, initial }: { onClose: () => void; initial?: Parsed }) {
  useLanguage();
  const flow = useImportFlow(initial);
  const { step } = flow;
  const running = step.name === 'running';
  const close = () => {
    if (!running) onClose();
  };

  return (
    <Modal
      title={t('Importieren')}
      onCancel={close}
      footer={
        step.name === 'preview' ? (
          <>
            <Button variant="ghost" onClick={flow.reset} data-secondary>
              {t('Andere Datei')}
            </Button>
            <span className="spacer" />
            <Button
              variant="primary"
              onClick={() => void flow.start()}
              disabled={summarize(step.parsed).items.length === 0}
            >
              {t('{n} Einträge importieren', { n: summarize(step.parsed).items.length })}
            </Button>
          </>
        ) : undefined
      }
    >
      {(step.name === 'pick' || step.name === 'reading') && (
        <PickView flow={flow} onCancel={close} />
      )}
      {step.name === 'password' && (
        <PasswordView flow={flow} name={step.file.name} keepass={step.kind === 'keepass'} />
      )}
      {step.name === 'preview' && <PreviewView flow={flow} parsed={step.parsed} />}
      {step.name === 'running' && (
        <div className="form" aria-live="polite">
          <p className="dialog-lead">
            {step.progress && step.progress.total > 0
              ? t('{done} von {total} Einträgen im Tresor', {
                  done: step.progress.done,
                  total: step.progress.total,
                })
              : t('Verschlüsselt die Einträge …')}
          </p>
          <progress
            className="move-progress"
            max={step.progress?.total || 1}
            value={step.progress?.done ?? 0}
          />
          <p className="field-hint">
            {t(
              'Jeder Eintrag wird hier auf diesem Gerät verschlüsselt, bevor er zum Server geht. Lass UwULock so lange offen.',
            )}
          </p>
        </div>
      )}
      {step.name === 'done' && (
        <DoneView outcome={step.outcome} onAgain={flow.reset} onClose={onClose} />
      )}
    </Modal>
  );
}

function PickView({ flow, onCancel }: { flow: ImportFlow; onCancel: () => void }) {
  useLanguage();
  const input = useRef<HTMLInputElement>(null);
  const reading = flow.step.name === 'reading';
  return (
    <div className="form">
      <p className="dialog-lead">
        {t(
          'Aus Bitwarden, Vaultwarden, UwULock, KeePass, KeePassXC, 1Password, Chrome, Edge, Firefox, Apple Passwörter, Proton Pass oder LastPass. Die Datei wird nur hier auf diesem Gerät gelesen, und du siehst vorher, was kommt.',
        )}
      </p>
      <label className="field">
        <span>{t('Importieren aus')}</span>
        <select
          value={flow.source}
          onChange={(e) => flow.setSource(e.target.value as Source | 'auto')}
          disabled={reading}
        >
          <option value="auto">{t('Automatisch erkennen')}</option>
          {SOURCES.map((option) => (
            <option key={option.value} value={option.value}>
              {t(option.label)}
            </option>
          ))}
        </select>
      </label>
      <input
        ref={input}
        type="file"
        accept={IMPORT_ACCEPT}
        hidden
        onChange={(e) => {
          const file = e.target.files?.[0];
          e.target.value = '';
          void flow.pick(file);
        }}
      />
      {flow.error && (
        <p className="form-error" role="alert">
          {flow.error}
        </p>
      )}
      <p className="field-hint">
        {t(
          'Anhänge kommen nicht mit: Lade sie danach im Web-Tresor hoch. Aus Apple Passwörter: Exportiere in den Einstellungen als CSV-Datei.',
        )}
      </p>
      <div className="form-actions">
        <Button variant="ghost" onClick={onCancel}>
          {t('Abbrechen')}
        </Button>
        <span className="spacer" />
        <Button variant="primary" onClick={() => input.current?.click()} disabled={reading}>
          {reading ? t('Liest …') : t('Datei wählen …')}
        </Button>
      </div>
    </div>
  );
}

/** A KeePass file's password and, if it has one, its key file; or a Bitwarden export's password. */
function PasswordView({
  flow,
  name,
  keepass,
}: {
  flow: ImportFlow;
  name: string;
  keepass: boolean;
}) {
  useLanguage();
  const [password, setPassword] = useState('');
  const [keyFile, setKeyFile] = useState<{ name: string; bytes: Uint8Array } | null>(null);
  const [keyError, setKeyError] = useState<string | null>(null);
  const keyInput = useRef<HTMLInputElement>(null);
  const ready = Boolean(password || keyFile);

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    if (ready && !flow.busy) void flow.unlockFile(password, keyFile?.bytes ?? null);
  };

  const pickKey = async (chosen: File | undefined) => {
    if (!chosen) return;
    try {
      checkFileSize(chosen.size);
      setKeyError(null);
      setKeyFile({ name: chosen.name, bytes: new Uint8Array(await chosen.arrayBuffer()) });
    } catch (e) {
      setKeyError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <form className="form" onSubmit={submit}>
      <p className="dialog-lead">
        {keepass
          ? t(
              '„{name}“ ist mit einem Passwort geschützt. UwULock öffnet die Datei hier auf diesem Gerät; das Passwort geht nirgendwohin.',
              { name },
            )
          : t(
              '„{name}“ ist ein passwortgeschützter Export von Bitwarden. Gib das Passwort ein, das beim Exportieren gewählt wurde.',
              { name },
            )}
      </p>
      <label className="field">
        <span>{t('Passwort der Datei')}</span>
        <PasswordInput
          value={password}
          onChange={setPassword}
          autoFocus
          disabled={flow.busy}
          autoComplete="off"
        />
      </label>
      {keepass && (
        <div className="field">
          <span id="import-key-file">{t('Schlüsseldatei (wenn die Datei eine hat)')}</span>
          <div className="import-key-file" aria-labelledby="import-key-file" role="group">
            <input
              ref={keyInput}
              type="file"
              hidden
              onChange={(e) => {
                const file = e.target.files?.[0];
                e.target.value = '';
                void pickKey(file);
              }}
            />
            <Button
              size="sm"
              type="button"
              onClick={() => keyInput.current?.click()}
              disabled={flow.busy}
            >
              {keyFile ? t('Andere wählen …') : t('Schlüsseldatei wählen …')}
            </Button>
            {keyFile && (
              <>
                <span className="import-key-name">{keyFile.name}</span>
                <Button
                  size="sm"
                  variant="ghost"
                  type="button"
                  onClick={() => setKeyFile(null)}
                  disabled={flow.busy}
                  aria-label={t('Schlüsseldatei entfernen')}
                  data-secondary
                >
                  {t('Entfernen')}
                </Button>
              </>
            )}
          </div>
        </div>
      )}
      {(flow.error || keyError) && (
        <p className="form-error" role="alert">
          {flow.error ?? keyError}
        </p>
      )}
      <div className="form-actions">
        <Button variant="ghost" type="button" onClick={flow.reset} disabled={flow.busy}>
          {t('Zurück')}
        </Button>
        <span className="spacer" />
        <Button variant="primary" type="submit" disabled={flow.busy || !ready}>
          {flow.busy ? t('Öffnet …') : t('Öffnen')}
        </Button>
      </div>
    </form>
  );
}

/** What the file holds, before any of it goes to the vault. */
function PreviewView({ flow, parsed }: { flow: ImportFlow; parsed: Parsed }) {
  useLanguage();
  const summary = summarize(parsed);
  const total = summary.items.length;
  const counts = importCounts(parsed);
  const passkeys = passkeyCount(parsed);
  return (
    <div className="form">
      <p className="dialog-lead">
        {t('{app}, {format}: {n} Einträge.', {
          app: sourceLabel(parsed.source),
          format: parsed.format,
          n: total,
        })}{' '}
        {t('Noch ist nichts im Tresor; das passiert erst mit „Importieren“.')}
      </p>
      {(counts.length > 0 || passkeys > 0) && (
        <ul className="import-counts" aria-label={t('Einträge nach Art')}>
          {counts.map(([n, label]) => (
            <li key={label}>
              <strong>{n}</strong> {label}
            </li>
          ))}
          {passkeys > 0 && (
            <li>
              <strong>{passkeys}</strong> {t('Passkeys')}
            </li>
          )}
        </ul>
      )}
      {summary.folders.length > 0 && (
        <p className="field-hint">{t('Ordner: {names}', { names: summary.folders.join(', ') })}</p>
      )}
      {parsed.warnings.length > 0 && (
        <ul className="move-notices" aria-label={t('Hinweise')}>
          {parsed.warnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      )}
      {total > 0 && (
        <ul className="import-list" tabIndex={0} aria-label={t('Einträge')}>
          {summary.items.slice(0, LIST_LIMIT).map((item, i) => (
            <li key={i}>
              <span className="import-item-name">{item.name}</span>
              <span className="import-item-detail">
                {[
                  item.wifi ? t('WLAN') : t(TYPE_LABELS[item.type]),
                  item.detail,
                  item.folder ?? t('Ohne Ordner'),
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
              {item.totp && (
                <span className="import-chip" title={t('Mit Einmalcodes (TOTP)')}>
                  TOTP
                </span>
              )}
            </li>
          ))}
          {total > LIST_LIMIT && (
            <li className="import-item-more">
              {t('… und {n} weitere', { n: total - LIST_LIMIT })}
            </li>
          )}
        </ul>
      )}
      {flow.error && (
        <p className="form-error" role="alert">
          {flow.error}
        </p>
      )}
    </div>
  );
}

function DoneView({
  outcome,
  onAgain,
  onClose,
}: {
  outcome: ImportOutcome;
  onAgain: () => void;
  onClose: () => void;
}) {
  useLanguage();
  return (
    <div className="form" role="status">
      <p className="dialog-lead">{outcomeText(outcome)}</p>
      {outcome.foldersCreated > 0 && (
        <p className="field-hint">
          {outcome.foldersCreated === 1
            ? t('Ein neuer Ordner angelegt.')
            : t('{n} neue Ordner angelegt.', { n: outcome.foldersCreated })}
        </p>
      )}
      {outcome.error && (
        <p className="form-error" role="alert">
          {importOutcomeError(outcome)}
        </p>
      )}
      {outcome.skipped.length > 0 && (
        <>
          <p className="field-hint">{skippedHeader(outcome.skipped.length)}</p>
          <ul className="move-notices">
            {outcome.skipped.slice(0, 50).map((item, i) => (
              <li key={i}>
                <strong>{item.name}</strong> – {skippedText(item.reason)}
              </li>
            ))}
          </ul>
        </>
      )}
      <div className="form-actions">
        <Button variant="ghost" onClick={onAgain}>
          {t('Noch eine Datei')}
        </Button>
        <span className="spacer" />
        <Button variant="primary" onClick={onClose}>
          {t('Fertig')}
        </Button>
      </div>
    </div>
  );
}

/** Why an import stopped part way, and that what came in stays. */
export function importOutcomeError(outcome: ImportOutcome): string {
  return t('{reason} Was schon im Tresor ist, bleibt dort; importiere den Rest nicht doppelt.', {
    reason: outcome.error ? importErrorText(outcome.error) : '',
  });
}

/** The row in Settings → Konto that opens the import. */
export function ImportSetting() {
  useLanguage();
  const [open, setOpen] = useState(false);
  return (
    <>
      <SettingRow
        label={t('Importieren')}
        description={t(
          'Einträge aus der Export-Datei eines anderen Passwort-Managers holen: Bitwarden, KeePass, 1Password, Chrome, Firefox, Apple Passwörter, Proton Pass, LastPass.',
        )}
      >
        <Button size="sm" onClick={() => setOpen(true)}>
          {t('Importieren …')}
        </Button>
      </SettingRow>
      {open && <ImportDialog onClose={() => setOpen(false)} />}
    </>
  );
}
