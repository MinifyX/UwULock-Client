/**
 * "Als Send teilen" on a phone or iPad: which values of an item go into a
 * Send, how long it lives and who may open it; afterwards the link to copy
 * or share, in the same sheet. The logic is the desktop's `ShareSendDialog`
 * (ItemExtras.tsx), its list of values `choices()`.
 */

import { haptic, ICONS, ListRow, ListSection, Stepper } from '@uwusuite/design';
import { useEffect, useState } from 'react';
import { copyGenerated, vaultItem, type ItemDetail } from '../../lib/api';
import { errorText } from '../../lib/errors';
import { when } from '../../lib/format';
import { t, useLanguage } from '../../lib/i18n';
import { note } from '../../lib/toast';
import { sendOptions, shareAsSend, useUwu, type SendOptions } from '../../lib/uwu';
import { ItemTile } from '../../components/ItemTile';
import { choices } from '../../components/ItemExtras';
import { playNyu } from '../../components/nyu/stage';
import { copyLink, sendStore, shareLink } from '../pages/Sends';
import { useMobile } from '../state';
import { ChoiceSheet, EditSurface, Empty, FieldInput, LinkBox, Toggle, useConfirm } from '../ui';

type Choice = ReturnType<typeof choices>[number];

const DAYS = [1, 2, 3, 7, 14, 30];

const TOTP_WARNING =
  'Der Schlüssel des Einmal-Codes reist verschlüsselt im Send mit. Die Send-Seite zeigt nur die laufenden Codes – wer den Link hat, kann den Schlüssel aber auslesen und damit auch nach dem Löschen des Sends weiter Codes erzeugen.';

export function ShareSheet({
  id,
  open = true,
  onClose,
}: {
  id: string;
  open?: boolean;
  onClose: () => void;
}) {
  useLanguage();
  const { data } = useMobile();
  const summary = data.byId(id);
  // UwULock Server's Send page shows an entry Send as the entry, with live codes.
  const entry = useUwu().uwu;
  const confirm = useConfirm();
  const [detail, setDetail] = useState<ItemDetail | null>(null);
  const [fields, setFields] = useState<Choice[] | null>(null);
  const [options, setOptions] = useState<SendOptions | null>(null);
  const [days, setDays] = useState(1);
  const [maxAccess, setMaxAccess] = useState(1);
  const [password, setPassword] = useState('');
  const [onlyFor, setOnlyFor] = useState(false);
  const [emails, setEmails] = useState('');
  const [domain, setDomain] = useState('');
  const [hideText, setHideText] = useState(true);
  const [choosing, setChoosing] = useState<'days' | 'domain' | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [made, setMade] = useState<{ link: string; deletionDate: string } | null>(null);

  useEffect(() => {
    let gone = false;
    vaultItem(id).then(
      (found) => {
        if (!gone) setDetail(found);
      },
      (e) => {
        if (!gone) setError(errorText(e));
      },
    );
    sendOptions().then(
      (next) => {
        if (gone) return;
        setOptions(next);
        setDomain(next.defaultDomainId ?? '');
      },
      () => {
        if (!gone) setOptions({ emails: false, domains: [], defaultDomainId: null });
      },
    );
    return () => {
      gone = true;
    };
  }, [id]);

  // The values to tick, once the item and whether its passwords may be seen are known.
  useEffect(() => {
    if (detail && summary && fields === null)
      setFields(choices(detail, summary.viewPassword, entry));
  }, [detail, summary, entry, fields]);

  const tick = (name: string, checked: boolean) =>
    setFields((current) => (current ?? []).map((f) => (f.field === name ? { ...f, checked } : f)));

  const addresses = emails
    .split(/[\s,;]+/)
    .map((e) => e.trim())
    .filter(Boolean);
  const chosen = (fields ?? []).filter((f) => f.checked);

  const create = async () => {
    if (!summary) return;
    setBusy(true);
    setError(null);
    try {
      const result = await shareAsSend(summary.id, {
        fields: chosen.map((f) => [f.field, f.label]),
        deletionDays: days,
        maxAccess: maxAccess > 0 ? maxAccess : null,
        password: onlyFor ? null : password || null,
        emails: onlyFor ? addresses : [],
        sendDomainId: domain || null,
        hideText,
        entry,
      });
      setMade(result);
      playNyu('shared');
      haptic('success');
      // The link is what the person wants next; the new Send also shows under Sends.
      const copied = await copyGenerated(result.link).then(
        () => true,
        () => false,
      );
      note(copied ? t('Send angelegt – der Link ist kopiert ✧') : t('Send angelegt ✧'), {
        tone: 'success',
        detail: t('Liegt jetzt unter Extras › Sends'),
      });
      void sendStore.load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const dayOptions = DAYS.map((n) => ({
    value: n,
    label: n === 1 ? t('1 Tag') : t('{n} Tagen', { n }),
  }));
  const domainOptions = [
    { value: '', label: t('Hauptadresse des Servers') },
    ...(options?.domains ?? []).map((d) => ({
      value: d.id,
      label: d.url.replace(/^https?:\/\//, ''),
    })),
  ];
  const totpTicked = chosen.some((f) => f.field === 'totp');

  return (
    <EditSurface
      open={open}
      onClose={onClose}
      title={t('Als Send teilen')}
      dirty={!made && (password !== '' || emails !== '')}
      action={
        made
          ? { label: t('Fertig'), onClick: onClose }
          : {
              label: busy ? t('Erstellt …') : t('Anlegen'),
              onClick: () => void create(),
              disabled:
                busy ||
                !options ||
                !summary ||
                chosen.length === 0 ||
                (onlyFor && addresses.length === 0),
            }
      }
    >
      {summary && (
        <ListSection>
          <div className="m-icon-head">
            <ItemTile item={summary} />
            <span className="m-icon-head-text">
              <b>{summary.name || t('(ohne Namen)')}</b>
              <span>{entry ? t('Wird als Eintrag geteilt') : t('Wird als Text-Send geteilt')}</span>
            </span>
          </div>
        </ListSection>
      )}
      {error && (
        <p className="m-error" role="alert">
          {error}
        </p>
      )}

      {made ? (
        <ListSection
          header={t('Link')}
          footer={t('Wer diesen Link hat, kann die gewählten Werte ansehen – bis {when}.', {
            when: when(made.deletionDate) ?? '',
          })}
        >
          <LinkBox
            url={made.link}
            onCopy={() => void copyLink(made.link)}
            onShare={() => void shareLink(made.link)}
          />
        </ListSection>
      ) : (
        <>
          <ListSection
            header={t('Was mitgeht')}
            footer={
              totpTicked
                ? t(TOTP_WARNING)
                : entry
                  ? t(
                      'Die gewählten Werte gehen verschlüsselt an einen Send. Der Schlüssel steckt im Link, nicht auf dem Server. Die Send-Seite zeigt sie als Eintrag mit Kopier-Knöpfen.',
                    )
                  : t(
                      'Die gewählten Werte gehen verschlüsselt an einen Send. Der Schlüssel steckt im Link, nicht auf dem Server. Der Einmal-Code-Schlüssel wird nie geteilt.',
                    )
            }
          >
            {fields === null ? (
              <Empty title={t('Einen Moment …')} />
            ) : fields.length === 0 ? (
              <Empty title={t('Dieser Eintrag hat nichts zu teilen.')} />
            ) : (
              fields.map((field) => (
                <ListRow
                  key={field.field}
                  title={field.shown ?? field.label}
                  subtitle={field.shown ? field.label : undefined}
                  trailing={
                    <span className="m-tick" data-on={field.checked || undefined} aria-hidden>
                      <ICONS.done strokeWidth={3} />
                    </span>
                  }
                  icon={field.secret ? ICONS.masterPassword : undefined}
                  iconTone="neutral"
                  chevron={false}
                  aria-pressed={field.checked}
                  onClick={() => {
                    haptic('selection');
                    // Ticking the one-time code hands out its key for good: asked once more.
                    if (field.field === 'totp' && !field.checked)
                      confirm.ask({
                        title: t('Schlüssel mitgeben'),
                        text: `${t(TOTP_WARNING)} ${t('Teile ihn nur, wenn das okay ist.')}`,
                        confirm: t('Schlüssel mitgeben'),
                        run: () => tick('totp', true),
                      });
                    else tick(field.field, !field.checked);
                  }}
                />
              ))
            )}
          </ListSection>

          <ListSection header={t('Gültigkeit')}>
            <ListRow
              icon={ICONS.delete}
              iconTone="neutral"
              title={t('Löschen nach')}
              value={dayOptions.find((o) => o.value === days)?.label}
              onClick={() => setChoosing('days')}
            />
            <ListRow
              icon={ICONS.show}
              iconTone="neutral"
              title={t('Höchstens so oft öffnen')}
              value={maxAccess ? String(maxAccess) : t('unbegrenzt')}
              trailing={
                <Stepper
                  label={t('Höchstens so oft öffnen')}
                  value={maxAccess}
                  min={0}
                  max={1000}
                  onChange={setMaxAccess}
                />
              }
            />
          </ListSection>

          <ListSection
            footer={t('Wer den Link hat, sieht diese Felder. Teile ihn nur, wenn das okay ist.')}
          >
            {options?.emails && (
              <ListRow
                title={t('Nur für diese Adressen (mit Code per E-Mail)')}
                wrap
                trailing={
                  <Toggle
                    label={t('Nur für diese Adressen (mit Code per E-Mail)')}
                    checked={onlyFor}
                    onChange={setOnlyFor}
                  />
                }
              />
            )}
            {onlyFor ? (
              <FieldInput
                label={t('E-Mail-Adressen')}
                multiline
                rows={2}
                value={emails}
                placeholder="name@example.com"
                onChange={setEmails}
              />
            ) : (
              <FieldInput
                label={t('Passwort (optional)')}
                type="password"
                autoComplete="new-password"
                placeholder={t('Ohne Passwort')}
                value={password}
                onChange={setPassword}
              />
            )}
            {options && options.domains.length > 0 && (
              <ListRow
                icon={ICONS.website}
                iconTone="neutral"
                title={t('Link-Adresse')}
                value={domainOptions.find((o) => o.value === domain)?.label}
                onClick={() => setChoosing('domain')}
              />
            )}
            <ListRow
              title={t('Text erst nach Tipp zeigen')}
              trailing={
                <Toggle
                  label={t('Text erst nach Tipp zeigen')}
                  checked={hideText}
                  onChange={setHideText}
                />
              }
            />
          </ListSection>
        </>
      )}

      <ChoiceSheet
        open={choosing === 'days'}
        onClose={() => setChoosing(null)}
        title={t('Löschen nach')}
        options={dayOptions}
        value={days}
        onChange={setDays}
      />
      <ChoiceSheet
        open={choosing === 'domain'}
        onClose={() => setChoosing(null)}
        title={t('Link-Adresse')}
        options={domainOptions}
        value={domain}
        onChange={setDomain}
      />
      {confirm.element}
    </EditSurface>
  );
}
