/**
 * Choosing an item's own icon, as the web vault offers it: a picture from
 * this device, an icon from the server's library (with its variants), the
 * icon of a device in the home network, or none (the site's again). Whatever
 * is chosen becomes a PNG of at most 128 pixels, which Rust seals — under the
 * extras key, or the organisation's — so the server never learns which
 * library icon belongs to which item.
 *
 * The item editor holds the choice until it saves ({@link OwnIconEditor});
 * the tile's menu applies a library pick right away ({@link LibraryDialog}).
 */

import { useEffect, useRef, useState } from 'react';
import { Button, ICONS } from '@uwusuite/design';
import type { ItemSummary } from '../lib/api';
import { errorText } from '../lib/errors';
import { ICON_ACCEPT, iconFromFile } from '../lib/iconImage';
import {
  hostOf,
  searchLibrary,
  suggestLibrary,
  type IconLibrary,
  type LibraryIcon,
} from '../lib/iconLibrary';
import { t, useLanguage } from '../lib/i18n';
import { toast } from '../lib/toast';
import {
  deviceIcon,
  has,
  iconLibrary,
  isLocalHost,
  libraryIcon,
  useUwu,
  type IconChoice,
} from '../lib/uwu';
import { ItemTile } from './ItemTile';
import { Modal } from './Modal';

const variantLabel = (name: string) =>
  name === 'light' ? t('hell') : name === 'dark' ? t('dunkel') : t('normal');

/** One icon of the library: its picture (fetched through the server), its name, its variants. */
function LibraryPreview({ icon, onPick }: { icon: LibraryIcon; onPick: (png: string) => void }) {
  useLanguage();
  const [variant, setVariant] = useState('default');
  const [picture, setPicture] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let gone = false;
    setPicture(null);
    setFailed(false);
    libraryIcon(icon, variant).then(
      (png) => !gone && setPicture(png),
      () => !gone && setFailed(true),
    );
    return () => {
      gone = true;
    };
  }, [icon, variant]);
  return (
    <li className="library-icon">
      <button
        type="button"
        disabled={!picture}
        onClick={() => picture && onPick(picture)}
        aria-label={t('{name} wählen', { name: icon.name })}
        title={icon.name}
      >
        <span className="library-picture">
          {picture ? <img src={picture} alt="" draggable={false} /> : failed ? '×' : '…'}
        </span>
        <span className="library-name">{icon.name}</span>
      </button>
      {icon.variants.length > 1 && (
        <select
          value={variant}
          aria-label={t('Variante von {name}', { name: icon.name })}
          onChange={(event) => setVariant(event.target.value)}
        >
          {icon.variants.map((name) => (
            <option key={name} value={name}>
              {variantLabel(name)}
            </option>
          ))}
        </select>
      )}
    </li>
  );
}

/** For a device in the home network: library icons that fit its name or the item's. */
function LibrarySuggestions({
  host,
  name,
  onPick,
}: {
  host: string;
  name: string;
  onPick: (png: string) => void;
}) {
  useLanguage();
  const [index, setIndex] = useState<IconLibrary | null>(null);
  useEffect(() => {
    let gone = false;
    iconLibrary().then(
      (library) => !gone && setIndex(library),
      () => undefined,
    );
    return () => {
      gone = true;
    };
  }, []);
  const found = index ? suggestLibrary(index, host, name) : [];
  if (!found.length) return null;
  return (
    <div className="library-suggestions grid gap-1">
      <small className="field-hint">{t('Passt vielleicht, aus der Bibliothek:')}</small>
      <ul className="library-grid">
        {found.map((icon) => (
          <LibraryPreview key={`${icon.source}/${icon.id}`} icon={icon} onPick={onPick} />
        ))}
      </ul>
    </div>
  );
}

/** Searching the library; `onPick` gets the chosen icon as a PNG `data:` URL. */
export function LibraryDialog({
  initial,
  onCancel,
  onPick,
}: {
  initial: string;
  onCancel: () => void;
  onPick: (png: string) => void;
}) {
  useLanguage();
  const [index, setIndex] = useState<IconLibrary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState(initial);
  useEffect(() => {
    iconLibrary().then(setIndex, (e: unknown) => setError(errorText(e)));
  }, []);
  const found = index ? searchLibrary(index, query, 30) : [];
  return (
    <Modal
      title={t('Symbol aus der Bibliothek')}
      size="wide"
      onCancel={onCancel}
      footer={
        <>
          <span className="spacer" />
          <Button variant="ghost" data-secondary onClick={onCancel}>
            {t('Abbrechen')}
          </Button>
        </>
      }
    >
      <div className="extras-form">
        <label className="field">
          <span>{t('Suchen')}</span>
          <input
            type="search"
            value={query}
            autoFocus
            placeholder={t('Zum Beispiel Nextcloud')}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        {error && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
        {!index && !error && <p className="muted">{t('Lädt die Bibliothek …')}</p>}
        {index &&
          (found.length ? (
            <ul className="library-grid">
              {found.map((icon) => (
                <LibraryPreview key={`${icon.source}/${icon.id}`} icon={icon} onPick={onPick} />
              ))}
            </ul>
          ) : (
            <p className="muted">
              {query.trim() ? t('Kein Symbol gefunden.') : t('Tippe einen Namen ein.')}
            </p>
          ))}
        {index && (
          <small className="field-hint">
            {index.sources.map((source) => (
              <span key={source.id}>
                {source.name} · {source.license} · {source.attribution}.{' '}
              </span>
            ))}
            {t(
              'Das gewählte Symbol holt dein Server; im Eintrag wird es verschlüsselt gespeichert.',
            )}
          </small>
        )}
      </div>
    </Modal>
  );
}

/**
 * The item editor's icon: what it is now (or what was chosen), and the ways
 * to change it. The choice is applied after the item is saved
 * (`applyIconChoice`), so a new item can get one too. `uris` are the
 * addresses as typed in the editor: a local one offers its device's icon and
 * library suggestions.
 */
export function OwnIconEditor({
  summary,
  name,
  uris,
  value,
  onChange,
}: {
  summary: ItemSummary | null;
  name: string;
  uris: string[];
  value: IconChoice;
  onChange: (choice: IconChoice) => void;
}) {
  useLanguage();
  const uwu = useUwu();
  const [busy, setBusy] = useState(false);
  const [library, setLibrary] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  if (!has(uwu, 'own-icons')) return null;
  const libraryOn = has(uwu, 'icon-library');
  const hasOwn = Boolean(summary && summary.id in uwu.ownIcons);
  const local = uris.find((uri) => isLocalHost(hostOf(uri))) ?? null;
  const localHost = local ? hostOf(local) : null;
  const own = value === 'remove' ? false : Boolean(value) || hasOwn;

  const take = async (make: () => Promise<string>) => {
    setBusy(true);
    try {
      onChange({ png: await make() });
    } catch (e) {
      toast(e instanceof Error ? t('Das Bild ließ sich nicht lesen.') : errorText(e), 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="editor-icon">
      {value && value !== 'remove' ? (
        <span className="item-tile" data-size="large" data-image aria-hidden>
          <img src={value.png} alt="" draggable={false} />
        </span>
      ) : summary && value !== 'remove' ? (
        <ItemTile item={summary} size="large" />
      ) : (
        <span className="item-tile" data-size="large" data-hue="1" aria-hidden>
          {(name.trim().charAt(0) || '?').toUpperCase()}
        </span>
      )}
      <small className="field-hint">
        {value === 'remove'
          ? t('Das eigene Symbol wird beim Speichern entfernt.')
          : value
            ? t('Das neue Symbol gilt, sobald du speicherst.')
            : own
              ? t(
                  'Ein eigenes Symbol, verschlüsselt gespeichert. Die offiziellen Bitwarden-Apps zeigen das der Website.',
                )
              : local
                ? t(
                    'Ein Gerät im Heimnetz: dein Server fragt es nie. Hol das Symbol vom Gerät, nimm eins aus der Bibliothek oder ein eigenes Bild.',
                  )
                : t('Das Symbol der Website, geholt von deinem Server. Oder wähle ein eigenes.')}
      </small>
      <div className="flex flex-wrap gap-1">
        <input
          ref={input}
          type="file"
          accept={ICON_ACCEPT}
          hidden
          onChange={(event) => {
            const file = event.target.files?.[0];
            event.target.value = '';
            if (file) void take(() => iconFromFile(file));
          }}
        />
        <Button
          variant="ghost"
          size="sm"
          icon={ICONS.upload}
          disabled={busy}
          onClick={() => input.current?.click()}
        >
          {t('Bild wählen …')}
        </Button>
        {libraryOn && (
          <Button
            variant="ghost"
            size="sm"
            icon={ICONS.search}
            disabled={busy}
            onClick={() => setLibrary(true)}
          >
            {t('Aus der Bibliothek …')}
          </Button>
        )}
        {local && (
          <Button
            variant="ghost"
            size="sm"
            icon={ICONS.network}
            disabled={busy}
            onClick={() => void take(() => deviceIcon(local))}
          >
            {t('Symbol vom Gerät holen')}
          </Button>
        )}
        {value && (
          <Button variant="ghost" size="sm" icon={ICONS.undo} onClick={() => onChange(null)}>
            {t('Wie bisher')}
          </Button>
        )}
        {hasOwn && value !== 'remove' && (
          <Button
            variant="ghost"
            size="sm"
            icon={ICONS.delete}
            disabled={busy}
            onClick={() => onChange('remove')}
          >
            {t('Eigenes Symbol entfernen')}
          </Button>
        )}
      </div>
      {libraryOn && localHost && !own && !value && (
        <LibrarySuggestions host={localHost} name={name} onPick={(png) => onChange({ png })} />
      )}
      {library && (
        <LibraryDialog
          initial={name}
          onCancel={() => setLibrary(false)}
          onPick={(png) => {
            setLibrary(false);
            onChange({ png });
          }}
        />
      )}
    </div>
  );
}
