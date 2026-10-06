/**
 * The account switcher (the avatar on iOS and Android, the account card in
 * Settings): every account on this device, the open one ticked, the others
 * marked when they are locked. One account is open at a time; the others keep
 * their own keys and their own vault, as on the desktop's AccountCard.
 */

import { ICONS, ListRow, ListSection } from '@uwusuite/design';
import { lock, switchAccount, type AccountBrief } from '../../lib/api';
import { toastError } from '../../lib/errors';
import { t, useLanguage } from '../../lib/i18n';
import { initialOf } from '../../components/AccountCard';
import { useMobile } from '../state';
import { Chip, ShortSheet } from '../ui';

export function AccountsSheet({ open, onClose }: { open: boolean; onClose: () => void }) {
  useLanguage();
  const { status, onAddAccount } = useMobile();
  const accounts = status.accounts;

  const choose = (account: AccountBrief) => {
    onClose();
    if (account.active) return;
    void switchAccount(account.id).catch(toastError);
  };

  return (
    <ShortSheet open={open} onClose={onClose} title={t('Konten')}>
      <ListSection>
        {accounts.map((account, index) => (
          <button
            key={account.id}
            type="button"
            className="m-account"
            aria-current={account.active || undefined}
            onClick={() => choose(account)}
          >
            <span className="m-avatar" data-size="small" data-alt={index % 2 ? '' : undefined}>
              {initialOf(account)}
            </span>
            <span className="m-account-text">
              <b>{account.label}</b>
              <span>
                {account.email} · {account.server}
              </span>
            </span>
            {account.active ? (
              <ICONS.done className="m-check" aria-label={t('Geöffnet')} />
            ) : (
              !account.unlocked && (
                <Chip icon={ICONS.locked} tone="muted">
                  {t('Gesperrt')}
                </Chip>
              )
            )}
          </button>
        ))}
      </ListSection>
      <ListSection>
        <ListRow
          icon={ICONS.add}
          iconTone="pink"
          title={t('Konto hinzufügen')}
          chevron={false}
          onClick={() => {
            onClose();
            onAddAccount();
          }}
        />
      </ListSection>
      <ListSection>
        <ListRow
          icon={ICONS.locked}
          iconTone="neutral"
          title={accounts.length > 1 ? t('Alle sperren') : t('Jetzt sperren')}
          chevron={false}
          onClick={() => {
            onClose();
            void lock().catch(toastError);
          }}
        />
      </ListSection>
    </ShortSheet>
  );
}
