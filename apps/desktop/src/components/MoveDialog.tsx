import { listen } from '@tauri-apps/api/event';
import { Button, Segmented, SettingRow } from '@uwusuite/design';
import { useEffect, useState, type FormEvent } from 'react';
import type { ServerKind, TwoFactorMethod } from '../lib/api';
import { errorText } from '../lib/errors';
import { N_, locale, t, useLanguage } from '../lib/i18n';
import {
  moveCancel,
  moveClose,
  moveLogin,
  moveLoginNewDevice,
  moveLoginSendEmail,
  moveLoginTwoFactor,
  moveStart,
  moveTarget,
  type MoveCount,
  type MoveFinished,
  type MoveKind,
  type MoveNoticeCode,
  type MovePreview,
  type MoveProgress,
  type MoveStep,
  type MoveTarget,
} from '../lib/moving';
import { NewDevice, TwoFactor, type CodeActions } from './LoginScreen';
import { Modal } from './Modal';
import { PasswordInput } from './PasswordInput';
import { NyuBusy } from './nyu/stage';

const CODES: CodeActions<MoveStep> = {
  twoFactor: (provider, code) => moveLoginTwoFactor(provider, code),
  newDevice: moveLoginNewDevice,
  sendEmail: moveLoginSendEmail,
  remember: false,
};

const NOTICE: Record<MoveNoticeCode, string> = {
  trash: N_('Einträge im Papierkorb bleiben, wo sie sind.'),
  broken: N_(
    'Einträge, Organisationen oder Sends ließen sich nicht entschlüsseln und bleiben zurück.',
  ),
  'read-only': N_(
    'Einträge in Organisationen darfst du nur lesen (oder ihr Passwort nicht sehen) – sie bleiben dort.',
  ),
  'org-members': N_(
    'Organisationen ziehen ohne ihre Mitglieder um: Lade sie danach im Web-Tresor wieder ein.',
  ),
  'orgs-as-folders': N_(
    'Organisationen werden zu Ordnern mit ihrem Namen: Hier kannst du keine (weitere) Familie anlegen.',
  ),
  'send-password': N_(
    'Sends hatten ein Passwort. Es liegt nur als Hash auf dem Server und zieht nicht mit – sie kommen ohne Passwort an.',
  ),
  'send-emails': N_('Sends nur für bestimmte Adressen kommen ohne diese Einschränkung an.'),
  'send-file-locked': N_(
    'Datei-Sends mit Passwort, deaktiviert oder schon ausgeschöpft lassen sich nicht abrufen und bleiben zurück.',
  ),
  'send-expired': N_('Abgelaufene Sends bleiben zurück.'),
  'send-file-counted': N_(
    'Datei-Sends werden dafür einmal abgerufen – das zählt dort als ein Zugriff.',
  ),
  'too-large': N_('Dateien sind größer, als dieser Server erlaubt, und bleiben zurück.'),
};

const KIND: Record<MoveKind, string> = {
  folder: N_('Ordner'),
  organization: N_('Organisation'),
  collection: N_('Sammlung'),
  item: N_('Eintrag'),
  attachment: N_('Anhang'),
  send: N_('Send'),
};

type Phase =
  | { name: 'checking' }
  | { name: 'no-target'; target: MoveTarget | null; error: string | null }
  | { name: 'login'; target: MoveTarget }
  | { name: 'two-factor'; methods: TwoFactorMethod[]; message: string | null }
  | { name: 'new-device' }
  | { name: 'preview'; preview: MovePreview }
  | { name: 'running'; preview: MovePreview; progress: MoveProgress | null }
  | { name: 'finished'; preview: MovePreview; finished: MoveFinished }
  | { name: 'failed'; preview: MovePreview; error: string };

function bytes(n: number): string {
  const format = (value: number, unit: string) =>
    `${new Intl.NumberFormat(locale(), { maximumFractionDigits: 1 }).format(value)} ${unit}`;
  if (n >= 1024 * 1024 * 1024) return format(n / 1024 ** 3, 'GB');
  if (n >= 1024 * 1024) return format(n / 1024 ** 2, 'MB');
  if (n >= 1024) return format(n / 1024, 'KB');
  return format(n, 'B');
}

/**
 * Moving a vault in from Bitwarden (cloud or self-hosted) or Vaultwarden:
 * log in to the source, look at what will move, move it. The account on
 * screen is where it goes, and it must be on a UwULock Server.
 */
export function MoveDialog({ onClose }: { onClose: () => void }) {
  useLanguage();
  const [phase, setPhase] = useState<Phase>({ name: 'checking' });

  useEffect(() => {
    moveTarget()
      .then((target) =>
        setPhase(
          target.uwulock ? { name: 'login', target } : { name: 'no-target', target, error: null },
        ),
      )
      .catch((e) => setPhase({ name: 'no-target', target: null, error: errorText(e) }));
    // However the dialog goes (its own buttons, or Settings closing around
    // it), the source account's session is dropped; a running move stops
    // after its current step. Calling it twice is harmless.
    return () => void moveClose().catch(() => undefined);
  }, []);

  useEffect(() => {
    const stops = [
      listen<MoveProgress>('move-progress', ({ payload }) =>
        setPhase((p) => (p.name === 'running' ? { ...p, progress: payload } : p)),
      ),
      listen<MoveFinished>('move-finished', ({ payload }) =>
        setPhase((p) =>
          p.name === 'running' ? { name: 'finished', preview: p.preview, finished: payload } : p,
        ),
      ),
      listen<unknown>('move-failed', ({ payload }) =>
        setPhase((p) =>
          p.name === 'running'
            ? { name: 'failed', preview: p.preview, error: errorText(payload) }
            : p,
        ),
      ),
    ];
    return () => stops.forEach((stop) => void stop.then((unlisten) => unlisten()));
  }, []);

  const close = () => {
    if (phase.name === 'running') {
      // Stops after the object it is on; the dialog stays until it has.
      void moveCancel();
      return;
    }
    void moveClose();
    onClose();
  };

  const loggedIn = (step: MoveStep) => {
    if (step.step === 'done') setPhase({ name: 'preview', preview: step.preview });
    else if (step.step === 'two-factor')
      setPhase({ name: 'two-factor', methods: step.methods, message: step.message });
    else setPhase({ name: 'new-device' });
  };

  const start = async (preview: MovePreview) => {
    setPhase({ name: 'running', preview, progress: null });
    try {
      await moveStart();
    } catch (e) {
      setPhase({ name: 'failed', preview, error: errorText(e) });
    }
  };

  const back = () => {
    void moveClose();
    // Back to the login: whatever the target was, it still is.
    moveTarget()
      .then((target) => setPhase({ name: 'login', target }))
      .catch((e) => setPhase({ name: 'no-target', target: null, error: errorText(e) }));
  };

  return (
    <Modal title={t('Von Bitwarden umziehen')} onCancel={close}>
      {phase.name === 'checking' && <NyuBusy label={t('Einen Moment …')} />}

      {phase.name === 'no-target' && (
        <div className="form">
          <p className="dialog-lead">
            {phase.error ??
              t(
                'Umziehen geht nur in ein Konto auf einem UwULock Server. Füge dein UwULock-Konto hinzu (unten links), wechsle dorthin und öffne das hier noch einmal.',
              )}
          </p>
          <div className="form-actions">
            <span className="spacer" />
            <Button variant="primary" onClick={close}>
              {t('Schließen')}
            </Button>
          </div>
        </div>
      )}

      {phase.name === 'login' && (
        <SourceLogin target={phase.target} onDone={loggedIn} onCancel={close} />
      )}

      {phase.name === 'two-factor' && (
        <TwoFactor
          methods={phase.methods}
          message={phase.message}
          actions={CODES}
          onBack={back}
          onDone={loggedIn}
        />
      )}

      {phase.name === 'new-device' && <NewDevice actions={CODES} onBack={back} onDone={loggedIn} />}

      {phase.name === 'preview' && (
        <PreviewView
          preview={phase.preview}
          onStart={() => void start(phase.preview)}
          onCancel={close}
        />
      )}

      {phase.name === 'running' && (
        <div className="form" aria-live="polite">
          <p className="dialog-lead">
            {phase.progress
              ? t('{done} von {total} – gerade: {kind}', {
                  done: phase.progress.done,
                  total: phase.progress.total,
                  kind: t(KIND[phase.progress.kind]),
                })
              : t('Es geht los …')}
          </p>
          <progress
            className="move-progress"
            max={phase.progress?.total || 1}
            value={phase.progress?.done ?? 0}
          />
          <p className="field-hint">
            {t(
              'Alles wird hier auf diesem Gerät entschlüsselt und für UwULock neu verschlüsselt. Anhalten geht jederzeit – ein neuer Umzug macht dort weiter.',
            )}
          </p>
          <div className="form-actions">
            <span className="spacer" />
            <Button onClick={() => void moveCancel()}>{t('Anhalten')}</Button>
          </div>
        </div>
      )}

      {phase.name === 'finished' && (
        <FinishedView
          finished={phase.finished}
          onContinue={() => void start(phase.preview)}
          onClose={close}
        />
      )}

      {phase.name === 'failed' && (
        <div className="form">
          <p className="form-error" role="alert">
            {phase.error}
          </p>
          <p className="dialog-lead">
            {t(
              'Was schon umgezogen ist, bleibt. Weitermachen setzt dort an, wo es stehen geblieben ist.',
            )}
          </p>
          <div className="form-actions">
            <Button variant="ghost" onClick={close}>
              {t('Schließen')}
            </Button>
            <span className="spacer" />
            <Button variant="primary" onClick={() => void start(phase.preview)}>
              {t('Weitermachen')}
            </Button>
          </div>
        </div>
      )}
    </Modal>
  );
}

function SourceLogin({
  target,
  onDone,
  onCancel,
}: {
  target: MoveTarget;
  onDone: (step: MoveStep) => void;
  onCancel: () => void;
}) {
  useLanguage();
  const [kind, setKind] = useState<ServerKind>('bitwarden-us');
  const [url, setUrl] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const step = await moveLogin(
        kind === 'self-hosted' ? { kind, url: url.trim() } : { kind },
        email.trim(),
        password,
      );
      setPassword('');
      onDone(step);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const servers: { value: ServerKind; label: string }[] = [
    { value: 'bitwarden-us', label: 'bitwarden.com' },
    { value: 'bitwarden-eu', label: 'bitwarden.eu' },
    { value: 'self-hosted', label: t('Selbst gehostet') },
  ];

  return (
    <form className="form" onSubmit={submit} aria-busy={busy}>
      <p className="dialog-lead">
        {t(
          'Melde dich bei dem Konto an, aus dem du umziehst. Diese Anmeldung bleibt nur, solange dieser Dialog offen ist; am alten Konto ändert sich nichts.',
        )}
      </p>
      <p className="field-hint">
        {t('Ziel: {email} auf {server}.', { email: target.email, server: target.server })}{' '}
        {target.families
          ? t('Organisationen werden dort zu Familien, wenn du welche anlegen darfst.')
          : t('Organisationen kommen dort in Ordner mit ihrem Namen.')}
      </p>
      <div className="field">
        <span>{t('Server')}</span>
        <Segmented
          value={kind}
          onChange={setKind}
          options={servers}
          label={t('Server')}
          disabled={busy}
          className="w-full [&>button]:flex-1 phone:[&>button]:h-auto phone:[&>button]:min-h-8 phone:[&>button]:px-2 phone:[&>button]:py-1"
        />
      </div>
      {kind === 'self-hosted' && (
        <label className="field">
          <span>{t('Server-Adresse')}</span>
          <input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://vault.example.org"
            spellCheck={false}
            required
            disabled={busy}
          />
        </label>
      )}
      <label className="field">
        <span>{t('E-Mail-Adresse')}</span>
        <input
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          autoComplete="off"
          spellCheck={false}
          required
          disabled={busy}
        />
      </label>
      <label className="field">
        <span>{t('Master-Passwort')}</span>
        <PasswordInput value={password} onChange={setPassword} disabled={busy} autoComplete="off" />
      </label>
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      <div className="form-actions">
        <Button variant="ghost" onClick={onCancel} disabled={busy}>
          {t('Abbrechen')}
        </Button>
        <span className="spacer" />
        <Button variant="primary" type="submit" disabled={busy || !password}>
          {busy ? t('Liest beide Tresore …') : t('Anmelden')}
        </Button>
      </div>
    </form>
  );
}

function CountRow({ label, count }: { label: string; count: MoveCount }) {
  if (count.total === 0) return null;
  return (
    <li>
      <span>{label}</span>
      <span className="move-count">
        {count.todo}
        {count.moved > 0 && <small>{t('({n} schon umgezogen)', { n: count.moved })}</small>}
      </span>
    </li>
  );
}

function PreviewView({
  preview,
  onStart,
  onCancel,
}: {
  preview: MovePreview;
  onStart: () => void;
  onCancel: () => void;
}) {
  useLanguage();
  const counts = [
    preview.folders,
    preview.organizations,
    preview.collections,
    preview.items,
    preview.attachments,
    preview.sends,
  ];
  const nothing = counts.every((c) => c.todo === 0);
  return (
    <div className="form">
      <p className="dialog-lead">
        {t('Von {source} ({sourceEmail}) nach {target} ({targetEmail}):', {
          source: preview.source,
          sourceEmail: preview.sourceEmail,
          target: preview.target,
          targetEmail: preview.targetEmail,
        })}
      </p>
      <ul className="move-counts">
        <CountRow label={t('Ordner')} count={preview.folders} />
        <CountRow
          label={preview.families ? t('Organisationen → Familien') : t('Organisationen → Ordner')}
          count={preview.organizations}
        />
        <CountRow label={t('Sammlungen')} count={preview.collections} />
        <CountRow label={t('Einträge')} count={preview.items} />
        <CountRow label={t('Anhänge')} count={preview.attachments} />
        <CountRow label={t('Sends')} count={preview.sends} />
      </ul>
      {preview.fileBytes > 0 && (
        <p className="field-hint">
          {t('Dateien zusammen: {size}', { size: bytes(preview.fileBytes) })}
        </p>
      )}
      {preview.notices.length > 0 && (
        <ul className="move-notices">
          {preview.notices.map((notice) => (
            <li key={notice.code}>
              <strong>{notice.count}×</strong> {t(NOTICE[notice.code])}
            </li>
          ))}
        </ul>
      )}
      <p className="field-hint">
        {t(
          'Jeder Send bekommt einen neuen Link. Am alten Konto wird nichts gelöscht. Ein zweiter Umzug holt nur, was neu dazugekommen ist.',
        )}
      </p>
      <div className="form-actions">
        <Button variant="ghost" onClick={onCancel}>
          {t('Abbrechen')}
        </Button>
        <span className="spacer" />
        <Button variant="primary" onClick={onStart} disabled={nothing}>
          {nothing ? t('Alles schon umgezogen ✧') : t('Umziehen')}
        </Button>
      </div>
    </div>
  );
}

function FinishedView({
  finished,
  onContinue,
  onClose,
}: {
  finished: MoveFinished;
  onContinue: () => void;
  onClose: () => void;
}) {
  useLanguage();
  const { moved, failed } = finished.summary;
  const parts = [
    [moved.items, t('Einträge')],
    [moved.attachments, t('Anhänge')],
    [moved.folders, t('Ordner')],
    [moved.organizations, t('Organisationen')],
    [moved.collections, t('Sammlungen')],
    [moved.sends, t('Sends')],
  ] as const;
  return (
    <div className="form">
      <p className="dialog-lead">
        {finished.cancelled
          ? t('Angehalten. Weitermachen setzt dort an, wo es stehen geblieben ist.')
          : t('Umgezogen ✧')}
      </p>
      <ul className="move-counts">
        {parts
          .filter(([n]) => n > 0)
          .map(([n, label]) => (
            <li key={label}>
              <span>{label}</span>
              <span className="move-count">{n}</span>
            </li>
          ))}
      </ul>
      {finished.summary.orgsAsFolders > 0 && (
        <p className="field-hint">
          {t('Der Server hat keine Familie angelegt; {n} Organisationen sind jetzt Ordner.', {
            n: finished.summary.orgsAsFolders,
          })}
        </p>
      )}
      {failed.length > 0 && (
        <>
          <p className="form-error" role="alert">
            {t('{n} Dinge sind nicht umgezogen. Ein neuer Umzug versucht sie noch einmal.', {
              n: failed.length,
            })}
          </p>
          <ul className="move-notices">
            {failed.slice(0, 20).map((f, index) => (
              <li key={index}>
                <strong>{t(KIND[f.kind])}</strong> {f.message}
              </li>
            ))}
          </ul>
        </>
      )}
      <div className="form-actions">
        {finished.cancelled && (
          <Button variant="ghost" onClick={onClose}>
            {t('Schließen')}
          </Button>
        )}
        <span className="spacer" />
        {finished.cancelled ? (
          <Button variant="primary" onClick={onContinue}>
            {t('Weitermachen')}
          </Button>
        ) : (
          <Button variant="primary" onClick={onClose}>
            {t('Fertig')}
          </Button>
        )}
      </div>
    </div>
  );
}

/** The row in Settings → Account that opens the move. */
export function MoveSetting() {
  useLanguage();
  const [open, setOpen] = useState(false);
  return (
    <>
      <SettingRow
        label={t('Von Bitwarden umziehen')}
        description={t(
          'Einträge, Ordner, Anhänge, Sends und Organisationen aus Bitwarden oder Vaultwarden in dieses UwULock-Konto holen. Entschlüsselt wird nur auf diesem Gerät.',
        )}
      >
        <Button size="sm" onClick={() => setOpen(true)}>
          {t('Umziehen …')}
        </Button>
      </SettingRow>
      {open && <MoveDialog onClose={() => setOpen(false)} />}
    </>
  );
}
