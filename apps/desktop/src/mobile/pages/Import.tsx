/**
 * Settings → Importieren on a phone and an iPad: the desktop's import dialog
 * (components/ImportDialog.tsx) as a page of grouped lists — the app to read
 * from, the file, a password where the file has one, the preview, the
 * progress, what came in. The steps are the same (lib/importFlow.ts).
 */

import { ICONS, ListRow, ListSection } from '@uwusuite/design';
import { useEffect, useRef, useState } from 'react';
import {
  importCounts,
  importOutcomeError,
  outcomeText,
  skippedHeader,
  passkeyCount,
  TYPE_LABELS,
} from '../../components/ImportDialog';
import { forget, HANDED_OVER, handedOver } from '../../lib/credentialExchange';
import { t, useLanguage } from '../../lib/i18n';
import { SOURCES, sourceLabel, summarize, type Parsed, type Source } from '../../lib/import';
import { checkFileSize } from '../../lib/import/limits';
import { skippedText } from '../../lib/import/run';
import { useImportFlow, type ImportFlow } from '../../lib/importFlow';
import { useNav } from '../state';
import { BigButton, ChoiceSheet, FieldInput, Page } from '../ui';

/** A phone lists fewer items in the preview than the desktop. */
const LIST_LIMIT = 100;

/**
 * Settings → Importieren. What Apple Passwords handed over (iOS 26, lib/credentialExchange.ts)
 * starts the page at its preview; a new hand-over while the page is open starts it over.
 */
export function SettingsImportPage() {
  const [waiting, setWaiting] = useState(handedOver);
  useEffect(() => {
    const update = () => setWaiting(handedOver());
    window.addEventListener(HANDED_OVER, update);
    return () => window.removeEventListener(HANDED_OVER, update);
  }, []);
  useEffect(() => {
    if (waiting) forget(waiting.generation);
  }, [waiting]);
  return <ImportPage key={waiting?.generation ?? 0} initial={waiting?.parsed} />;
}

export function ImportPage({ initial }: { initial?: Parsed }) {
  useLanguage();
  const flow = useImportFlow(initial);
  const nav = useNav();
  const { step } = flow;

  return (
    <Page title={t('Importieren')} largeTitle>
      {(step.name === 'pick' || step.name === 'reading') && <PickStep flow={flow} />}
      {step.name === 'password' && (
        <PasswordStep flow={flow} name={step.file.name} keepass={step.kind === 'keepass'} />
      )}
      {step.name === 'preview' && <PreviewStep flow={flow} parsed={step.parsed} />}
      {step.name === 'running' && (
        <ListSection
          footer={t(
            'Jeder Eintrag wird hier auf diesem Gerät verschlüsselt, bevor er zum Server geht. Lass UwULock so lange offen.',
          )}
        >
          <div className="m-import-progress" aria-live="polite">
            <span>
              {step.progress && step.progress.total > 0
                ? t('{done} von {total} Einträgen im Tresor', {
                    done: step.progress.done,
                    total: step.progress.total,
                  })
                : t('Verschlüsselt die Einträge …')}
            </span>
            <progress
              className="move-progress"
              max={step.progress?.total || 1}
              value={step.progress?.done ?? 0}
            />
          </div>
        </ListSection>
      )}
      {step.name === 'done' && (
        <>
          <ListSection>
            <ListRow
              icon={step.outcome.error ? ICONS.warning : ICONS.done}
              iconTone={step.outcome.error ? 'warning' : 'success'}
              title={outcomeText(step.outcome)}
              wrap
            />
            {step.outcome.foldersCreated > 0 && (
              <ListRow
                icon={ICONS.folder}
                iconTone="neutral"
                title={t('Neue Ordner')}
                value={step.outcome.foldersCreated}
              />
            )}
          </ListSection>
          {step.outcome.error && (
            <p className="m-error" role="alert">
              {importOutcomeError(step.outcome)}
            </p>
          )}
          {step.outcome.skipped.length > 0 && (
            <ListSection header={skippedHeader(step.outcome.skipped.length)}>
              {step.outcome.skipped.slice(0, 50).map((item, i) => (
                <ListRow key={i} label={item.name} title={skippedText(item.reason)} wrap />
              ))}
            </ListSection>
          )}
          <div className="m-buttons">
            <BigButton icon={ICONS.done} onClick={() => (nav.canBack ? nav.back() : flow.reset())}>
              {t('Fertig')}
            </BigButton>
          </div>
          <ListSection>
            <ListRow
              icon={ICONS.import}
              iconTone="neutral"
              title={t('Noch eine Datei')}
              onClick={flow.reset}
            />
          </ListSection>
        </>
      )}
    </Page>
  );
}

function PickStep({ flow }: { flow: ImportFlow }) {
  useLanguage();
  const input = useRef<HTMLInputElement>(null);
  const [choosing, setChoosing] = useState(false);
  const reading = flow.step.name === 'reading';
  const options: { value: Source | 'auto'; label: string }[] = [
    { value: 'auto', label: t('Automatisch erkennen') },
    ...SOURCES.map((option) => ({ value: option.value, label: t(option.label) })),
  ];
  return (
    <>
      <ListSection
        footer={t(
          'Aus Bitwarden, Vaultwarden, UwULock, KeePass, KeePassXC, 1Password, Chrome, Edge, Firefox, Apple Passwörter, Proton Pass oder LastPass. Die Datei wird nur hier auf diesem Gerät gelesen, und du siehst vorher, was kommt.',
        )}
      >
        <ListRow
          icon={ICONS.import}
          iconTone="neutral"
          title={t('Aus')}
          value={options.find((option) => option.value === flow.source)?.label}
          disabled={reading}
          onClick={() => setChoosing(true)}
        />
        <input
          ref={input}
          type="file"
          hidden
          onChange={(event) => {
            const file = event.target.files?.[0];
            event.target.value = '';
            void flow.pick(file);
          }}
        />
        <ListRow
          icon={ICONS.file}
          iconTone="solid"
          title={reading ? t('Liest …') : t('Datei wählen …')}
          disabled={reading}
          onClick={() => input.current?.click()}
        />
      </ListSection>
      {flow.error && (
        <p className="m-error" role="alert">
          {flow.error}
        </p>
      )}
      <p className="m-footnote">
        {t(
          'Anhänge kommen nicht mit: Lade sie danach im Web-Tresor hoch. Aus Apple Passwörter: Exportiere in den Einstellungen als CSV-Datei.',
        )}
      </p>
      <ChoiceSheet
        open={choosing}
        onClose={() => setChoosing(false)}
        title={t('Importieren aus')}
        options={options}
        value={flow.source}
        onChange={flow.setSource}
      />
    </>
  );
}

function PasswordStep({
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
  const submit = () => {
    if (ready && !flow.busy) void flow.unlockFile(password, keyFile?.bytes ?? null);
  };
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <ListSection
        footer={
          keepass
            ? t(
                '„{name}“ ist mit einem Passwort geschützt. UwULock öffnet die Datei hier auf diesem Gerät; das Passwort geht nirgendwohin.',
                { name },
              )
            : t(
                '„{name}“ ist ein passwortgeschützter Export von Bitwarden. Gib das Passwort ein, das beim Exportieren gewählt wurde.',
                { name },
              )
        }
      >
        <FieldInput
          label={t('Passwort der Datei')}
          type="password"
          value={password}
          onChange={setPassword}
          autoFocus
          disabled={flow.busy}
        />
      </ListSection>
      {keepass && (
        <ListSection footer={t('Schlüsseldatei (wenn die Datei eine hat)')}>
          <input
            ref={keyInput}
            type="file"
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0];
              event.target.value = '';
              if (!file) return;
              try {
                checkFileSize(file.size);
                setKeyError(null);
                void file
                  .arrayBuffer()
                  .then((buffer) => setKeyFile({ name: file.name, bytes: new Uint8Array(buffer) }));
              } catch (e) {
                setKeyError(e instanceof Error ? e.message : String(e));
              }
            }}
          />
          <ListRow
            icon={ICONS.securityKey}
            iconTone="neutral"
            title={keyFile ? keyFile.name : t('Schlüsseldatei wählen …')}
            disabled={flow.busy}
            onClick={() => keyInput.current?.click()}
          />
          {keyFile && (
            <ListRow
              title={t('Schlüsseldatei entfernen')}
              tone="danger"
              disabled={flow.busy}
              onClick={() => setKeyFile(null)}
            />
          )}
        </ListSection>
      )}
      {(flow.error || keyError) && (
        <p className="m-error" role="alert">
          {flow.error ?? keyError}
        </p>
      )}
      <div className="m-buttons">
        <BigButton icon={ICONS.unlocked} disabled={flow.busy || !ready} onClick={submit}>
          {flow.busy ? t('Öffnet …') : t('Öffnen')}
        </BigButton>
      </div>
      <ListSection>
        <ListRow title={t('Andere Datei')} tone="accent" onClick={flow.reset} />
      </ListSection>
    </form>
  );
}

function PreviewStep({ flow, parsed }: { flow: ImportFlow; parsed: Parsed }) {
  useLanguage();
  const summary = summarize(parsed);
  const total = summary.items.length;
  const counts = importCounts(parsed);
  const passkeys = passkeyCount(parsed);
  return (
    <>
      <ListSection
        header={t('{app}, {format}', { app: sourceLabel(parsed.source), format: parsed.format })}
        footer={t('Noch ist nichts im Tresor; das passiert erst mit „Importieren“.')}
      >
        {counts.map(([n, label]) => (
          <ListRow key={label} title={label} value={n} />
        ))}
        {passkeys > 0 && <ListRow title={t('Passkeys')} value={passkeys} />}
        {summary.folders.length > 0 && (
          <ListRow title={t('Ordner')} value={summary.folders.length} />
        )}
        {total === 0 && <ListRow title={t('In der Datei sind keine Einträge.')} wrap />}
      </ListSection>
      {parsed.warnings.length > 0 && (
        <ListSection header={t('Hinweise')}>
          {parsed.warnings.map((warning) => (
            <ListRow key={warning} icon={ICONS.info} iconTone="warning" title={warning} wrap />
          ))}
        </ListSection>
      )}
      {flow.error && (
        <p className="m-error" role="alert">
          {flow.error}
        </p>
      )}
      <div className="m-buttons">
        <BigButton icon={ICONS.import} disabled={total === 0} onClick={() => void flow.start()}>
          {t('{n} Einträge importieren', { n: total })}
        </BigButton>
      </div>
      {total > 0 && (
        <ListSection header={t('Einträge')}>
          {summary.items.slice(0, LIST_LIMIT).map((item, i) => (
            <ListRow
              key={i}
              title={item.name}
              subtitle={[
                item.wifi ? t('WLAN') : t(TYPE_LABELS[item.type]),
                item.detail,
                item.folder,
              ]
                .filter(Boolean)
                .join(' · ')}
              value={item.totp ? 'TOTP' : undefined}
            />
          ))}
          {total > LIST_LIMIT && (
            <ListRow title={t('… und {n} weitere', { n: total - LIST_LIMIT })} />
          )}
        </ListSection>
      )}
      <ListSection>
        <ListRow title={t('Andere Datei')} tone="accent" onClick={flow.reset} />
      </ListSection>
    </>
  );
}
