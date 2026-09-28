import { useCallback, useEffect, useState } from 'react';
import { Icon } from '@desktop/components/Icon';
import { NyuScene } from '@desktop/components/nyu/scenes';
import { t } from '../../shared/i18n';
import { ItemIcon } from '../icons';
import type { ItemSummary, PendingSave, StatusMessage, TabItems } from '../../shared/protocol';
import { ext } from '../../shared/browser';
import { answerPendingSave, copyField, fillTab, pendingSaves, tabItems } from '../api';
import { copiedText, errorText, toast, useSettings } from '../lib';
import type { EditorTarget } from './Editor';

/**
 * What belongs to the page in the active tab: its logins, with a button to fill them, and the
 * cards and addresses for a checkout. Logins sent while the vault was locked wait on top.
 */
export function TabView({
  onOpen,
  onNew,
}: {
  onOpen: (id: string) => void;
  onNew: (target: EditorTarget) => void;
}) {
  const settings = useSettings();
  const [items, setItems] = useState<TabItems | null>(null);
  const [pending, setPending] = useState<PendingSave[]>([]);
  const [confirm, setConfirm] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setItems(await tabItems());
      setPending(await pendingSaves());
    } catch (e) {
      // Locked meanwhile: the lock screen takes over, nothing to say here.
      if ((e as { kind?: string }).kind !== 'locked') toast(errorText(e), 'error');
    }
  }, []);

  useEffect(() => {
    void load();
    const listener = (message: unknown) => {
      if ((message as StatusMessage | null)?.type === 'bg:status-changed') void load();
    };
    ext.runtime.onMessage.addListener(listener);
    return () => ext.runtime.onMessage.removeListener(listener);
  }, [load]);

  const fill = async (item: ItemSummary, insecureOk = false) => {
    if (items?.insecure && !insecureOk) {
      setConfirm(item.id);
      return;
    }
    try {
      await fillTab(item.id, insecureOk);
      window.close();
    } catch (e) {
      toast(errorText(e), 'error');
    }
  };

  const copy = async (item: ItemSummary, field: 'username' | 'password' | 'totp') => {
    try {
      await copyField(item.id, field);
      toast(copiedText(field, settings?.clipboardClear ?? 30));
    } catch (e) {
      toast(errorText(e), 'error');
    }
  };

  const answer = async (save: PendingSave, choice: 'save' | 'update' | 'never' | 'dismiss') => {
    try {
      await answerPendingSave(save.id, choice);
      if (choice === 'save' || choice === 'update') toast(t('Gespeichert ✧'));
    } catch (e) {
      toast(errorText(e), 'error');
    }
    void load();
  };

  if (!items) return <div className="popup-scroll" aria-busy />;

  const row = (item: ItemSummary, fillable: boolean) => (
    <li key={item.id} className="item-row" onClick={() => onOpen(item.id)}>
      <ItemIcon item={item} />
      <span className="item-text">
        <span className="item-name">{item.name || t('(ohne Namen)')}</span>
        {item.subtitle && <span className="item-sub">{item.subtitle}</span>}
      </span>
      <span className="row-actions" onClick={(e) => e.stopPropagation()}>
        {item.kind === 'login' && item.hasUsername && (
          <button
            type="button"
            className="icon-button"
            onClick={() => void copy(item, 'username')}
            aria-label={t('Benutzername kopieren')}
            title={t('Benutzername kopieren')}
          >
            <Icon name="user" size={15} />
          </button>
        )}
        {item.kind === 'login' && item.hasPassword && (
          <button
            type="button"
            className="icon-button"
            onClick={() => void copy(item, 'password')}
            aria-label={t('Passwort kopieren')}
            title={t('Passwort kopieren')}
          >
            <Icon name="key" size={15} />
          </button>
        )}
        {item.kind === 'login' && item.hasTotp && (
          <button
            type="button"
            className="icon-button"
            onClick={() => void copy(item, 'totp')}
            aria-label={t('Einmal-Code kopieren')}
            title={t('Einmal-Code kopieren')}
          >
            <Icon name="clock" size={15} />
          </button>
        )}
        {fillable && (
          <button type="button" className="fill-button" onClick={() => void fill(item)}>
            {t('Ausfüllen')}
          </button>
        )}
      </span>
    </li>
  );

  const canFill = items.fillable;
  return (
    <div className="popup-scroll">
      {pending.map((save) => (
        <section key={save.id} className="pending-save" aria-live="polite">
          <p>
            {save.action === 'update'
              ? t('Passwort für {name} bei {host} aktualisieren?', {
                  name: save.itemName ?? save.host,
                  host: save.host,
                })
              : t('Login bei {host} speichern?', { host: save.host })}
            {save.username && <span className="muted"> · {save.username}</span>}
          </p>
          <div className="form-actions">
            <button type="button" className="quiet" onClick={() => void answer(save, 'never')}>
              {t('Nie für diese Seite')}
            </button>
            <button type="button" className="quiet" onClick={() => void answer(save, 'dismiss')}>
              {t('Verwerfen')}
            </button>
            <span className="spacer" />
            <button
              type="button"
              className="primary"
              onClick={() => void answer(save, save.action)}
            >
              {save.action === 'update' ? t('Aktualisieren') : t('Speichern')}
            </button>
          </div>
        </section>
      ))}

      <h2 className="section-title">
        {items.host ?? t('Diese Seite')}
        {items.insecure && (
          <span
            className="chip chip-warning"
            title={t('Diese Seite ist nicht verschlüsselt (http).')}
          >
            http
          </span>
        )}
      </h2>

      {confirm && (
        <div className="notice" data-tone="error" role="alert">
          <span>{t('Diese Seite ist nicht verschlüsselt (http). Trotzdem ausfüllen?')}</span>
          <button
            type="button"
            onClick={() => {
              const item =
                items.logins.find((i) => i.id === confirm) ??
                [...items.cards, ...items.identities].find((i) => i.id === confirm);
              setConfirm(null);
              if (item) void fill(item, true);
            }}
          >
            {t('Ausfüllen')}
          </button>
          <button type="button" className="quiet" onClick={() => setConfirm(null)}>
            {t('Abbrechen')}
          </button>
        </div>
      )}

      {!canFill && (
        <p className="empty-line">{t('Auf dieser Seite kann UwULock nichts ausfüllen.')}</p>
      )}
      {canFill && items.logins.length === 0 && (
        <div className="empty-block">
          <NyuScene name="pick" className="empty-scene small" />
          <p className="empty-line">{t('Noch kein Login für diese Seite.')}</p>
        </div>
      )}
      {items.logins.length > 0 && (
        <ul className="item-list plain">{items.logins.map((i) => row(i, canFill))}</ul>
      )}
      {canFill && (
        <button
          type="button"
          className="add-here"
          onClick={() =>
            onNew({
              id: null,
              kind: 'login',
              name: items.host ?? '',
              uri: items.url ? new URL(items.url).origin : undefined,
            })
          }
        >
          <Icon name="plus" size={14} /> {t('Neuer Login für diese Seite')}
        </button>
      )}

      {canFill && items.cards.length > 0 && (
        <>
          <h2 className="section-title">{t('Karten')}</h2>
          <ul className="item-list plain">{items.cards.map((i) => row(i, true))}</ul>
        </>
      )}
      {canFill && items.identities.length > 0 && (
        <>
          <h2 className="section-title">{t('Identitäten')}</h2>
          <ul className="item-list plain">{items.identities.map((i) => row(i, true))}</ul>
        </>
      )}
    </div>
  );
}
