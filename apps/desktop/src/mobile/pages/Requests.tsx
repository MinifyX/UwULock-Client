/**
 * File requests on a phone or iPad: links over which somebody without an
 * account uploads files and a message, encrypted for this account. The list
 * (with how many uploads are new), a request's page (its link, what arrived,
 * its rules), an upload in a short sheet (save its files, take it over into
 * an item) and the editor. The logic is the desktop's `FileRequestsDialog`.
 */

import { Fab, haptic, ICONS, ListRow, ListSection, NavButton, Stepper } from '@uwusuite/design';
import { useCallback, useEffect, useState } from 'react';
import { errorText, toastError } from '../../lib/errors';
import { when } from '../../lib/format';
import { t, useLanguage } from '../../lib/i18n';
import { note } from '../../lib/toast';
import {
  createFileRequest,
  deleteFileRequest,
  deleteSubmission,
  fileRequestSubmissions,
  fileRequests,
  has,
  markSubmissionSeen,
  refreshUwu,
  saveSubmissionFile,
  takeOverSubmission,
  updateFileRequest,
  useUwu,
  type FileRequest,
  type FileRequestInput,
  type Submission,
} from '../../lib/uwu';
import { useMobile, useNav } from '../state';
import {
  BigButton,
  ChoiceSheet,
  Chip,
  EditSurface,
  Empty,
  FieldInput,
  GlyphTile,
  Hero,
  LinkBox,
  Page,
  ShortSheet,
  StateChip,
  Toggle,
  useConfirm,
} from '../ui';
import { copyLink, fileSize, listStore, shareLink, useEditing, useListStore } from './Sends';

export const requestStore = listStore(fileRequests);

const expired = (request: FileRequest) =>
  request.expirationDate ? new Date(request.expirationDate).getTime() < Date.now() : false;

function requestState(request: FileRequest): 'on' | 'off' | 'gone' {
  return request.disabled ? 'off' : expired(request) ? 'gone' : 'on';
}

function validText(request: FileRequest): string {
  return request.disabled
    ? t('abgeschaltet')
    : expired(request)
      ? t('abgelaufen')
      : t('bis {when}', { when: when(request.expirationDate) ?? '' });
}

function uploadsText(request: FileRequest): string {
  return request.maxSubmissions
    ? t('{n} von {max} Uploads', { n: request.submissionCount, max: request.maxSubmissions })
    : t('{n} Uploads', { n: request.submissionCount });
}

const requestName = (request: FileRequest) => request.label || request.title || t('Dateianfrage');

/** The request as the editor saves it, with `patch` changed; the date and password stay. */
function requestInput(request: FileRequest, patch: Partial<FileRequestInput>): FileRequestInput {
  return {
    label: request.label ?? '',
    title: request.title ?? '',
    note: request.note,
    owner: request.owner,
    expiresInDays: null,
    maxSubmissions: request.maxSubmissions,
    maxFiles: request.maxFiles,
    maxFileMib: request.maxFileBytes ? Math.floor(request.maxFileBytes / 1024 / 1024) : null,
    textAllowed: request.textAllowed,
    password: null,
    sendDomainId: request.sendDomainId,
    disabled: request.disabled,
    ...patch,
  };
}

function Unavailable({ title }: { title: string }) {
  useLanguage();
  return (
    <Page title={title}>
      <Empty title={t('Keine Dateianfragen')}>
        {t('Dieser Server bietet keine Dateianfragen an.')}
      </Empty>
    </Page>
  );
}

export function RequestsPage() {
  useLanguage();
  const { ios, ipad, android } = useMobile();
  const nav = useNav();
  const uwu = useUwu();
  const { value: requests, error } = useListStore(requestStore);
  const editor = useEditing<null>();

  if (!has(uwu, 'file-requests')) return <Unavailable title={t('Dateianfragen')} />;

  const sorted = [...(requests ?? [])].sort((a, b) =>
    (b.expirationDate ?? '').localeCompare(a.expirationDate ?? ''),
  );
  const add = () => editor.open(null);

  return (
    <>
      <Page
        title={t('Dateianfragen')}
        largeTitle
        subtitle={t('Andere laden Dateien verschlüsselt zu dir hoch')}
        onRefresh={() => requestStore.load()}
        trailing={
          (ios || ipad) && (
            <NavButton label={t('Neue Dateianfrage')} icon={ICONS.add} onClick={add} />
          )
        }
      >
        {error && <p className="m-error">{error}</p>}
        {requests !== null && (
          <ListSection>
            {sorted.length === 0 ? (
              <Empty title={t('Noch keine Dateianfragen')}>
                {t(
                  'Mit einer Dateianfrage schickst du jemandem einen Link, über den er dir Dateien und eine Nachricht hochlädt – verschlüsselt, nur du kannst sie öffnen. Ein Konto braucht er dafür nicht.',
                )}
              </Empty>
            ) : (
              sorted.map((request) => {
                const state = requestState(request);
                return (
                  <ListRow
                    key={request.id}
                    icon={<GlyphTile icon={ICONS.inbox} />}
                    iconTone="none"
                    title={requestName(request)}
                    subtitle={[
                      request.submissionCount ? uploadsText(request) : t('Noch nichts angekommen'),
                      validText(request),
                    ].join(' · ')}
                    trailing={
                      request.unseen > 0 ? (
                        <span
                          className="m-count-badge"
                          role="img"
                          aria-label={t('{n} neu', { n: request.unseen })}
                        >
                          {request.unseen}
                        </span>
                      ) : state !== 'on' ? (
                        <StateChip state={state} />
                      ) : undefined
                    }
                    chevron={!android}
                    selected={
                      nav.column !== 'phone' &&
                      nav.selected?.page === 'request' &&
                      nav.selected.id === request.id
                    }
                    onClick={() => nav.open({ page: 'request', id: request.id })}
                  />
                );
              })
            )}
          </ListSection>
        )}
      </Page>
      {android && <Fab label={t('Neue Dateianfrage')} icon={ICONS.add} onClick={add} />}
      {editor.editing && (
        <RequestEditor
          key={editor.editing.n}
          open={editor.editing.open}
          request={null}
          onClose={editor.close}
          onSaved={(saved) => {
            editor.close();
            // A new request's link is what the person wants next.
            if (saved.link) void copyLink(saved.link);
            nav.open({ page: 'request', id: saved.id });
          }}
        />
      )}
    </>
  );
}

export function RequestPage({ id }: { id: string }) {
  useLanguage();
  const { android, data } = useMobile();
  const nav = useNav();
  const uwu = useUwu();
  const { value: requests } = useListStore(requestStore);
  const request = requests?.find((r) => r.id === id) ?? null;
  const editor = useEditing<FileRequest>();
  const confirm = useConfirm();
  const [submissions, setSubmissions] = useState<Submission[] | null>(null);
  // The upload in the short sheet; it stays while the sheet slides away.
  const [upload, setUpload] = useState<{ submission: Submission; open: boolean } | null>(null);
  const [takeOver, setTakeOver] = useState<{
    n: number;
    submission: Submission;
    open: boolean;
  } | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setSubmissions(await fileRequestSubmissions(id));
    } catch (e) {
      toastError(e);
      setSubmissions([]);
    }
  }, [id]);

  useEffect(() => {
    void load();
  }, [load]);

  if (!has(uwu, 'file-requests')) return <Unavailable title={t('Dateianfrage')} />;
  if (!request) return <Page title="">{requests !== null && <Empty title={t('Gelöscht')} />}</Page>;

  /** Runs `what`; then the request and its uploads (and the badge on the overview) again. */
  const act = async (what: () => Promise<unknown>, done?: string) => {
    setBusy(true);
    try {
      await what();
      if (done) {
        haptic('success');
        note(done, { tone: 'success' });
      }
      await Promise.all([requestStore.load(), load()]);
      refreshUwu();
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  };

  const openUpload = (submission: Submission) => {
    setUpload({ submission, open: true });
    // Opening an upload is seeing it.
    if (!submission.seen)
      void markSubmissionSeen(request.id, submission.id)
        .then(() => Promise.all([requestStore.load(), load()]))
        .then(refreshUwu)
        .catch((e) => toastError(e));
  };
  const closeUpload = () => setUpload((current) => current && { ...current, open: false });

  const save = (submission: Submission, files: Submission['files']) => {
    const run = () =>
      void act(async () => {
        let last = '';
        for (const file of files.filter((f) => f.name))
          last = await saveSubmissionFile(request.id, submission.id, file.id, file.risky);
        haptic('success');
        note(t('Gespeichert ✧'), { tone: 'success', detail: last || undefined });
      });
    // Programs, scripts and documents with macros are asked about first, as on the desktop.
    if (files.some((f) => f.risky))
      confirm.ask({
        title: t('Datei speichern?'),
        text: t(
          'Achtung: Diese Datei ist ein Programm, ein Skript oder ein Dokument mit Makros. Sie kann beim Öffnen alles tun, was du auf diesem Rechner darfst. Öffne sie nur, wenn du weißt, von wem sie ist und dass du sie erwartest.',
        ),
        confirm: t('Speichern'),
        run,
      });
    else run();
  };

  const name = requestName(request);
  const shown = upload?.submission;
  const saveLabel = android ? t('In „Downloads“ speichern') : t('In Dateien sichern');

  return (
    <>
      <Page
        hero
        title={name}
        trailing={
          android ? (
            <NavButton label={t('Ändern')} icon={ICONS.edit} onClick={() => editor.open(request)} />
          ) : (
            <NavButton label={t('Ändern')} text onClick={() => editor.open(request)} />
          )
        }
      >
        <Hero
          tile={<GlyphTile icon={ICONS.inbox} size="large" />}
          title={name}
          sub={request.label && request.title ? request.title : undefined}
        >
          <StateChip state={requestState(request)} />
          <Chip icon={ICONS.reminder}>{validText(request)}</Chip>
        </Hero>

        {request.foreignKey && (
          <p className="m-error" role="alert">
            {t(
              'Dieser Link verschlüsselt Uploads für einen Schlüssel, der nicht deiner ist – jemand, der das Link-Geheimnis kannte, hat ihn eingesetzt. Gib den Link nicht weiter; bearbeite die Anfrage mit einem neuen Link oder lösche sie.',
            )}
          </p>
        )}

        <ListSection header={t('Link')}>
          {request.link ? (
            <LinkBox
              url={request.link}
              onCopy={() => void copyLink(request.link ?? '')}
              onShare={() => void shareLink(request.link ?? '')}
            />
          ) : (
            <div className="m-note">
              {request.foreignKey
                ? t('Zurückgehalten: nicht für deinen Schlüssel.')
                : t('Der Link lässt sich nicht mehr zeigen.')}
            </div>
          )}
        </ListSection>

        {request.note && (
          <ListSection header={t('Hinweis')}>
            <div className="m-note">{request.note}</div>
          </ListSection>
        )}

        <ListSection header={t('Eingegangen')}>
          {submissions === null ? (
            <Empty title={t('Einen Moment …')} />
          ) : submissions.length === 0 ? (
            <Empty title={t('Noch nichts angekommen')}>
              {t('Teile den Link, dann landen Uploads hier.')}
            </Empty>
          ) : (
            submissions.map((submission) => (
              <ListRow
                key={submission.id}
                icon={ICONS.file}
                iconTone="neutral"
                title={submission.senderName || submission.senderEmail || t('Ohne Absender')}
                subtitle={[
                  submission.files.map((file) => file.name ?? t('(unlesbarer Name)')).join(', ') ||
                    null,
                  when(submission.creationDate),
                ]
                  .filter(Boolean)
                  .join(' · ')}
                trailing={!submission.seen ? <StateChip state="new" /> : undefined}
                chevron={!android}
                onClick={() => openUpload(submission)}
              />
            ))
          )}
        </ListSection>

        <ListSection
          header={t('Regeln')}
          footer={
            request.deletionDate
              ? t('Der Server löscht die Anfrage mit allem darin am {when}.', {
                  when: when(request.deletionDate) ?? '',
                })
              : undefined
          }
        >
          <ListRow
            title={t('Größe je Datei')}
            value={request.maxFileBytes ? fileSize(request.maxFileBytes) : '–'}
          />
          <ListRow title={t('Dateien je Upload')} value={String(request.maxFiles)} />
          <ListRow
            title={t('Uploads höchstens')}
            value={request.maxSubmissions ? uploadsText(request) : t('bis zum Ablauf')}
          />
          <ListRow title={t('Mit Passwort')} value={request.passwordSet ? t('Ja') : t('Nein')} />
          <ListRow
            title={t('Eine Nachricht erlauben')}
            trailing={
              <Toggle
                label={t('Eine Nachricht erlauben')}
                checked={request.textAllowed}
                // Without files and without a message nothing could arrive.
                disabled={busy || (request.textAllowed && request.maxFiles === 0)}
                onChange={(textAllowed) =>
                  void act(() =>
                    updateFileRequest(
                      request.id,
                      requestInput(request, { textAllowed }),
                      false,
                      false,
                    ),
                  )
                }
              />
            }
          />
        </ListSection>

        <ListSection>
          <ListRow
            tone={request.disabled ? 'accent' : 'danger'}
            title={request.disabled ? t('Wieder einschalten') : t('Abschalten')}
            disabled={busy}
            onClick={() =>
              void act(
                () =>
                  updateFileRequest(
                    request.id,
                    requestInput(request, { disabled: !request.disabled }),
                    false,
                    false,
                  ),
                request.disabled
                  ? t('Wieder an ✧')
                  : t('Abgeschaltet: der Link nimmt nichts mehr an'),
              )
            }
          />
          <ListRow
            tone="danger"
            title={t('Dateianfrage löschen')}
            disabled={busy}
            onClick={() =>
              confirm.ask({
                title: t('Dateianfrage löschen?'),
                text: t(
                  'Die Anfrage, ihr Link und alles, was darüber hochgeladen wurde, verschwinden vom Server.',
                ),
                confirm: t('Löschen'),
                run: () =>
                  void (async () => {
                    setBusy(true);
                    try {
                      await deleteFileRequest(request.id);
                      haptic('success');
                      note(t('Dateianfrage gelöscht'), { tone: 'success' });
                      refreshUwu();
                      await requestStore.load();
                      nav.back();
                    } catch (e) {
                      toastError(e);
                    } finally {
                      setBusy(false);
                    }
                  })(),
              })
            }
          />
        </ListSection>
      </Page>

      <ShortSheet
        open={upload?.open ?? false}
        onClose={closeUpload}
        title={t('Upload von {name}', {
          name: shown?.senderName || shown?.senderEmail || t('Unbekannt'),
        })}
      >
        {shown && (
          <>
            <ListSection>
              <ListRow
                icon={ICONS.account}
                iconTone="neutral"
                title={[shown.senderName, shown.senderEmail].filter(Boolean).join(' · ') || '–'}
                subtitle={[t('Absender (nicht geprüft)'), when(shown.creationDate)]
                  .filter(Boolean)
                  .join(' · ')}
              />
              {shown.text && <div className="m-note">{shown.text}</div>}
            </ListSection>
            {shown.broken && (
              <p className="m-error" role="alert">
                {t('Dieser Upload ließ sich mit deinem Schlüssel nicht öffnen.')}
              </p>
            )}
            {shown.files.length > 0 && (
              <ListSection
                header={t('Dateien')}
                footer={t('Öffne Dateien von Unbekannten mit Vorsicht.')}
              >
                {shown.files.map((file) => (
                  <ListRow
                    key={file.id}
                    icon={file.risky ? ICONS.warning : ICONS.file}
                    iconTone={file.risky ? 'warning' : 'neutral'}
                    title={file.name ?? t('(unlesbarer Name)')}
                    subtitle={fileSize(file.size)}
                    trailing={<ICONS.download aria-hidden className="m-check" />}
                    aria-label={t('{name} speichern', { name: file.name ?? '' })}
                    disabled={busy || !file.name}
                    onClick={() => save(shown, [file])}
                  />
                ))}
              </ListSection>
            )}
            <div className="m-buttons">
              <BigButton
                icon={ICONS.import}
                disabled={busy || shown.broken}
                onClick={() => {
                  closeUpload();
                  setTakeOver((current) => ({
                    n: (current?.n ?? 0) + 1,
                    submission: shown,
                    open: true,
                  }));
                }}
              >
                {t('Als Eintrag übernehmen')}
              </BigButton>
              {shown.files.some((f) => f.name) && (
                <BigButton
                  soft
                  icon={ICONS.download}
                  disabled={busy}
                  onClick={() => save(shown, shown.files)}
                >
                  {saveLabel}
                </BigButton>
              )}
              <BigButton
                soft
                danger
                icon={ICONS.delete}
                disabled={busy}
                onClick={() =>
                  confirm.ask({
                    title: t('Upload löschen?'),
                    text: t(
                      'Die Nachricht und die Dateien dieses Uploads verschwinden vom Server.',
                    ),
                    confirm: t('Löschen'),
                    run: () => {
                      closeUpload();
                      void act(() => deleteSubmission(request.id, shown.id), t('Gelöscht.'));
                    },
                  })
                }
              >
                {t('Upload löschen')}
              </BigButton>
            </div>
          </>
        )}
      </ShortSheet>

      {takeOver && (
        <TakeOver
          key={takeOver.n}
          open={takeOver.open}
          request={request}
          submission={takeOver.submission}
          onClose={() => setTakeOver((current) => current && { ...current, open: false })}
          onDone={async (itemId) => {
            setTakeOver((current) => current && { ...current, open: false });
            refreshUwu();
            await Promise.all([data.reload(), requestStore.load(), load()]);
            nav.open({ page: 'item', id: itemId });
          }}
        />
      )}
      {confirm.element}
      {editor.editing && (
        <RequestEditor
          key={editor.editing.n}
          open={editor.editing.open}
          request={editor.editing.target}
          onClose={editor.close}
          onSaved={() => editor.close()}
        />
      )}
    </>
  );
}

/** "Als Eintrag übernehmen": a secure note with the message, the files as attachments. */
function TakeOver({
  open,
  request,
  submission,
  onClose,
  onDone,
}: {
  open: boolean;
  request: FileRequest;
  submission: Submission;
  onClose: () => void;
  onDone: (itemId: string) => Promise<void>;
}) {
  useLanguage();
  const [name, setName] = useState(() =>
    [request.title || request.label, submission.senderName].filter(Boolean).join(' – '),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const id = await takeOverSubmission(
        request.id,
        submission.id,
        name.trim(),
        t('Absender (nicht geprüft)'),
      );
      haptic('success');
      note(t('Als Eintrag übernommen ✧'), { tone: 'success' });
      await onDone(id);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <EditSurface
      open={open}
      onClose={onClose}
      title={t('Als Eintrag übernehmen')}
      action={{
        label: busy ? t('Übernimmt …') : t('Übernehmen'),
        onClick: () => void run(),
        disabled: busy || !name.trim(),
      }}
    >
      {error && (
        <p className="m-error" role="alert">
          {error}
        </p>
      )}
      <ListSection
        footer={t(
          'Eine sichere Notiz mit der Nachricht und dem Absender, die Dateien als Anhänge. Der Upload verschwindet danach aus der Anfrage.',
        )}
      >
        <FieldInput
          label={t('Name des Eintrags')}
          value={name}
          maxLength={200}
          onChange={setName}
        />
      </ListSection>
    </EditSurface>
  );
}

// ── The editor ───────────────────────────────────────────────────────────────

const DAYS = [1, 3, 7, 14, 30, 90];

function RequestEditor({
  open,
  request,
  onClose,
  onSaved,
}: {
  open: boolean;
  request: FileRequest | null;
  onClose: () => void;
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
    // A week, or the longest the server allows when that is shorter.
    days: request ? 0 : Math.min(7, DAYS.filter((n) => n <= maxDays).at(-1) ?? 1),
    maxSubmissions: request ? (request.maxSubmissions ?? 0) : 1,
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
  const [choosing, setChoosing] = useState<'days' | 'domain' | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const set = (patch: Partial<typeof form>) => {
    setDirty(true);
    setForm((f) => ({ ...f, ...patch }));
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    const input: FileRequestInput = {
      label: form.label.trim(),
      title: form.title.trim(),
      note: form.note.trim() || null,
      owner: form.owner.trim() || null,
      expiresInDays: form.days > 0 ? form.days : null,
      maxSubmissions: form.maxSubmissions > 0 ? form.maxSubmissions : null,
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
      haptic('success');
      note(request ? t('Gespeichert ✧') : t('Angelegt ✧'), { tone: 'success' });
      refreshUwu();
      await requestStore.load();
      onSaved(saved);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const dayOptions = [
    ...(request ? [{ value: 0, label: t('Datum behalten') }] : []),
    ...DAYS.filter((n) => n <= maxDays).map((n) => ({
      value: n,
      label: n === 1 ? t('1 Tag') : t('{n} Tage', { n }),
    })),
  ];
  const domainOptions = [
    { value: '', label: t('Hauptadresse des Servers') },
    ...uwu.sendDomains.map((d) => ({ value: d.id, label: d.url.replace(/^https?:\/\//, '') })),
  ];
  const ready = !busy && form.title.trim() !== '' && !(form.maxFiles === 0 && !form.textAllowed);

  return (
    <EditSurface
      open={open}
      onClose={onClose}
      title={request ? t('Dateianfrage ändern') : t('Neue Dateianfrage')}
      dirty={dirty}
      action={{
        label: busy ? t('Speichert …') : request ? t('Sichern') : t('Anlegen'),
        onClick: () => void save(),
        disabled: !ready,
      }}
    >
      {error && (
        <p className="m-error" role="alert">
          {error}
        </p>
      )}
      <ListSection>
        <FieldInput
          label={t('Dein Name dafür (nur für dich)')}
          value={form.label}
          maxLength={200}
          placeholder={form.title || t('z. B. Steuerunterlagen')}
          onChange={(label) => set({ label })}
        />
      </ListSection>

      <ListSection header={t('Was der Empfänger sieht')}>
        <FieldInput
          label={t('Titel für den Empfänger')}
          value={form.title}
          maxLength={200}
          placeholder={t('z. B. Scan des Reisepasses')}
          onChange={(title) => set({ title })}
        />
        <FieldInput
          label={t('Hinweis für den Empfänger (optional)')}
          value={form.note}
          multiline
          rows={2}
          onChange={(value) => set({ note: value })}
        />
        <FieldInput
          label={t('Dein Name, wie der Empfänger ihn sieht (optional)')}
          value={form.owner}
          maxLength={100}
          onChange={(owner) => set({ owner })}
        />
      </ListSection>

      <ListSection header={t('Regeln')}>
        <ListRow
          icon={ICONS.reminder}
          iconTone="neutral"
          title={t('Gültig für')}
          value={dayOptions.find((o) => o.value === form.days)?.label}
          onClick={() => setChoosing('days')}
        />
        <ListRow
          icon={ICONS.inbox}
          iconTone="neutral"
          title={t('Uploads höchstens')}
          value={form.maxSubmissions ? String(form.maxSubmissions) : t('bis zum Ablauf')}
          trailing={
            <Stepper
              label={t('Uploads höchstens')}
              value={form.maxSubmissions}
              min={0}
              max={100}
              onChange={(maxSubmissions) => set({ maxSubmissions })}
            />
          }
        />
        <ListRow
          icon={ICONS.files}
          iconTone="neutral"
          title={t('Dateien je Upload')}
          value={String(form.maxFiles)}
          trailing={
            <Stepper
              label={t('Dateien je Upload')}
              value={form.maxFiles}
              min={0}
              max={maxFiles}
              onChange={(n) => set({ maxFiles: n })}
            />
          }
        />
        <FieldInput
          label={t('Größe je Datei (MB)')}
          type="number"
          inputMode="numeric"
          value={form.maxFileMib}
          onChange={(value) => set({ maxFileMib: value.replace(/\D/g, '') })}
        />
        <ListRow
          title={t('Eine Nachricht erlauben')}
          trailing={
            <Toggle
              label={t('Eine Nachricht erlauben')}
              checked={form.textAllowed}
              onChange={(textAllowed) => set({ textAllowed })}
            />
          }
        />
      </ListSection>

      <ListSection>
        <FieldInput
          label={
            request?.passwordSet ? t('Neues Passwort (leer: bleibt)') : t('Passwort (optional)')
          }
          type="password"
          autoComplete="new-password"
          placeholder={request?.passwordSet ? undefined : t('Ohne Passwort')}
          value={form.password}
          disabled={form.removePassword}
          onChange={(password) => set({ password })}
        />
        {request?.passwordSet && (
          <ListRow
            title={t('Passwort entfernen')}
            trailing={
              <Toggle
                label={t('Passwort entfernen')}
                checked={form.removePassword}
                onChange={(removePassword) => set({ removePassword, password: '' })}
              />
            }
          />
        )}
        {uwu.sendDomains.length > 0 && (
          <ListRow
            icon={ICONS.website}
            iconTone="neutral"
            title={t('Link-Adresse')}
            value={domainOptions.find((o) => o.value === form.sendDomainId)?.label}
            onClick={() => setChoosing('domain')}
          />
        )}
      </ListSection>

      {request && (
        <ListSection>
          <ListRow
            title={t('Abschalten (der Link nimmt nichts mehr an)')}
            trailing={
              <Toggle
                label={t('Abschalten (der Link nimmt nichts mehr an)')}
                checked={form.disabled}
                onChange={(disabled) => set({ disabled })}
              />
            }
          />
          <ListRow
            title={t('Neuer Link – alle bisherigen Links funktionieren dann nicht mehr')}
            wrap
            trailing={
              <Toggle
                label={t('Neuer Link – alle bisherigen Links funktionieren dann nicht mehr')}
                checked={form.newLink}
                onChange={(newLink) => set({ newLink })}
              />
            }
          />
        </ListSection>
      )}

      <ChoiceSheet
        open={choosing === 'days'}
        onClose={() => setChoosing(null)}
        title={t('Gültig für')}
        options={dayOptions}
        value={form.days}
        onChange={(days) => set({ days })}
      />
      <ChoiceSheet
        open={choosing === 'domain'}
        onClose={() => setChoosing(null)}
        title={t('Link-Adresse')}
        options={domainOptions}
        value={form.sendDomainId}
        onChange={(sendDomainId) => set({ sendDomainId })}
      />
    </EditSurface>
  );
}
