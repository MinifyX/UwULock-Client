/** Into another folder of the own vault. */

import { haptic } from '@uwusuite/design';
import { setItemFolder } from '../../lib/api';
import { toastError } from '../../lib/errors';
import { t, useLanguage } from '../../lib/i18n';
import { note } from '../../lib/toast';
import { useMobile } from '../state';
import { ChoiceSheet } from '../ui';

export function MoveSheet({
  open,
  onClose,
  id,
}: {
  open: boolean;
  onClose: () => void;
  id: string;
}) {
  useLanguage();
  const { data } = useMobile();
  const item = data.byId(id);
  const folders = [...(data.overview?.folders ?? [])].sort((a, b) => a.name.localeCompare(b.name));
  const options = [
    { value: '', label: t('Ohne Ordner') },
    ...folders.map((folder) => ({ value: folder.id, label: folder.name })),
  ];
  return (
    <ChoiceSheet
      open={open}
      onClose={onClose}
      title={t('In Ordner verschieben')}
      options={options}
      value={item?.folderId ?? ''}
      onChange={(folderId) => {
        if (!item || (item.folderId ?? '') === folderId) return;
        void setItemFolder(item.id, folderId || null)
          .then(() => {
            haptic('success');
            note(t('Verschoben ✧'), { tone: 'success' });
          })
          .catch((e) => toastError(e));
      }}
    />
  );
}
