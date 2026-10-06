/**
 * The password check on a phone and an iPad: the score with its headline,
 * "Passwörter durchgehen", the hints per group (each a list of the items it
 * is about), the addresses in breaches and what was ignored. The same answer
 * of Rust (`health_report`) as the desktop's HealthPane; the breach sources
 * are only asked again with the refresh button or a pull.
 */

import { listen } from '@tauri-apps/api/event';
import { ICONS, ListRow, ListSection, NavButton, type RowTone } from '@uwusuite/design';
import type { LucideIcon } from 'lucide-react';
import { useEffect, useState, useSyncExternalStore, type CSSProperties } from 'react';
import { errorText, toastError } from '../../lib/errors';
import { when } from '../../lib/format';
import {
  breachText,
  checkEmails,
  healthReport,
  problemTitle,
  progressText,
  setIgnored,
  type CheckProgress,
  type EmailResult,
  type HealthView,
  type ProblemKind,
} from '../../lib/health';
import { t, useLanguage } from '../../lib/i18n';
import { isIgnored } from '../../lib/review';
import { HealthReview } from '../../components/HealthPane';
import { ItemTile } from '../../components/ItemTile';
import { playNyu } from '../../components/nyu/stage';
import type { FindingGroup } from '../nav';
import { useMobile, useNav } from '../state';
import { BigButton, Empty, Page } from '../ui';
import { ItemRow } from './ItemRow';

// ── One report for every page of the tab ────────────────────────────────────

type HealthState = {
  view: HealthView | null;
  /** What the check is doing right now ("Prüft …", the progress), or null. */
  busy: string | null;
  error: string | null;
  /** The account the report belongs to. */
  account: string | null;
};

const EMPTY: HealthState = { view: null, busy: null, error: null, account: null };

// The check page, a group's list and the review sit side by side on an iPad
// and on top of each other on a phone: they share one report instead of
// asking Rust once each. It leaves memory when the last of them closes.
let health: HealthState = EMPTY;
let generation = 0;
let quietRunning = false;
const subscribers = new Set<() => void>();

function setHealth(patch: Partial<HealthState>) {
  health = { ...health, ...patch };
  for (const subscriber of subscribers) subscriber();
}

function subscribe(subscriber: () => void) {
  subscribers.add(subscriber);
  return () => {
    subscribers.delete(subscriber);
    if (!subscribers.size) {
      generation += 1;
      health = EMPTY;
    }
  };
}

/** `fresh`: ask the breach sources again; else their last answers count. */
async function runCheck(fresh: boolean) {
  const at = generation;
  setHealth({ error: null, busy: fresh ? t('Prüft …') : t('Lädt …') });
  try {
    const view = await healthReport(fresh);
    if (at !== generation) return;
    setHealth({ view });
    if (fresh) playNyu('checked');
  } catch (e) {
    if (at === generation) setHealth({ error: errorText(e) });
  } finally {
    if (at === generation) setHealth({ busy: null });
  }
}

/** A sync or a change elsewhere: the report follows, without asking the sources. */
async function refreshQuietly() {
  if (quietRunning) return;
  quietRunning = true;
  const at = generation;
  try {
    const view = await healthReport(false);
    if (at === generation) setHealth({ view });
  } catch {
    // The next change or a pull tries again.
  } finally {
    quietRunning = false;
  }
}

async function changeIgnore(itemId: string, kind: ProblemKind, ignored: boolean) {
  try {
    const list = await setIgnored(itemId, kind, ignored);
    if (health.view) setHealth({ view: { ...health.view, ignored: list } });
  } catch (e) {
    toastError(e);
  }
}

function useHealth(): HealthState {
  const { status } = useMobile();
  const state = useSyncExternalStore(subscribe, () => health);
  const account = status.accountId;

  // The first page of the tab loads the report; another account starts over.
  useEffect(() => {
    if (health.account === account && (health.view || health.busy)) return;
    generation += 1;
    health = { ...EMPTY, account };
    void runCheck(false);
  }, [account]);

  useEffect(() => {
    const changed = listen('vault-changed', () => void refreshQuietly());
    const progress = listen<CheckProgress>('health-progress', ({ payload }) => {
      if (health.busy) setHealth({ busy: progressText(payload) });
    });
    return () => {
      void changed.then((unlisten) => unlisten());
      void progress.then((unlisten) => unlisten());
    };
  }, []);

  return state;
}

// ── The groups ──────────────────────────────────────────────────────────────

type Entry = { id: string; name: string; kind?: ProblemKind };

type GroupInfo = {
  title: string;
  lead: string;
  icon: LucideIcon;
  tone: RowTone;
};

const ORDER: Exclude<FindingGroup, 'ignored'>[] = [
  'breached',
  'siteBreach',
  'reused',
  'weak',
  'unsecured',
  'twofa',
];

function groupInfo(group: FindingGroup): GroupInfo {
  switch (group) {
    case 'breached':
      return {
        title: t('In Datenlecks'),
        lead: t('Diese Passwörter tauchen in bekannten Datenlecks auf. Ändere sie zuerst.'),
        icon: ICONS.warning,
        tone: 'danger',
      };
    case 'siteBreach':
      return {
        title: t('Datenleck nach deiner letzten Passwortänderung'),
        lead: t(
          'Bei diesen Websites wurden Passwörter gestohlen, nachdem du deines zuletzt geändert hast. Ändere es dort.',
        ),
        icon: ICONS.error,
        tone: 'danger',
      };
    case 'reused':
      return {
        title: t('Mehrfach benutzt'),
        lead: t('Wird eines davon bekannt, sind die anderen Konten mit offen.'),
        icon: ICONS.repeat,
        tone: 'warning',
      };
    case 'weak':
      return {
        title: t('Schwach'),
        lead: t('Zu kurz oder zu leicht zu erraten.'),
        icon: ICONS.score,
        tone: 'warning',
      };
    case 'unsecured':
      return {
        title: t('Ohne https'),
        lead: t(
          'Die Adresse beginnt mit http://: Das Passwort geht unverschlüsselt über das Netz.',
        ),
        icon: ICONS.unlocked,
        tone: 'neutral',
      };
    case 'twofa':
      return {
        title: t('2FA möglich, nicht eingerichtet'),
        lead: t(
          'Diese Websites bieten Einmal-Codes aus einer Authenticator-App an, im Eintrag ist aber keiner hinterlegt. Richte die Zwei-Schritt-Anmeldung dort ein und trag den Schlüssel im Eintrag ein.',
        ),
        icon: ICONS.oneTimeCode,
        tone: 'neutral',
      };
    case 'ignored':
      return {
        title: t('Ignoriert'),
        lead: t('Diese Hinweise zeigt die Prüfung nicht mehr, auf keinem Gerät.'),
        icon: ICONS.hide,
        tone: 'neutral',
      };
  }
}

/** The items of each group, without what was ignored (as the desktop's report). */
function groupsOf(view: HealthView): Record<FindingGroup, Entry[]> {
  const ignored = view.ignored;
  const findings = view.report.findings;
  const pick = (kind: ProblemKind, test: (f: (typeof findings)[number]) => boolean): Entry[] =>
    findings
      .filter((f) => test(f) && !isIgnored(ignored, f.id, kind))
      .map((f) => ({ id: f.id, name: f.name }));
  const names = new Map(findings.map((f) => [f.id, f.name]));
  for (const m of view.twofa) names.set(m.itemId, m.name);
  return {
    breached: pick('breached', (f) => (f.breached ?? 0) > 0),
    siteBreach: pick('siteBreach', (f) => Boolean(view.siteBreaches[f.id])),
    reused: pick('reused', (f) => f.reused > 0),
    weak: pick('weak', (f) => f.weak),
    unsecured: pick('unsecured', (f) => f.unsecured),
    twofa: view.twofa
      .filter((m) => !isIgnored(ignored, m.itemId, 'twofa'))
      .map((m) => ({ id: m.itemId, name: m.name })),
    ignored: (ignored ?? [])
      .filter((entry) => names.has(entry.itemId))
      .map((entry) => ({
        id: entry.itemId,
        name: names.get(entry.itemId) ?? '',
        kind: entry.kind,
      })),
  };
}

/** Whether the server asks breach sources about passwords at all. */
const hasSources = (view: HealthView | null) =>
  Boolean(view && (view.switches.hibp || view.switches.xonPasswords));

/** Groups the check can fill on this server (the others would always be empty). */
function shownGroups(view: HealthView): Exclude<FindingGroup, 'ignored'>[] {
  return ORDER.filter((group) => {
    if (group === 'breached') return hasSources(view);
    if (group === 'siteBreach') return view.switches.siteBreaches;
    return true;
  });
}

/**
 * 0–100: the share of checked passwords without an open hint about the
 * password itself (2FA is about the site, not the password).
 */
function scoreOf(view: HealthView, groups: Record<FindingGroup, Entry[]>): number {
  const checked = view.report.checked;
  if (!checked) return 100;
  const affected = new Set(
    (['breached', 'siteBreach', 'reused', 'weak', 'unsecured'] as const).flatMap((group) =>
      groups[group].map((entry) => entry.id),
    ),
  );
  return Math.max(0, Math.round((100 * (checked - affected.size)) / checked));
}

function headline(score: number, open: number): string {
  if (!open) return t('Alles gut: nichts gefunden ✧');
  const tier = score >= 80 ? t('Gut') : score >= 50 ? t('Geht so') : t('Ausbaufähig');
  return open === 1
    ? t('{tier}, mit einer Baustelle', { tier })
    : t('{tier}, mit {n} Baustellen', { tier, n: open });
}

// ── The check ───────────────────────────────────────────────────────────────

export function CheckPage() {
  useLanguage();
  const nav = useNav();
  const { view, busy, error } = useHealth();
  const sources = hasSources(view);
  const groups = view ? groupsOf(view) : null;
  const shown = view ? shownGroups(view) : [];
  const open = groups ? shown.reduce((sum, group) => sum + groups[group].length, 0) : 0;
  const score = view && groups ? scoreOf(view, groups) : 0;
  const tone =
    score >= 80
      ? 'var(--uwu-success-ink)'
      : score >= 50
        ? 'var(--uwu-warning-ink)'
        : 'var(--uwu-danger-ink)';
  const selected = (group: FindingGroup) =>
    nav.column !== 'phone' && nav.selected?.page === 'findings' && nav.selected.group === group;
  const checkedAt = when(view?.checkedAt);
  const subtitle =
    busy ??
    (checkedAt
      ? t('Zuletzt geprüft {when}', { when: checkedAt })
      : view
        ? t('{n} Passwörter geprüft', { n: view.report.checked })
        : undefined);

  const breached = groups?.breached.length ?? 0;
  const lead =
    breached === 1
      ? t('Ein Passwort steckt in Datenlecks. Das geht zuerst.')
      : breached > 1
        ? t('{n} Passwörter stecken in Datenlecks. Die gehen zuerst.', { n: breached })
        : open
          ? t('Geh die Hinweise der Reihe nach durch.')
          : view
            ? t('{n} Passwörter geprüft', { n: view.report.checked })
            : '';

  return (
    <Page
      title={t('Prüfung')}
      largeTitle
      subtitle={subtitle}
      trailing={
        <NavButton
          label={sources && view?.checkedAt ? t('Nochmal prüfen') : t('Jetzt prüfen')}
          icon={ICONS.refresh}
          disabled={Boolean(busy)}
          onClick={() => void runCheck(true)}
        />
      }
      onRefresh={() => runCheck(sources)}
    >
      {!view ? (
        error ? (
          <Empty title={t('Die Prüfung hat nicht geklappt')}>
            <span role="alert">{error}</span>
          </Empty>
        ) : (
          <Empty title={busy ?? t('Lädt …')} />
        )
      ) : (
        <>
          <ListSection>
            <div className="m-score">
              <div
                className="m-score-ring"
                style={{ '--v': score / 100, '--m-score-tone': tone } as CSSProperties}
                role="img"
                aria-label={t('{n} von 100', { n: score })}
              >
                <b aria-hidden>{score}</b>
              </div>
              <div>
                <h3>{headline(score, open)}</h3>
                {lead && <p>{lead}</p>}
              </div>
            </div>
          </ListSection>
          {view.cards.length > 0 && (
            <div className="m-buttons">
              <BigButton
                icon={ICONS.start}
                disabled={Boolean(busy)}
                onClick={() => nav.open({ page: 'review' })}
              >
                {t('Passwörter durchgehen')}
              </BigButton>
            </div>
          )}
          {sources && !view.checkedAt && !busy && (
            <>
              <p className="m-footnote">
                {t(
                  'Findet schwache und doppelte Passwörter, Logins ohne https – und mit „Jetzt prüfen“ Passwörter aus bekannten Datenlecks. Dafür fragt dein Server Have I Been Pwned (die ersten fünf Zeichen eines SHA-1) und XposedOrNot (die ersten zehn Zeichen eines Keccak-512) nach dem Hash jedes Passworts; das Passwort selbst verlässt dieses Gerät nie. Dein Server sieht diese Zeichen dabei, schreibt sie aber nirgends auf.',
                )}
              </p>
              <div className="m-buttons">
                <BigButton soft icon={ICONS.securityCheck} onClick={() => void runCheck(true)}>
                  {t('Jetzt prüfen')}
                </BigButton>
              </div>
            </>
          )}
          {error && (
            <p className="m-footnote" role="alert">
              {error}
            </p>
          )}
          {view.report.breachesIncomplete && (
            <p className="m-footnote" role="alert">
              {t(
                'Eine Quelle für Datenlecks hat nicht für alle Passwörter geantwortet. Der Rest des Berichts stimmt; prüf später noch einmal.',
              )}
            </p>
          )}
          <ListSection header={t('Hinweise')}>
            {shown.map((group) => {
              const info = groupInfo(group);
              return (
                <ListRow
                  key={group}
                  icon={info.icon}
                  iconTone={info.tone}
                  title={info.title}
                  value={String(groups?.[group].length ?? 0)}
                  selected={selected(group)}
                  onClick={() => nav.open({ page: 'findings', group })}
                />
              );
            })}
          </ListSection>
          {view.switches.emailCheck && (
            <ListSection header={t('Deine Adressen in Datenlecks')}>
              <ListRow
                icon={ICONS.mail}
                iconTone="neutral"
                title={t('Adressen prüfen')}
                value={view.emailOptIn?.optedIn ? undefined : t('Aus')}
                selected={nav.column !== 'phone' && nav.selected?.page === 'emails'}
                onClick={() => nav.open({ page: 'emails' })}
              />
            </ListSection>
          )}
          {groups && groups.ignored.length > 0 && (
            <ListSection>
              <ListRow
                icon={ICONS.hide}
                iconTone="neutral"
                title={t('Ignoriert')}
                value={String(groups.ignored.length)}
                selected={selected('ignored')}
                onClick={() => nav.open({ page: 'findings', group: 'ignored' })}
              />
            </ListSection>
          )}
          {(view.twofaFailed || view.sitesFailed) && (
            <p className="m-footnote" role="status">
              {t(
                'Eine Liste von Websites war gerade nicht zu haben; dieser Teil fehlt im Bericht.',
              )}
            </p>
          )}
          <p className="m-footnote">
            {sources
              ? t(
                  'Schwache und doppelte Passwörter findet UwULock auf diesem Gerät. Für Datenlecks fragt dein Server nur nach dem Anfang eines Hashs (k-Anonymität); das Passwort verlässt dieses Gerät nie.',
                )
              : view.uwu
                ? t(
                    'Findet schwache und doppelte Passwörter und Logins ohne https. Den Abgleich mit Datenlecks hat die Verwaltung dieses Servers ausgeschaltet.',
                  )
                : t(
                    'Findet schwache und doppelte Passwörter und Logins ohne https – auf diesem Gerät. Datenlecks prüft UwULock über einen UwULock Server; dieser Server bietet das nicht an.',
                  )}
          </p>
        </>
      )}
    </Page>
  );
}

// ── One group ───────────────────────────────────────────────────────────────

export function FindingsPage({ group }: { group: FindingGroup }) {
  useLanguage();
  const { data } = useMobile();
  const nav = useNav();
  const { view, busy } = useHealth();
  const info = groupInfo(group);
  const entries = view ? groupsOf(view)[group] : [];

  const sourceNote =
    group === 'siteBreach' && view && view.siteSources.length > 0
      ? t('Listen der Datenlecks: {sources}', {
          sources: view.siteSources
            .map((s) => (s.license ? `${s.name} (${s.license})` : s.name))
            .join(', '),
        })
      : group === 'twofa' && view?.twofaSource
        ? t('Liste der Websites: {source}, {license}', {
            source: view.twofaSource.name,
            license: view.twofaSource.license ?? '',
          })
        : null;

  return (
    <Page title={info.title} onRefresh={refreshQuietly}>
      <p className="m-footnote m-lead">{info.lead}</p>
      {!view ? (
        <Empty title={busy ?? t('Lädt …')} />
      ) : !entries.length ? (
        <Empty title={t('Alles gut: nichts gefunden ✧')} />
      ) : group === 'ignored' ? (
        <ListSection>
          {entries.map((entry) => {
            const item = data.byId(entry.id);
            const kind = entry.kind as ProblemKind;
            return (
              <ListRow
                key={`${entry.id}:${kind}`}
                icon={item ? <ItemTile item={item} /> : ICONS.website}
                iconTone={item ? 'none' : 'neutral'}
                title={item?.name || entry.name || t('(ohne Namen)')}
                subtitle={problemTitle(kind)}
                onClick={item ? () => nav.open({ page: 'item', id: entry.id }) : undefined}
                trailing={
                  <button
                    type="button"
                    className="m-text-button"
                    onClick={() => void changeIgnore(entry.id, kind, false)}
                  >
                    {t('Rückgängig')}
                  </button>
                }
              />
            );
          })}
        </ListSection>
      ) : (
        <ListSection footer={sourceNote ?? undefined}>
          {entries.map((entry) => {
            const item = data.byId(entry.id);
            if (item) return <ItemRow key={entry.id} item={item} />;
            const breach = group === 'siteBreach' ? view.siteBreaches[entry.id] : undefined;
            return (
              <ListRow
                key={entry.id}
                icon={ICONS.website}
                iconTone="neutral"
                title={entry.name || t('(ohne Namen)')}
                subtitle={breach ? breachText(breach) : undefined}
              />
            );
          })}
        </ListSection>
      )}
    </Page>
  );
}

// ── The review ──────────────────────────────────────────────────────────────

/** One login at a time: the desktop's review, which already works on a narrow screen. */
export function ReviewPage() {
  useLanguage();
  const { data, status } = useMobile();
  const nav = useNav();
  const { view, busy } = useHealth();
  return (
    <Page title={t('Passwörter durchgehen')}>
      {view ? (
        <div className="m-review">
          <HealthReview
            key={status.accountId ?? ''}
            view={view}
            items={data.items}
            onBack={nav.back}
            onOpen={(id) => nav.open({ page: 'item', id })}
            onIgnore={changeIgnore}
            onRenewed={() => void refreshQuietly()}
            phone
          />
        </div>
      ) : (
        <Empty title={busy ?? t('Lädt …')} />
      )}
    </Page>
  );
}

// ── The addresses ───────────────────────────────────────────────────────────

/** The addresses at XposedOrNot: only with the account's consent, given in Settings → Konto. */
export function EmailsPage() {
  useLanguage();
  const { openInTab } = useMobile();
  const { view, busy: loading } = useHealth();
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<EmailResult[] | null>(null);
  const opted = view?.emailOptIn?.optedIn ?? false;

  // The consent may just have changed in Settings → Konto.
  useEffect(() => {
    void refreshQuietly();
  }, []);

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

  const resultText = (result: EmailResult) =>
    result.status === 'found'
      ? result.breaches.join(', ')
      : result.status === 'clean'
        ? t('In keinem bekannten Datenleck')
        : result.status === 'later'
          ? t('Später – das Kontingent des Servers ist gerade aufgebraucht')
          : t('XposedOrNot hat nicht geantwortet');

  return (
    <Page title={t('Deine Adressen in Datenlecks')}>
      {!view ? (
        <Empty title={loading ?? t('Lädt …')} />
      ) : opted ? (
        <>
          <ListSection
            footer={t(
              'Dein Server fragt XposedOrNot nach deiner Kontoadresse und den Adressen, die in Logins als Benutzername stehen. Dafür geht jede Adresse im Klartext an XposedOrNot; dein Server merkt sich die Antworten eine Woche lang, nur unter einem Hash der Adresse.',
            )}
          >
            <ListRow
              icon={ICONS.mail}
              iconTone="pink"
              title={busy ? t('Prüft Adressen …') : t('Adressen prüfen')}
              disabled={busy}
              chevron={false}
              onClick={() => void run()}
            />
          </ListSection>
          {results && (
            <ListSection header={t('Ergebnis')}>
              {results.map((result) => (
                <ListRow
                  key={result.email}
                  icon={result.status === 'found' ? ICONS.warning : ICONS.success}
                  iconTone={
                    result.status === 'found'
                      ? 'danger'
                      : result.status === 'clean'
                        ? 'success'
                        : 'neutral'
                  }
                  title={result.email}
                  subtitle={resultText(result)}
                  wrap
                />
              ))}
            </ListSection>
          )}
        </>
      ) : (
        <ListSection
          footer={t(
            'Auf Wunsch fragt dein Server XposedOrNot, ob deine Adressen in Datenlecks auftauchen. Einschalten kannst du das unter Einstellungen → Konto.',
          )}
        >
          <ListRow
            icon={ICONS.settings}
            iconTone="neutral"
            title={t('In den Einstellungen einschalten')}
            onClick={() => openInTab({ page: 'settings-page', section: 'account' })}
          />
        </ListSection>
      )}
    </Page>
  );
}
