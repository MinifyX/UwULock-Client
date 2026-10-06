/** A new folder, or a folder's new name. */

import { haptic, ListSection } from '@uwusuite/design';
import { useState } from 'react';
import { saveFolder } from '../../lib/api';
import { errorText } from '../../lib/errors';
import { t, useLanguage } from '../../lib/i18n';
import { note } from '../../lib/toast';
import { useMobile } from '../state';
import { EditSurface, FieldInput } from '../ui';

export function FolderSheet({
  open,
  onClose,
  id,
  name: before,
}: {
  open: boolean;
  onClose: () => void;
  id: string | null;
  name: string;
}) {
  useLanguage();
  const { data } = useMobile();
  const [name, setName] = useState(before);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const save = async () => {
    if (!name.trim()) return;
    setBusy(true);
    try {
      await saveFolder(id, name.trim());
      haptic('success');
      note(id ? t('Ordner umbenannt ✧') : t('Ordner angelegt ✧'), { tone: 'success' });
      await data.reload();
      onClose();
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
      title={id ? t('Ordner umbenennen') : t('Neuer Ordner')}
      dirty={name !== before}
      action={{
        label: id ? t('Sichern') : t('Anlegen'),
        onClick: () => void save(),
        disabled: busy || !name.trim() || name === before,
      }}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <ListSection
          footer={t('Ordner gibt es nur in deinem eigenen Tresor, nicht in Organisationen.')}
        >
          <FieldInput label={t('Name')} value={name} autoFocus onChange={setName} maxLength={200} />
        </ListSection>
        {error && (
          <p className="m-error" role="alert">
            {error}
          </p>
        )}
      </form>
    </EditSurface>
  );
}
