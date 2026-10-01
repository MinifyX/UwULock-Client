import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { ItemSummary } from '../lib/api';
import { errorText, toastError } from '../lib/errors';
import {
  checkEmails,
  healthReport,
  openChangePage,
  problemDetail,
  problemTitle,
  saveNewPassword,
  setIgnored,
  breachText,
  type Card,
  type EmailResult,
  type Finding,
  type HealthView,
  type ProblemKind,
} from '../lib/health';
import { t, useLanguage } from '../lib/i18n';
import {
  aboutThePassword,
  dragStarts,
  dragTransform,
  isIgnored,
  keyStep,
  parseLater,
  progress,
  skipCard,
  step,
  swipeStep,
  withoutLater,
} from '../lib/review';
import { toast } from '../lib/toast';
import { GeneratorDialog } from './GeneratorDialog';
import { Icon } from './Icon';
import { ItemTile } from './ItemTile';

type Props = {
  /** `report` or `review`, chosen in the sidebar. */
  mode: 'report' | 'review';
  onMode: (mode: 'report' | 'review') => void;
  items: ItemSummary[];
  onOpen: (id: string) => void;
  /** Phone layout: a bar with the drawer's button on top. */
  phone: boolean;
  onMenu: () => void;
};

/**
 * The password check: the report with its groups, and the review one login at
 * a time. Both come from the same answer of Rust (`health_report`); the
 * breach sources are only asked again with "Check again" — otherwise their
 * last answers count for passwords that haven't changed since.
 */
export function HealthPane({ mode, onMode, items, onOpen, phone, onMenu }: Props) {
  useLanguage();
  const [view, setView] = useState<HealthView | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (fresh: boolean) => {
    setError(null);
    setBusy(fresh ? t('Prüft …') : t('Lädt …'));
    try {
      setView(await healthReport(fresh));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  }, []);

  useEffect(() => {
    void load(false);
  }, [load]);

  // A sync or a change elsewhere: the report follows, without asking the sources.
  useEffect(() => {
    const stop = listen('vault-changed', () => {
      void healthReport(false).then(setView, () => undefined);
    });
    return () => void stop.then((unlisten) => unlisten());
  }, []);

  useEffect(() => {
    const stop = listen<{ done: number; total: number }>('health-progress', ({ payload }) => {
      setBusy((now) => (now ? t('Fragt nach Datenlecks … {done} von {total}', payload) : now));
    });
    return () => void stop.then((unlisten) => unlisten());
  }, []);

  const changeIgnore = async (id: string, kind: ProblemKind, on: boolean) => {
    try {
      const ignored = await setIgnored(id, kind, on);
      setView((now) => (now ? { ...now, ignored } : now));
    } catch (e) {
      toastError(e);
    }
  };

  const bar = phone && (
    <div className="detail-back report-bar">
      {mode === 'review' ? (
        <button className="quiet" onClick={() => onMode('report')}>
          <Icon name="back" size={16} />
          {t('Zum Bericht')}
        </button>
      ) : (
        <button className="quiet" aria-label={t('Ordner und Typen')} onClick={onMenu}>
          <Icon name="menu" size={16} />
          {t('Passwortprüfung')}
        </button>
      )}
    </div>
  );

  return (
    <section
      className="report-pane"
      aria-label={mode === 'review' ? t('Passwörter durchgehen') : t('Passwortprüfung')}
    >
      {bar}
      {mode === 'review' && view ? (
        <HealthReview
          view={view}
          items={items}
          onBack={() => onMode('report')}
          onOpen={onOpen}
          onIgnore={changeIgnore}
          onRenewed={() => void healthReport(false).then(setView, () => undefined)}
          phone={phone}
        />
      ) : (
        <HealthReport
          view={view}
          busy={busy}
          error={error}
          onRun={() => void load(true)}
          onReview={() => onMode('review')}
          onOpen={onOpen}
          onIgnore={changeIgnore}
        />
      )}
    </section>
  );
}

// ── The report ─────────────────────────────────────────────

function HealthReport({
  view,
  busy,
  error,
  onRun,
  onReview,
  onOpen,
  onIgnore,
}: {
  view: HealthView | null;
  busy: string | null;
  error: string | null;
  onRun: () => void;
  onReview: () => void;
  onOpen: (id: string) => void;
  onIgnore: (id: string, kind: ProblemKind, on: boolean) => Promise<void>;
}) {
  useLanguage();
  const ignored = view?.ignored ?? null;
  const shown = (kind: ProblemKind) => (f: { id: string }) => !isIgnored(ignored, f.id, kind);
  const findings = view?.report.findings ?? [];
  const breached = findings.filter((f) => (f.breached ?? 0) > 0).filter(shown('breached'));
  const siteBreached = findings.filter((f) => view?.siteBreaches[f.id]).filter(shown('siteBreach'));
  const reused = findings.filter((f) => f.reused > 0).filter(shown('reused'));
  const weak = findings.filter((f) => f.weak).filter(shown('weak'));
  const unsecured = findings.filter((f) => f.unsecured).filter(shown('unsecured'));
  const missing = (view?.twofa ?? []).filter((m) => !isIgnored(ignored, m.itemId, 'twofa'));
  const names = new Map(findings.map((f) => [f.id, f.name]));
  for (const m of view?.twofa ?? []) names.set(m.itemId, m.name);
  const ignoredShown = (ignored ?? []).filter((entry) => names.has(entry.itemId));
  const problems =
    breached.length +
    siteBreached.length +
    reused.length +
    weak.length +
    unsecured.length +
    missing.length;
  const sources = view && (view.switches.hibp || view.switches.xonPasswords);
  const checkedOnce = Boolean(view?.checkedAt) || !sources;

  const group = (
    title: string,
    lead: string,
    list: Finding[],
    detail: (f: Finding) => string,
    testId: string,
  ) =>
    list.length > 0 && (
      <section className="detail-card" data-testid={testId}>
        <h3 className="detail-card-title">
          {title} <span className="nav-count">{list.length}</span>
        </h3>
        <p className="field-hint report-lead">{lead}</p>
        {list.map((finding) => (
          <div className="detail-row" key={finding.id}>
            <div className="detail-text">
              <span className="detail-value">{finding.name || t('(ohne Namen)')}</span>
              <span className="detail-label">
                {[finding.subtitle, detail(finding)].filter(Boolean).join(' · ')}
              </span>
            </div>
            <div className="detail-actions">
              <button className="quiet" onClick={() => onOpen(finding.id)}>
                {t('Öffnen')}
              </button>
            </div>
          </div>
        ))}
      </section>
    );

  return (
    <article className="detail report">
      <header className="detail-head">
        <span className="item-tile" data-size="large" data-hue="4" aria-hidden>
          <Icon name="shield" size={24} />
        </span>
        <div className="detail-title">
          <h2>{t('Passwortprüfung')}</h2>
          <p className="chips">
            {view && (
              <span className="chip">
                {t('{n} Passwörter geprüft', { n: view.report.checked })}
              </span>
            )}
            {view?.checkedAt && (
              <span className="chip">
                {t('Stand: {when}', { when: new Date(view.checkedAt).toLocaleString() })}
              </span>
            )}
          </p>
        </div>
        <div className="detail-tools">
          {view && view.cards.length > 0 && (
            <button disabled={Boolean(busy)} onClick={onReview} data-testid="review-start">
              <Icon name="layers" size={15} />
              {t('Durchgehen')}
            </button>
          )}
          {sources && (
            <button className="primary" disabled={Boolean(busy)} onClick={onRun}>
              <Icon name="refresh" size={15} />
              {view?.checkedAt ? t('Nochmal prüfen') : t('Jetzt prüfen')}
            </button>
          )}
        </div>
      </header>
      {view && !view.checkedAt && !busy && (
        <p className="dialog-lead">
          {sources
            ? t(
                'Findet schwache und doppelte Passwörter, Logins ohne https – und mit „Jetzt prüfen“ Passwörter aus bekannten Datenlecks. Dafür fragt dein Server Have I Been Pwned (die ersten fünf Zeichen eines SHA-1) und XposedOrNot (die ersten zehn Zeichen eines Keccak-512) nach dem Hash jedes Passworts; das Passwort selbst verlässt dieses Gerät nie. Dein Server sieht diese Zeichen dabei, schreibt sie aber nirgends auf.',
              )
            : view.uwu
              ? t(
                  'Findet schwache und doppelte Passwörter und Logins ohne https. Den Abgleich mit Datenlecks hat die Verwaltung dieses Servers ausgeschaltet.',
                )
              : t(
                  'Findet schwache und doppelte Passwörter und Logins ohne https – auf diesem Gerät. Datenlecks prüft UwULock über einen UwULock Server; dieser Server bietet das nicht an.',
                )}
        </p>
      )}
      {busy && (
        <p className="dialog-lead" role="status">
          {busy}
        </p>
      )}
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      {view?.report.breachesIncomplete && (
        <p className="form-error" role="alert">
          {t(
            'Eine Quelle für Datenlecks hat nicht für alle Passwörter geantwortet. Der Rest des Berichts stimmt; prüf später noch einmal.',
          )}
        </p>
      )}
      {view && !problems && checkedOnce && !busy && (
        <p className="dialog-lead">{t('Alles gut: nichts gefunden ✧')}</p>
      )}
      {group(
        t('In Datenlecks'),
        t('Diese Passwörter tauchen in bekannten Datenlecks auf. Ändere sie zuerst.'),
        breached,
        (f) => t('{n} Mal gesehen', { n: (f.breached ?? 0).toLocaleString() }),
        'group-breached',
      )}
      {group(
        t('Datenleck nach deiner letzten Passwortänderung'),
        t(
          'Bei diesen Websites wurden Passwörter gestohlen, nachdem du deines zuletzt geändert hast. Ändere es dort.',
        ),
        siteBreached,
        (f) => {
          const breach = view?.siteBreaches[f.id];
          return breach ? breachText(breach) : '';
        },
        'group-site',
      )}
      {siteBreached.length > 0 && view && view.siteSources.length > 0 && (
        <p className="field-hint report-source">
          {t('Listen der Datenlecks: {sources}', {
            sources: view.siteSources
              .map((s) => (s.license ? `${s.name} (${s.license})` : s.name))
              .join(', '),
          })}
        </p>
      )}
      {group(
        t('Mehrfach benutzt'),
        t('Wird eines davon bekannt, sind die anderen Konten mit offen.'),
        reused,
        (f) => t('noch {n} Mal im Tresor', { n: f.reused }),
        'group-reused',
      )}
      {group(
        t('Schwach'),
        t('Zu kurz oder zu leicht zu erraten.'),
        weak,
        (f) => t('{bits} Bit', { bits: f.bits }),
        'group-weak',
      )}
      {group(
        t('Ohne https'),
        t('Die Adresse beginnt mit http://: Das Passwort geht unverschlüsselt über das Netz.'),
        unsecured,
        () => '',
        'group-unsecured',
      )}
      {missing.length > 0 && (
        <section className="detail-card" data-testid="group-twofa">
          <h3 className="detail-card-title">
            {t('2FA möglich, nicht eingerichtet')}{' '}
            <span className="nav-count">{missing.length}</span>
          </h3>
          <p className="field-hint report-lead">
            {t(
              'Diese Websites bieten Einmal-Codes aus einer Authenticator-App an, im Eintrag ist aber keiner hinterlegt. Richte die Zwei-Schritt-Anmeldung dort ein und trag den Schlüssel im Eintrag ein.',
            )}
          </p>
          {missing.map((m) => (
            <div className="detail-row" key={m.itemId}>
              <div className="detail-text">
                <span className="detail-value">{m.name || t('(ohne Namen)')}</span>
                <span className="detail-label">
                  {[m.host, m.entry.name !== m.name ? m.entry.name : null]
                    .filter(Boolean)
                    .join(' · ')}
                </span>
              </div>
              <div className="detail-actions">
                <button className="quiet" onClick={() => onOpen(m.itemId)}>
                  {t('Öffnen')}
                </button>
              </div>
            </div>
          ))}
          {view?.twofaSource && (
            <p className="field-hint report-lead">
              {t('Liste der Websites: {source}, {license}', {
                source: view.twofaSource.name,
                license: view.twofaSource.license ?? '',
              })}
            </p>
          )}
        </section>
      )}
      {view?.switches.emailCheck && <EmailCheck view={view} />}
      {ignoredShown.length > 0 && (
        <section className="detail-card" data-testid="ignored">
          <h3 className="detail-card-title">
            {t('Ignoriert')} <span className="nav-count">{ignoredShown.length}</span>
          </h3>
          <p className="field-hint report-lead">
            {t('Diese Hinweise zeigt die Prüfung nicht mehr, auf keinem Gerät.')}
          </p>
          {ignoredShown.map((entry) => (
            <div className="detail-row" key={`${entry.itemId}:${entry.kind}`}>
              <div className="detail-text">
                <span className="detail-value">{names.get(entry.itemId) || t('(ohne Namen)')}</span>
                <span className="detail-label">{problemTitle(entry.kind)}</span>
              </div>
              <div className="detail-actions">
                <button
                  className="quiet"
                  onClick={() => void onIgnore(entry.itemId, entry.kind, false)}
                >
                  {t('Rückgängig')}
                </button>
              </div>
            </div>
          ))}
        </section>
      )}
      {view && (view.twofaFailed || view.sitesFailed) && (
        <p className="field-hint" role="status">
          {t('Eine Liste von Websites war gerade nicht zu haben; dieser Teil fehlt im Bericht.')}
        </p>
      )}
    </article>
  );
}

/** The addresses: only with the account's consent, given in the settings. */
function EmailCheck({ view }: { view: HealthView }) {
  useLanguage();
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<EmailResult[] | null>(null);
  const opted = view.emailOptIn?.optedIn ?? false;
  const run = async () => {
    setBusy(true);
    try {
      setResults((await checkEmails()).results);
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="detail-card" data-testid="email-check">
      <h3 className="detail-card-title">{t('Deine Adressen in Datenlecks')}</h3>
      <p className="field-hint report-lead">
        {opted
          ? t(
              'Dein Server fragt XposedOrNot nach deiner Kontoadresse und den Adressen, die in Logins als Benutzername stehen. Dafür geht jede Adresse im Klartext an XposedOrNot; dein Server merkt sich die Antworten eine Woche lang, nur unter einem Hash der Adresse.',
            )
          : t(
              'Auf Wunsch fragt dein Server XposedOrNot, ob deine Adressen in Datenlecks auftauchen. Einschalten kannst du das unter Einstellungen → Konto.',
            )}
      </p>
      {opted && (
        <div className="detail-row">
          <span className="spacer" />
          <button disabled={busy} onClick={() => void run()}>
            {busy ? t('Prüft Adressen …') : t('Adressen prüfen')}
          </button>
        </div>
      )}
      {results?.map((result) => (
        <div className="detail-row" key={result.email}>
          <div className="detail-text">
            <span className="detail-value">{result.email}</span>
            <span className="detail-label">
              {result.status === 'found'
                ? result.breaches.join(', ')
                : result.status === 'clean'
                  ? t('In keinem bekannten Datenleck')
                  : result.status === 'later'
                    ? t('Später – das Kontingent des Servers ist gerade aufgebraucht')
                    : t('XposedOrNot hat nicht geantwortet')}
            </span>
          </div>
        </div>
      ))}
    </section>
  );
}

// ── The review ─────────────────────────────────────────────

/** Logins put off with "Later" stay put off while the app runs. */
const LATER_KEY = 'uwulock.review.later';

function laterIds(): Set<string> {
  try {
    return parseLater(window.sessionStorage.getItem(LATER_KEY));
  } catch {
    return new Set();
  }
}

function keepLater(ids: Set<string>) {
  try {
    window.sessionStorage.setItem(LATER_KEY, JSON.stringify([...ids]));
  } catch {
    // Only a convenience.
  }
}

/**
 * One card per login with a problem, swiped (or paged with ← →) back and
 * forth. The stack is laid once: ignoring or fixing changes a card, it
 * doesn't reshuffle the rest.
 */
function HealthReview({
  view,
  items,
  onBack,
  onOpen,
  onIgnore,
  onRenewed,
  phone,
}: {
  view: HealthView;
  items: ItemSummary[];
  onBack: () => void;
  onOpen: (id: string) => void;
  onIgnore: (id: string, kind: ProblemKind, on: boolean) => Promise<void>;
  onRenewed: () => void;
  phone: boolean;
}) {
  useLanguage();
  const [cards, setCards] = useState<Card[]>(() => withoutLater(view.cards, laterIds()));
  const [index, setIndex] = useState(0);
  const [generating, setGenerating] = useState<string | null>(null);
  const [renewed, setRenewed] = useState<Set<string>>(new Set());
  const total = cards.length;
  const card = cards[index] ?? null;
  const summary = card ? items.find((item) => item.id === card.finding.id) : undefined;
  const at = progress(index, total);

  const go = useCallback((delta: number) => setIndex((i) => step(i, delta, total)), [total]);

  // The arrow keys, while no field and no dialog has them.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.altKey || event.ctrlKey || event.metaKey || generating) return;
      const target = event.target as HTMLElement | null;
      if (target?.closest('input, textarea, select, .modal')) return;
      const delta = keyStep(event.key);
      if (!delta) return;
      event.preventDefault();
      go(delta);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [go, generating]);

  // ── Swiping ──
  const drag = useRef<{ x: number; y: number; id: number; moved: boolean } | null>(null);
  const [dx, setDx] = useState(0);
  const onPointerDown = (event: React.PointerEvent) => {
    if (event.button !== 0) return;
    drag.current = { x: event.clientX, y: event.clientY, id: event.pointerId, moved: false };
  };
  const onPointerMove = (event: React.PointerEvent) => {
    const start = drag.current;
    if (!start || start.id !== event.pointerId) return;
    const x = event.clientX - start.x;
    if (!start.moved && dragStarts(x, event.clientY - start.y)) {
      start.moved = true;
      (event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId);
    }
    if (start.moved) setDx(x);
  };
  const onPointerUp = (event: React.PointerEvent) => {
    const start = drag.current;
    drag.current = null;
    if (!start?.moved) return;
    setDx(0);
    const delta = swipeStep(event.clientX - start.x);
    if (delta) go(delta);
  };
  // A drag is not a click on the button it began on.
  const onClickCapture = (event: React.MouseEvent) => {
    if (dx !== 0) event.stopPropagation();
  };

  const later = () => {
    const next = skipCard(cards, index);
    if (!next.skipped) return;
    const ids = laterIds();
    ids.add(next.skipped);
    keepLater(ids);
    setCards(next.cards);
    setIndex(next.index);
  };

  const saveNew = async (password: string) => {
    const id = generating;
    setGenerating(null);
    if (!id) return;
    try {
      await saveNewPassword(id, password);
      setRenewed((done) => new Set(done).add(id));
      toast(t('Neues Passwort gespeichert; das alte steht im Verlauf des Eintrags.'));
      onRenewed();
    } catch (e) {
      toastError(e);
    }
  };

  return (
    <article className="detail review">
      <header className="detail-head">
        <span className="item-tile" data-size="large" data-hue="4" aria-hidden>
          <Icon name="layers" size={24} />
        </span>
        <div className="detail-title">
          <h2>{t('Passwörter durchgehen')}</h2>
          <p className="chips">
            {total > 0 && (
              <span className="chip" aria-live="polite" data-testid="review-progress">
                {t('{n} von {total}', at)}
              </span>
            )}
          </p>
        </div>
        <div className="detail-tools review-back">
          <button className="quiet" onClick={onBack}>
            {t('Zum Bericht')}
          </button>
        </div>
      </header>
      {total === 0 && (
        <p className="dialog-lead">
          {t('Nichts mehr durchzugehen ✧ Ignoriertes findest du im Bericht.')}
        </p>
      )}
      {card && (
        <>
          <p className="field-hint review-hint">
            {phone ? t('Zum Blättern wischen.') : t('Wischen oder Pfeiltasten ← → blättern.')}
          </p>
          <div
            className="review-stack"
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={() => {
              drag.current = null;
              setDx(0);
            }}
            onClickCapture={onClickCapture}
          >
            {index + 1 < total && <div className="review-card review-card-behind" aria-hidden />}
            <section
              className="review-card"
              key={card.finding.id}
              aria-roledescription={t('Karte')}
              aria-label={card.finding.name || t('(ohne Namen)')}
              data-testid="review-card"
              style={dx ? { transform: dragTransform(dx), transition: 'none' } : undefined}
            >
              <header className="review-card-head">
                {summary ? (
                  <ItemTile item={summary} size="large" />
                ) : (
                  <span className="item-tile" data-size="large" data-hue="1" aria-hidden>
                    <Icon name="globe" size={22} />
                  </span>
                )}
                <div className="detail-title">
                  <h3>{card.finding.name || t('(ohne Namen)')}</h3>
                  <p className="detail-label">
                    {[card.finding.subtitle, card.finding.host].filter(Boolean).join(' · ')}
                  </p>
                </div>
              </header>
              <ul className="review-problems">
                {card.problems.map((problem) => {
                  const ignored = isIgnored(view.ignored, card.finding.id, problem.kind);
                  const solved = renewed.has(card.finding.id) && aboutThePassword(problem.kind);
                  return (
                    <li
                      key={problem.kind}
                      className="review-problem"
                      data-state={solved ? 'solved' : ignored ? 'ignored' : 'open'}
                    >
                      <Icon name={solved ? 'check' : ignored ? 'eyeOff' : 'warning'} size={16} />
                      <div className="detail-text">
                        <span className="detail-value">{problemTitle(problem.kind)}</span>
                        <span className="detail-label">
                          {solved
                            ? t('Neues Passwort gespeichert')
                            : ignored
                              ? t('Ignoriert')
                              : problemDetail(problem)}
                        </span>
                      </div>
                      {!solved && view.ignored && (
                        <button
                          className="quiet small"
                          onClick={() => void onIgnore(card.finding.id, problem.kind, !ignored)}
                        >
                          {ignored ? t('Rückgängig') : t('Ignorieren')}
                          <span className="sr-only">: {problemTitle(problem.kind)}</span>
                        </button>
                      )}
                    </li>
                  );
                })}
              </ul>
              {renewed.has(card.finding.id) && (
                <p className="field-hint" role="status">
                  {t('Neues Passwort gespeichert – jetzt noch auf der Website ändern.')}
                </p>
              )}
              <div className="review-actions">
                {(card.finding.host || card.finding.uri) && (
                  <button
                    className="primary"
                    onClick={() => void openChangePage(card.finding.id).catch(toastError)}
                  >
                    <Icon name="external" size={15} />
                    {t('Seite öffnen & Passwort ändern')}
                  </button>
                )}
                <button onClick={() => setGenerating(card.finding.id)}>
                  <Icon name="key" size={15} />
                  {t('Neues Passwort erzeugen & speichern')}
                </button>
                <button className="quiet" onClick={later}>
                  <Icon name="clock" size={15} />
                  {t('Später')}
                </button>
                <button className="quiet" onClick={() => onOpen(card.finding.id)}>
                  {t('Eintrag öffnen')}
                </button>
              </div>
            </section>
          </div>
          <nav className="review-nav" aria-label={t('Karten')}>
            <button className="quiet" disabled={index === 0} onClick={() => go(-1)}>
              <span aria-hidden>‹</span> {t('Zurück')}
            </button>
            <button className="quiet" disabled={index + 1 >= total} onClick={() => go(1)}>
              {t('Weiter')} <span aria-hidden>›</span>
            </button>
          </nav>
        </>
      )}
      {generating && (
        <GeneratorDialog onClose={() => setGenerating(null)} onUse={(pw) => void saveNew(pw)} />
      )}
    </article>
  );
}
