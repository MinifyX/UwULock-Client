/**
 * "Icon wählen", over the edit sheet: a photo or a file from this device, the
 * icon of a device in the home network, the website's icon again, or one of
 * the server's library in colour, light or dark. The choice goes back to the
 * editor, which applies it once the item is saved (`applyIconChoice`).
 */

import { haptic, ICONS, ListRow, ListSection } from '@uwusuite/design';
import { useEffect, useRef, useState } from 'react';
import type { ItemSummary } from '../../lib/api';
import { errorText } from '../../lib/errors';
import { ICON_ACCEPT, iconFromFile } from '../../lib/iconImage';
import {
  hostOf,
  searchLibrary,
  suggestLibrary,
  type IconLibrary,
  type LibraryIcon,
} from '../../lib/iconLibrary';
import { t, useLanguage } from '../../lib/i18n';
import { toast } from '../../lib/toast';
import {
  deviceIcon,
  has,
  iconLibrary,
  isLocalHost,
  libraryIcon,
  useUwu,
  type IconChoice,
} from '../../lib/uwu';
import { useMobile } from '../state';
import { EditSurface, Segmented } from '../ui';

type Variant = 'default' | 'light' | 'dark';

/** One library icon in the chosen variant (or its default, where it has no such variant). */
function LibraryTile({
  icon,
  variant,
  onPick,
}: {
  icon: LibraryIcon;
  variant: Variant;
  onPick: (png: string) => void;
}) {
  useLanguage();
  const [picture, setPicture] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const shown = icon.variants.includes(variant) ? variant : 'default';
  useEffect(() => {
    let gone = false;
    setPicture(null);
    setFailed(false);
    libraryIcon(icon, shown).then(
      (png) => !gone && setPicture(png),
      () => !gone && setFailed(true),
    );
    return () => {
      gone = true;
    };
  }, [icon, shown]);
  return (
    <button
      type="button"
      disabled={!picture}
      aria-label={t('{name} wählen', { name: icon.name })}
      onClick={() => {
        if (!picture) return;
        haptic('selection');
        onPick(picture);
      }}
    >
      {picture ? (
        <img src={picture} alt="" draggable={false} />
      ) : (
        <span className="m-library-blank">{failed ? '×' : ''}</span>
      )}
      <span>{icon.name}</span>
    </button>
  );
}

export function IconPicker({
  open,
  onClose,
  summary,
  name,
  uris,
  value,
  onChange,
}: {
  open: boolean;
  onClose: () => void;
  summary: ItemSummary | null;
  name: string;
  uris: string[];
  value: IconChoice;
  onChange: (choice: IconChoice) => void;
}) {
  useLanguage();
  const { android } = useMobile();
  const uwu = useUwu();
  const libraryOn = has(uwu, 'icon-library');
  const [index, setIndex] = useState<IconLibrary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [variant, setVariant] = useState<Variant>('default');
  const [busy, setBusy] = useState(false);
  const photo = useRef<HTMLInputElement>(null);
  const file = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open || !libraryOn || index) return;
    iconLibrary().then(setIndex, (e: unknown) => setError(errorText(e)));
  }, [open, libraryOn, index]);

  const hasOwn = Boolean(summary && summary.id in uwu.ownIcons);
  const own = value === 'remove' ? false : Boolean(value) || hasOwn;
  const local = uris.find((uri) => isLocalHost(hostOf(uri))) ?? null;
  const localHost = local ? hostOf(local) : null;

  const pick = (choice: IconChoice) => {
    onChange(choice);
    onClose();
  };
  const take = async (make: () => Promise<string>) => {
    setBusy(true);
    try {
      pick({ png: await make() });
    } catch (e) {
      toast(e instanceof Error ? t('Das Bild ließ sich nicht lesen.') : errorText(e), 'error');
    } finally {
      setBusy(false);
    }
  };
  const fromInput = (input: HTMLInputElement) => {
    const chosen = input.files?.[0];
    input.value = '';
    if (chosen) void take(() => iconFromFile(chosen));
  };

  const found = !index
    ? []
    : query.trim()
      ? searchLibrary(index, query, 30)
      : suggestLibrary(index, localHost, name, 15);

  return (
    <EditSurface open={open} onClose={onClose} title={t('Icon wählen')} closeIcon="back">
      <input
        ref={photo}
        type="file"
        accept="image/*"
        hidden
        onChange={(event) => fromInput(event.target)}
      />
      <input
        ref={file}
        type="file"
        accept={ICON_ACCEPT}
        hidden
        onChange={(event) => fromInput(event.target)}
      />
      <ListSection>
        <ListRow
          icon={ICONS.addImage}
          title={android ? t('Aus der Galerie') : t('Foto auswählen')}
          disabled={busy}
          onClick={() => photo.current?.click()}
        />
        <ListRow
          icon={ICONS.upload}
          title={t('Datei hochladen')}
          disabled={busy}
          onClick={() => file.current?.click()}
        />
        {local && (
          <ListRow
            icon={ICONS.home}
            title={t('Vom Gerät im Heimnetz')}
            subtitle={localHost ?? undefined}
            disabled={busy}
            onClick={() => void take(() => deviceIcon(local))}
          />
        )}
        <ListRow
          icon={ICONS.website}
          iconTone="neutral"
          title={t('Icon der Website')}
          chevron={false}
          trailing={!own ? <ICONS.done className="m-check" aria-label={t('Gewählt')} /> : undefined}
          onClick={() => pick(hasOwn ? 'remove' : null)}
        />
      </ListSection>

      {libraryOn && (
        <>
          <div className="uwu-list-header">
            <h2 style={{ font: 'inherit', margin: 0 }}>{t('Bibliothek')}</h2>
          </div>
          <Segmented
            label={t('Variante')}
            value={variant}
            onChange={setVariant}
            options={[
              { value: 'default', label: t('Farbig') },
              { value: 'light', label: t('Hell') },
              { value: 'dark', label: t('Dunkel') },
            ]}
          />
          <label className="m-search-field">
            <ICONS.search aria-hidden />
            <input
              type="search"
              value={query}
              placeholder={t('Zum Beispiel Nextcloud')}
              aria-label={t('Bibliothek durchsuchen')}
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>
          <ListSection
            footer={
              index
                ? index.sources.map((source) => `${source.name} · ${source.license}`).join(' · ')
                : undefined
            }
          >
            {error ? (
              <ListRow title={error} wrap />
            ) : !index ? (
              <ListRow title={t('Lädt die Bibliothek …')} />
            ) : found.length ? (
              <div className="m-library">
                {found.map((icon) => (
                  <LibraryTile
                    key={`${icon.source}/${icon.id}`}
                    icon={icon}
                    variant={variant}
                    onPick={(png) => pick({ png })}
                  />
                ))}
              </div>
            ) : (
              <ListRow
                title={query.trim() ? t('Kein Icon gefunden.') : t('Tippe einen Namen ein.')}
              />
            )}
          </ListSection>
        </>
      )}

      {hasOwn && value !== 'remove' && (
        <>
          <div className="m-gap" />
          <ListSection>
            <ListRow
              title={t('Eigenes Icon entfernen')}
              tone="danger"
              onClick={() => pick('remove')}
            />
          </ListSection>
        </>
      )}
      <p className="m-footnote">
        {t(
          'Eigene Icons liegen verschlüsselt im Tresor, wie im Web-Tresor. Die offiziellen Bitwarden-Apps zeigen weiter das Icon der Website.',
        )}
      </p>
    </EditSurface>
  );
}
