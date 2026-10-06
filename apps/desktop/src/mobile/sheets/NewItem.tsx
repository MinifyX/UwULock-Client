/** "Neuer Eintrag": which kind, then the editor. Also a new folder. */

import { ICONS, ListRow, ListSection } from '@uwusuite/design';
import type { LucideIcon } from 'lucide-react';
import type { ItemKind } from '../../lib/api';
import { t, useLanguage } from '../../lib/i18n';
import { KIND_LABEL } from '../../lib/items';
import { useMobile } from '../state';
import { ShortSheet } from '../ui';

export const KIND_ICON: Record<ItemKind, LucideIcon> = {
  login: ICONS.website,
  card: ICONS.card,
  identity: ICONS.identity,
  note: ICONS.note,
  'ssh-key': ICONS.sshKey,
  wifi: ICONS.wifi,
};

const KINDS: ItemKind[] = ['login', 'card', 'identity', 'note', 'ssh-key', 'wifi'];

export function NewItemSheet({ open, onClose }: { open: boolean; onClose: () => void }) {
  useLanguage();
  const { openSheet } = useMobile();
  return (
    <ShortSheet open={open} onClose={onClose} title={t('Neuer Eintrag')}>
      <ListSection>
        {KINDS.map((kind) => (
          <ListRow
            key={kind}
            icon={KIND_ICON[kind]}
            title={t(KIND_LABEL[kind])}
            onClick={() => openSheet({ kind: 'edit', id: null, type: kind })}
          />
        ))}
      </ListSection>
      <div className="m-gap" />
      <ListSection>
        <ListRow
          icon={ICONS.newFolder}
          iconTone="neutral"
          title={t('Neuer Ordner')}
          onClick={() => openSheet({ kind: 'folder', id: null, name: '' })}
        />
      </ListSection>
    </ShortSheet>
  );
}
