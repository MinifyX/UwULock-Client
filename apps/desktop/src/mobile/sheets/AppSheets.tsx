/**
 * The sheets the whole app opens (state.tsx's `AppSheet`): new item, editor,
 * sharing as a Send, accounts, folders, moving. The last one stays drawn
 * while it slides away.
 */

import { useRef } from 'react';
import { useMobile, type AppSheet } from '../state';
import { AccountsSheet } from './Accounts';
import { FolderSheet } from './Folder';
import { ItemEditSheet } from './ItemEdit';
import { MoveSheet } from './Move';
import { NewItemSheet } from './NewItem';
import { ShareSheet } from './Share';

/** The last sheet of a kind, kept for its closing animation; a new one gets a new key. */
function useLast<K extends AppSheet['kind']>(
  sheet: AppSheet | null,
  kind: K,
): { value: Extract<AppSheet, { kind: K }> | null; open: boolean; key: number } {
  const last = useRef<{ sheet: AppSheet | null; key: number }>({ sheet: null, key: 0 });
  // Settled while drawing, so the first frame of a new sheet already has its key.
  if (sheet?.kind === kind && last.current.sheet !== sheet)
    last.current = { sheet, key: last.current.key + 1 };
  return {
    value: last.current.sheet as Extract<AppSheet, { kind: K }> | null,
    open: sheet?.kind === kind,
    key: last.current.key,
  };
}

export function AppSheets() {
  const { sheet, closeSheet } = useMobile();
  const edit = useLast(sheet, 'edit');
  const share = useLast(sheet, 'share');
  const folder = useLast(sheet, 'folder');
  const move = useLast(sheet, 'move');
  return (
    <>
      <NewItemSheet open={sheet?.kind === 'new'} onClose={closeSheet} />
      {edit.value && (
        <ItemEditSheet
          key={`edit-${edit.key}`}
          open={edit.open}
          onClose={closeSheet}
          id={edit.value.id}
          kind={edit.value.type}
          folderId={edit.value.folderId}
        />
      )}
      {share.value && (
        <ShareSheet
          key={`share-${share.key}`}
          id={share.value.id}
          open={share.open}
          onClose={closeSheet}
        />
      )}
      <AccountsSheet open={sheet?.kind === 'accounts'} onClose={closeSheet} />
      {folder.value && (
        <FolderSheet
          key={`folder-${folder.key}`}
          open={folder.open}
          onClose={closeSheet}
          id={folder.value.id}
          name={folder.value.name}
        />
      )}
      {move.value && (
        <MoveSheet
          key={`move-${move.key}`}
          open={move.open}
          onClose={closeSheet}
          id={move.value.id}
        />
      )}
    </>
  );
}
