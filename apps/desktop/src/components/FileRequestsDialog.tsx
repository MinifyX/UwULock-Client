/**
 * File requests: links that let somebody without an account upload files
 * and a message, encrypted for this account. Here the owner makes them,
 * changes or withdraws their links, reads what arrived, saves files, and
 * takes an upload over into an item. The upload page itself is the web
 * vault's.
 */

import { useCallback, useEffect, useState } from 'react';
import { copyGenerated } from '../lib/api';
import { errorText } from '../lib/errors';
import { when } from '../lib/format';
import { t, useLanguage } from '../lib/i18n';
import { getSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import {
  createFileRequest,
  deleteFileRequest,
  deleteSubmission,
  fileRequestSubmissions,
  fileRequests,
  markSubmissionSeen,
  refreshUwu,
  saveSubmissionFile,
  takeOverSubmission,
  updateFileRequest,
  useUwu,
  type FileRequest,
  type FileRequestInput,
  type Submission,
} from '../lib/uwu';
import { Icon } from './Icon';
import { Modal } from './Modal';

type View =
  { kind: 'list' } | { kind: 'request'; id: string } | { kind: 'form'; id: string | null };

function size(bytes: number | null | undefined): string {
  if (!bytes) return '0 B';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

async function copyLink(link: string) {
  try {
    await copyGenerated(link);
    const seconds = getSettings().clipboardClear;
    toast(
      seconds > 0
        ? t('Link kopiert ✧ – wird nach {n} s geleert', { n: seconds })
        : t('Link kopiert ✧'),
    );
  } catch (e) {
    toast(errorText(e), 'error');
  }
}

export function FileRequestsDialog({
  onClose,
  onTakenOver,
}: {
  onClose: () => void;
  onTakenOver: (itemId: string) => void;
}) {
  useLanguage();
  const [view, setView] = useState<View>({ kind: 'list' });
  const [requests, setRequests] = useState<FileRequest[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setRequests(await fileRequests());
      setError(null);
    } catch (e) {
      setError(errorText(e));
      setRequests([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const current =
    view.kind !== 'list' && view.id ? (requests?.find((r) => r.id === view.id) ?? null) : null;

  return (
    <Modal
      title={
        view.kind === 'form'
          ? view.id
            ? t('Dateianfrage ändern')
            : t('Neue Dateianfrage')
          : current
            ? current.label || current.title || t('Dateianfrage')
            : t('Dateianfragen')
      }
      size="wide"
      onCancel={view.kind === 'list' ? onClose : () => setView({ kind: 'list' })}
      footer={
        view.kind === 'list' ? (
          <>
            <button className="quiet" onClick={onClose}>
              {t('Schließen')}
            </button>
            <span className="spacer" />
            <button className="primary" onClick={() => setView({ kind: 'form', id: null })}>
              <Icon name="plus" size={15} />
              {t('Neue Dateianfrage')}
            </button>
          </>
        ) : view.kind === 'request' ? (
          <>
            <button className="quiet" onClick={() => setView({ kind: 'list' })}>
              {t('Zurück')}
            </button>
            <span className="spacer" />
            {current && (
              <button onClick={() => setView({ kind: 'form', id: current.id })}>
                <Icon name="pencil" size={15} />
                {t('Ändern')}
              </button>
            )}
          </>
        ) : undefined
      }
    >
      <div className="extras-scroll">
        {error && (
          <p className="notice" data-tone="error">
            {error}
          </p>
        )}
        {view.kind === 'list' && (
          <RequestList requests={requests} onOpen={(id) => setView({ kind: 'request', id })} />
        )}
        {view.kind === 'request' && current && (
          <RequestView
            request={current}
            onChanged={() => void load()}
            onDeleted={() => {
              setView({ kind: 'list' });
              void load();
            }}
            onTakenOver={onTakenOver}
          />
        )}
        {view.kind === 'form' && (
          <RequestForm
            request={current}
            onCancel={() =>
              setView(current ? { kind: 'request', id: current.id } : { kind: 'list' })
            }
            onSaved={(saved) => {
              setRequests((list) => [...(list ?? []).filter((r) => r.id !== saved.id), saved]);
              setView({ kind: 'request', id: saved.id });
            }}
          />
        )}
      </div>
    </Modal>
  );
}

function RequestList({
  requests,
  onOpen,
}: {
  requests: FileRequest[] | null;
  onOpen: (id: string) => void;
}) {
  useLanguage();
  if (requests === null) return <p className="dialog-lead">{t('Einen Moment …')}</p>;
  if (requests.length === 0)
    return (
      <p className="dialog-lead">
        {t(
          'Mit einer Dateianfrage schickst du jemandem einen Link, über den er dir Dateien und eine Nachricht hochlädt – verschlüsselt, nur du kannst sie öffnen. Ein Konto braucht er dafür nicht.',
        )}
      </p>
    );
  const sorted = [...requests].sort((a, b) =>
    (b.expirationDate ?? '').localeCompare(a.expirationDate ?? ''),
  );
  return (
    <ul className="extras-list">
      {sorted.map((request) => {
        const expired = request.expirationDate
          ? new Date(request.expirationDate).getTime() < Date.now()
          : false;
        return (
          <li key={request.id}>
            <button className="extras-row" onClick={() => onOpen(request.id)}>
              <Icon name="inbox" size={16} />
              <span className="extras-row-text">
                <span className="item-name">
                  {request.label || request.title || t('(ohne Namen)')}
                </span>
                <span className="item-sub">
                  {[
                    request.disabled
                      ? t('abgeschaltet')
                      : expired
                        ? t('abgelaufen')
                        : t('bis {when}', { when: when(request.expirationDate) ?? '' }),
                    request.maxSubmissions
                      ? t('{n} von {max} Uploads', {
                          n: request.submissionCount,
                          max: request.maxSubmissions,
                        })
                      : t('{n} Uploads', { n: request.submissionCount }),
                  ].join(' · ')}
                </span>
              </span>
              {request.unseen > 0 && <span className="nav-badge">{request.unseen}</span>}
              <Icon name="chevron" size={14} />
            </button>
          </li>
        );
      })}
    </ul>
  );
}

function RequestView({
  request,
  onChanged,
  onDeleted,
  onTakenOver,
}: {
  request: FileRequest;
  onChanged: () => void;
  onDeleted: () => void;
  onTakenOver: (itemId: string) => void;
}) {
  useLanguage();
  const [submissions, setSubmissions] = useState<Submission[] | null>(null);
  const [asking, setAsking] = useState<
    | null
    | { kind: 'delete-request' }
    | { kind: 'delete'; submission: Submission }
    | { kind: 'save'; submission: Submission; file: Submission['files'][number] }
    | { kind: 'take-over'; submission: Submission }
  >(null);
  const [itemName, setItemName] = useState('');
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setSubmissions(await fileRequestSubmissions(request.id));
    } catch (e) {
      toast(errorText(e), 'error');
      setSubmissions([]);
    }
  }, [request.id]);

  useEffect(() => {
    void load();
  }, [load]);

  const act = async (what: () => Promise<unknown>, done?: string) => {
    setBusy(true);
    try {
      await what();
      if (done) toast(done);
    } catch (e) {
      toast(errorText(e), 'error');
    } finally {
      setBusy(false);
      setAsking(null);
    }
  };

  const senderLabel = t('Absender (nicht geprüft)');

  return (
    <div className="extras-form">
      <section className="detail-card">
        {request.title && (
          <div className="detail-row">
            <div className="detail-text">
              <span className="detail-label">{t('Titel für den Empfänger')}</span>
              <span className="detail-value">{request.title}</span>
            </div>
          </div>
        )}
        {request.note && (
          <div className="detail-row">
            <div className="detail-text">
              <span className="detail-label">{t('Hinweis')}</span>
              <span className="detail-value">{request.note}</span>
            </div>
          </div>
        )}
        {request.foreignKey && (
          <p className="notice" data-tone="error">
            {t(
              'Dieser Link verschlüsselt Uploads für einen Schlüssel, der nicht deiner ist – jemand, der das Link-Geheimnis kannte, hat ihn eingesetzt. Gib den Link nicht weiter; bearbeite die Anfrage mit einem neuen Link oder lösche sie.',
            )}
          </p>
        )}
        <div className="detail-row">
          <div className="detail-text">
            <span className="detail-label">{t('Link')}</span>
            <span className="detail-value mono uri">
              {request.link ??
                (request.foreignKey
                  ? t('Zurückgehalten: nicht für deinen Schlüssel.')
                  : t('Der Link lässt sich nicht mehr zeigen.'))}
            </span>
          </div>
          {request.link && (
            <div className="detail-actions">
              <button
                className="icon-button"
                title={t('Link kopieren')}
                aria-label={t('Link kopieren')}
                onClick={() => void copyLink(request.link ?? '')}
              >
                <Icon name="copy" size={15} />
              </button>
            </div>
          )}
        </div>
        <div className="detail-row">
          <div className="detail-text">
            <span className="detail-label">{t('Gültig')}</span>
            <span className="detail-value">
              {request.disabled
                ? t('abgeschaltet')
                : t('bis {when}', { when: when(request.expirationDate) ?? '' })}
              {request.passwordSet && ` · ${t('mit Passwort')}`}
              {' · '}
              {request.maxSubmissions
                ? t('{n} von {max} Uploads', {
                    n: request.submissionCount,
                    max: request.maxSubmissions,
                  })
                : t('{n} Uploads', { n: request.submissionCount })}
            </span>
          </div>
        </div>
      </section>
      {request.deletionDate && (
        <p className="muted small">
          {t('Der Server löscht die Anfrage mit allem darin am {when}.', {
            when: when(request.deletionDate) ?? '',
          })}
        </p>
      )}

      <h3 className="extras-heading">{t('Eingegangen')}</h3>
      {submissions === null && <p className="muted">{t('Einen Moment …')}</p>}
      {submissions?.length === 0 && <p className="muted">{t('Noch nichts angekommen.')}</p>}
      {submissions?.map((submission) => (
        <section className="detail-card submission" key={submission.id}>
          <div className="submission-head">
            <span>
              {when(submission.creationDate)}
              {!submission.seen && <span className="nav-badge">{t('neu')}</span>}
            </span>
            <span className="spacer" />
            {!submission.seen && (
              <button
                className="quiet"
                disabled={busy}
                onClick={() =>
                  void act(async () => {
                    await markSubmissionSeen(request.id, submission.id);
                    await load();
                    onChanged();
                  })
                }
              >
                <Icon name="check" size={14} />
                {t('Gesehen')}
              </button>
            )}
            <button
              className="quiet"
              disabled={busy || submission.broken}
              onClick={() => {
                setItemName(
                  [request.title || request.label, submission.senderName]
                    .filter(Boolean)
                    .join(' – '),
                );
                setAsking({ kind: 'take-over', submission });
              }}
            >
              <Icon name="import" size={14} />
              {t('Als Eintrag übernehmen')}
            </button>
            <button
              className="icon-button"
              disabled={busy}
              title={t('Löschen')}
              aria-label={t('Upload löschen')}
              onClick={() => setAsking({ kind: 'delete', submission })}
            >
              <Icon name="trash" size={15} />
            </button>
          </div>
          {submission.broken && (
            <p className="notice" data-tone="error">
              {t('Dieser Upload ließ sich mit deinem Schlüssel nicht öffnen.')}
            </p>
          )}
          {(submission.senderName || submission.senderEmail) && (
            <p className="submission-sender">
              <Icon name="user" size={13} />
              {[submission.senderName, submission.senderEmail].filter(Boolean).join(' · ')}
              <span
                className="chip chip-muted"
                title={t('Das hat der Absender selbst eingetragen.')}
              >
                {t('nicht geprüft')}
              </span>
            </p>
          )}
          {submission.text && <pre className="submission-text">{submission.text}</pre>}
          {submission.files.map((file) => (
            <div className="detail-row" key={file.id}>
              <Icon name="file" size={15} />
              <div className="detail-text">
                <span className="detail-value">{file.name ?? t('(unlesbarer Name)')}</span>
                <span className="detail-label">{size(file.size)}</span>
              </div>
              <div className="detail-actions">
                <button
                  className="icon-button"
                  disabled={busy || !file.name}
                  title={t('In „Downloads“ speichern')}
                  aria-label={t('{name} speichern', { name: file.name ?? '' })}
                  onClick={() => setAsking({ kind: 'save', submission, file })}
                >
                  <Icon name="download" size={15} />
                </button>
              </div>
            </div>
          ))}
        </section>
      ))}

      <p>
        <button
          className="link-button danger-text"
          disabled={busy}
          onClick={() => setAsking({ kind: 'delete-request' })}
        >
          {t('Dateianfrage löschen')}
        </button>
      </p>

      {asking?.kind === 'save' && (
        <Modal
          title={t('Datei speichern?')}
          onCancel={() => setAsking(null)}
          footer={
            <>
              <button className="quiet" data-secondary onClick={() => setAsking(null)}>
                {t('Abbrechen')}
              </button>
              <span className="spacer" />
              <button
                className="primary"
                data-autofocus
                onClick={() =>
                  void act(async () => {
                    const path = await saveSubmissionFile(
                      request.id,
                      asking.submission.id,
                      asking.file.id,
                      asking.file.risky,
                    );
                    toast(t('Gespeichert: {path}', { path }));
                  })
                }
              >
                <Icon name="download" size={15} />
                {t('Speichern')}
              </button>
            </>
          }
        >
          <p className="dialog-lead">
            {t(
              '„{name}“ wird entschlüsselt in deinem Ordner „Downloads“ gespeichert. Öffne Dateien von Unbekannten mit Vorsicht.',
              { name: asking.file.name ?? '' },
            )}
          </p>
          {asking.file.risky && (
            <p className="notice" data-tone="error" role="alert">
              {t(
                'Achtung: Diese Datei ist ein Programm, ein Skript oder ein Dokument mit Makros. Sie kann beim Öffnen alles tun, was du auf diesem Rechner darfst. Öffne sie nur, wenn du weißt, von wem sie ist und dass du sie erwartest.',
              )}
            </p>
          )}
        </Modal>
      )}

      {asking?.kind === 'take-over' && (
        <Modal
          title={t('Als Eintrag übernehmen')}
          onCancel={() => setAsking(null)}
          footer={
            <>
              <button className="quiet" data-secondary onClick={() => setAsking(null)}>
                {t('Abbrechen')}
              </button>
              <span className="spacer" />
              <button
                className="primary"
                disabled={busy || !itemName.trim()}
                onClick={() =>
                  void act(async () => {
                    const id = await takeOverSubmission(
                      request.id,
                      asking.submission.id,
                      itemName.trim(),
                      senderLabel,
                    );
                    toast(t('Als Eintrag übernommen ✧'));
                    refreshUwu();
                    onTakenOver(id);
                  })
                }
              >
                {busy ? t('Übernimmt …') : t('Übernehmen')}
              </button>
            </>
          }
        >
          <p className="dialog-lead">
            {t(
              'Eine sichere Notiz mit der Nachricht und dem Absender, die Dateien als Anhänge. Der Upload verschwindet danach aus der Anfrage.',
            )}
          </p>
          <label className="field">
            <span>{t('Name des Eintrags')}</span>
            <input
              type="text"
              value={itemName}
              maxLength={200}
              autoFocus
              onChange={(e) => setItemName(e.target.value)}
            />
          </label>
        </Modal>
      )}

      {(asking?.kind === 'delete' || asking?.kind === 'delete-request') && (
        <Modal
          title={asking.kind === 'delete' ? t('Upload löschen?') : t('Dateianfrage löschen?')}
          tone="warning"
          onCancel={() => setAsking(null)}
          footer={
            <>
              <span className="spacer" />
              <button
                className="danger"
                data-secondary
                onClick={() =>
                  asking.kind === 'delete'
                    ? void act(async () => {
                        await deleteSubmission(request.id, asking.submission.id);
                        await load();
                        onChanged();
                      }, t('Gelöscht.'))
                    : void act(async () => {
                        await deleteFileRequest(request.id);
                        refreshUwu();
                        onDeleted();
                      }, t('Gelöscht.'))
                }
              >
                {t('Löschen')}
              </button>
              <button className="primary" data-autofocus onClick={() => setAsking(null)}>
                {t('Abbrechen')}
              </button>
            </>
          }
        >
          <p className="dialog-lead">
            {asking.kind === 'delete'
              ? t('Die Nachricht und die Dateien dieses Uploads verschwinden vom Server.')
              : t(
                  'Die Anfrage, ihr Link und alles, was darüber hochgeladen wurde, verschwinden vom Server.',
                )}
          </p>
        </Modal>
      )}
    </div>
  );
}

const DAYS = [1, 3, 7, 14, 30, 90];

function RequestForm({
  request,
  onCancel,
  onSaved,
}: {
  request: FileRequest | null;
  onCancel: () => void;
  onSaved: (saved: FileRequest) => void;
}) {
  useLanguage();
  const uwu = useUwu();
  const maxDays = uwu.limits?.fileRequestMaxDays ?? 90;
  const maxFiles = Math.min(20, uwu.limits?.fileRequestMaxFiles ?? 20);
  const serverMib = uwu.limits?.maxFileBytes
    ? Math.floor(uwu.limits.maxFileBytes / 1024 / 1024)
    : null;
  const [form, setForm] = useState({
    label: request?.label ?? '',
    title: request?.title ?? '',
    note: request?.note ?? '',
    owner: request?.owner ?? '',
    days: request ? 0 : 7,
    maxSubmissions: request ? String(request.maxSubmissions ?? '') : '1',
    maxFiles: request?.maxFiles ?? 10,
    maxFileMib: request?.maxFileBytes
      ? String(Math.floor(request.maxFileBytes / 1024 / 1024))
      : serverMib
        ? String(Math.min(serverMib, 100))
        : '100',
    textAllowed: request?.textAllowed ?? true,
    password: '',
    removePassword: false,
    sendDomainId: request?.sendDomainId ?? '',
    disabled: request?.disabled ?? false,
    newLink: request?.foreignKey ?? false,
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<typeof form>) => setForm((f) => ({ ...f, ...patch }));

  const save = async () => {
    setBusy(true);
    setError(null);
    const input: FileRequestInput = {
      label: form.label.trim(),
      title: form.title.trim(),
      note: form.note.trim() || null,
      owner: form.owner.trim() || null,
      expiresInDays: form.days > 0 ? form.days : null,
      maxSubmissions: form.maxSubmissions.trim() ? Number(form.maxSubmissions) : null,
      maxFiles: form.maxFiles,
      maxFileMib: form.maxFileMib.trim() ? Number(form.maxFileMib) : null,
      textAllowed: form.textAllowed,
      password: form.password || null,
      sendDomainId: form.sendDomainId || null,
      disabled: form.disabled,
    };
    try {
      const saved = request
        ? await updateFileRequest(request.id, input, form.newLink, form.removePassword)
        : await createFileRequest(input);
      toast(request ? t('Gespeichert ✧') : t('Angelegt ✧'));
      onSaved(saved);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="extras-form"
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      <div className="editor-row">
        <label className="field">
          <span>{t('Titel für den Empfänger')}</span>
          <input
            type="text"
            value={form.title}
            maxLength={200}
            autoFocus
            placeholder={t('z. B. Scan des Reisepasses')}
            onChange={(e) => set({ title: e.target.value })}
          />
        </label>
        <label className="field">
          <span>{t('Dein Name dafür (nur für dich)')}</span>
          <input
            type="text"
            value={form.label}
            maxLength={200}
            placeholder={form.title}
            onChange={(e) => set({ label: e.target.value })}
          />
        </label>
      </div>
      <label className="field">
        <span>{t('Hinweis für den Empfänger (optional)')}</span>
        <textarea rows={2} value={form.note} onChange={(e) => set({ note: e.target.value })} />
      </label>
      <label className="field">
        <span>{t('Dein Name, wie der Empfänger ihn sieht (optional)')}</span>
        <input
          type="text"
          value={form.owner}
          maxLength={100}
          onChange={(e) => set({ owner: e.target.value })}
        />
      </label>
      <div className="editor-row">
        <label className="field">
          <span>{t('Gültig für')}</span>
          <select value={form.days} onChange={(e) => set({ days: Number(e.target.value) })}>
            {request && <option value={0}>{t('Datum behalten')}</option>}
            {DAYS.filter((n) => n <= maxDays).map((n) => (
              <option key={n} value={n}>
                {n === 1 ? t('1 Tag') : t('{n} Tage', { n })}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t('Uploads höchstens')}</span>
          <input
            type="number"
            min={1}
            max={100}
            value={form.maxSubmissions}
            placeholder={t('bis zum Ablauf')}
            onChange={(e) => set({ maxSubmissions: e.target.value.replace(/\D/g, '') })}
          />
        </label>
      </div>
      <div className="editor-row">
        <label className="field">
          <span>{t('Dateien je Upload')}</span>
          <input
            type="number"
            min={0}
            max={maxFiles}
            value={form.maxFiles}
            onChange={(e) =>
              set({ maxFiles: Math.min(maxFiles, Math.max(0, Number(e.target.value) || 0)) })
            }
          />
        </label>
        <label className="field">
          <span>{t('Größe je Datei (MB)')}</span>
          <input
            type="number"
            min={1}
            max={serverMib ?? undefined}
            value={form.maxFileMib}
            onChange={(e) => set({ maxFileMib: e.target.value.replace(/\D/g, '') })}
          />
        </label>
      </div>
      <label className="check">
        <input
          type="checkbox"
          checked={form.textAllowed}
          onChange={(e) => set({ textAllowed: e.target.checked })}
        />
        <span>{t('Eine Nachricht erlauben')}</span>
      </label>
      <label className="field">
        <span>
          {request?.passwordSet ? t('Neues Passwort (leer: bleibt)') : t('Passwort (optional)')}
        </span>
        <input
          type="password"
          value={form.password}
          autoComplete="new-password"
          disabled={form.removePassword}
          onChange={(e) => set({ password: e.target.value })}
        />
      </label>
      {request?.passwordSet && (
        <label className="check">
          <input
            type="checkbox"
            checked={form.removePassword}
            onChange={(e) => set({ removePassword: e.target.checked, password: '' })}
          />
          <span>{t('Passwort entfernen')}</span>
        </label>
      )}
      {uwu.sendDomains.length > 0 && (
        <label className="field">
          <span>{t('Link-Adresse')}</span>
          <select value={form.sendDomainId} onChange={(e) => set({ sendDomainId: e.target.value })}>
            <option value="">{t('Hauptadresse des Servers')}</option>
            {uwu.sendDomains.map((d) => (
              <option key={d.id} value={d.id}>
                {d.url.replace(/^https?:\/\//, '')}
              </option>
            ))}
          </select>
        </label>
      )}
      {request && (
        <>
          <label className="check">
            <input
              type="checkbox"
              checked={form.disabled}
              onChange={(e) => set({ disabled: e.target.checked })}
            />
            <span>{t('Abschalten (der Link nimmt nichts mehr an)')}</span>
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={form.newLink}
              onChange={(e) => set({ newLink: e.target.checked })}
            />
            <span>{t('Neuer Link – alle bisherigen Links funktionieren dann nicht mehr')}</span>
          </label>
        </>
      )}
      <div className="form-actions">
        <button type="button" className="quiet" onClick={onCancel}>
          {t('Abbrechen')}
        </button>
        <span className="spacer" />
        <button
          type="submit"
          className="primary"
          disabled={busy || !form.title.trim() || (form.maxFiles === 0 && !form.textAllowed)}
        >
          {busy ? t('Speichert …') : request ? t('Speichern') : t('Anlegen')}
        </button>
      </div>
    </form>
  );
}
