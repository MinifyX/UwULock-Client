/**
 * Small parts the phone and iPad pages share, on top of @uwusuite/design's
 * mobile components: the page frame, the switch, the hero at the top of an
 * item, input rows, the edit surface (iOS sheet, Android full-screen dialog,
 * iPad form sheet), choice sheets and the "…" menu. Styles in mobile.css.
 */

import {
  ContextMenu,
  FullScreenDialog,
  haptic,
  ICONS,
  NavButton,
  Screen,
  Sheet,
  type ContextMenuEntry,
  type ScreenProps,
} from '@uwusuite/design';
import type { LucideIcon } from 'lucide-react';
import { useEffect, useId, useState, type ReactNode } from 'react';
import { useBackLayer } from '../lib/backStack';
import { t, useLanguage } from '../lib/i18n';
import { useMobile, useNav } from './state';

/** A page: @uwusuite/design's Screen with back wired to where the page sits. */
export function Page({
  hero,
  ...props
}: Omit<ScreenProps, 'onBack' | 'underRef'> & {
  /** The page opens with a big name of its own (Hero), so it needs no large title. */
  hero?: boolean;
}) {
  const nav = useNav();
  const { ipad } = useMobile();
  // The iPad hides the small centred title (the tab bar floats there), so a
  // page without a Hero shows its title large instead.
  const largeTitle = props.largeTitle ?? (ipad && !hero && Boolean(props.title));
  return (
    <Screen
      {...props}
      largeTitle={largeTitle}
      onBack={nav.canBack ? nav.back : undefined}
      underRef={nav.underRef}
      className={props.className}
    />
  );
}

/** While `open`, Android's back button calls `close`. */
export function BackLayer({ open, close }: { open: boolean; close: () => void }) {
  useBackLayer(open, close);
  return null;
}

/** The switch: iOS's 51 × 31 capsule, Android's M3 switch; the suite's pink when on. */
export function Toggle({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      className="m-switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => {
        haptic('selection');
        onChange(!checked);
      }}
    />
  );
}

/** A chip under the hero's title. */
export function Chip({
  icon: Glyph,
  children,
  tone,
  onClick,
  pressed,
}: {
  icon?: LucideIcon;
  children: ReactNode;
  tone?: 'fav' | 'warn' | 'muted';
  onClick?: () => void;
  pressed?: boolean;
}) {
  const inner = (
    <>
      {Glyph && <Glyph aria-hidden strokeWidth={1.9} />}
      {children}
    </>
  );
  if (onClick)
    return (
      <button
        type="button"
        className="m-chip"
        data-tone={tone}
        aria-pressed={pressed}
        onClick={onClick}
      >
        {inner}
      </button>
    );
  return (
    <span className="m-chip" data-tone={tone}>
      {inner}
    </span>
  );
}

/** Aktiv · abgeschaltet · abgelaufen · neu. */
export function StateChip({ state }: { state: 'on' | 'off' | 'gone' | 'new' }) {
  useLanguage();
  const text =
    state === 'on'
      ? t('aktiv')
      : state === 'off'
        ? t('abgeschaltet')
        : state === 'gone'
          ? t('abgelaufen')
          : t('neu');
  return (
    <span className="m-state" data-state={state}>
      {text}
    </span>
  );
}

/** The top of a detail page: the tile, the name, chips. */
export function Hero({
  tile,
  title,
  sub,
  children,
}: {
  tile: ReactNode;
  title: ReactNode;
  sub?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="m-hero">
      {tile}
      <h2>{title}</h2>
      {sub && <p className="m-hero-sub">{sub}</p>}
      {children && <div className="m-hero-meta">{children}</div>}
    </div>
  );
}

/** A tile with a glyph for things that are not items (a Send, a file request). */
export function GlyphTile({ icon: Glyph, size }: { icon: LucideIcon; size?: 'large' }) {
  return (
    <span className="m-glyph-tile" data-size={size} aria-hidden>
      <Glyph strokeWidth={1.9} />
    </span>
  );
}

/** An input inside a grouped list: the label above, the value below, buttons at the end. */
export function FieldInput({
  label,
  value,
  onChange,
  placeholder,
  type = 'text',
  mono,
  multiline,
  rows = 3,
  trailing,
  autoFocus,
  inputMode,
  autoComplete = 'off',
  disabled,
  maxLength,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  type?: 'text' | 'password' | 'email' | 'url' | 'number' | 'search';
  mono?: boolean;
  multiline?: boolean;
  rows?: number;
  trailing?: ReactNode;
  autoFocus?: boolean;
  inputMode?: 'text' | 'numeric' | 'email' | 'url';
  autoComplete?: string;
  disabled?: boolean;
  maxLength?: number;
}) {
  const id = useId();
  return (
    <div className="m-field" data-uwu-field="" data-mono={mono || undefined}>
      <label htmlFor={id}>{label}</label>
      <div className="m-field-row">
        {multiline ? (
          <textarea
            id={id}
            value={value}
            rows={rows}
            placeholder={placeholder}
            disabled={disabled}
            maxLength={maxLength}
            onChange={(event) => onChange(event.target.value)}
          />
        ) : (
          <input
            id={id}
            type={type}
            value={value}
            placeholder={placeholder}
            autoFocus={autoFocus}
            inputMode={inputMode}
            autoComplete={autoComplete}
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            disabled={disabled}
            maxLength={maxLength}
            onChange={(event) => onChange(event.target.value)}
          />
        )}
        {trailing}
      </div>
    </div>
  );
}

/** A round button inside a row or field (copy, show, roll the dice). */
export function RowButton({
  icon: Glyph,
  label,
  onClick,
  pressed,
  disabled,
}: {
  icon: LucideIcon;
  label: string;
  onClick: () => void;
  pressed?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      className="m-rowbtn"
      aria-label={label}
      title={label}
      aria-pressed={pressed}
      disabled={disabled}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
    >
      <Glyph aria-hidden strokeWidth={1.9} />
    </button>
  );
}

/** A wide pink button under a card ("Passwörter durchgehen"). */
export function BigButton({
  icon: Glyph,
  children,
  onClick,
  soft,
  danger,
  disabled,
}: {
  icon?: LucideIcon;
  children: ReactNode;
  onClick: () => void;
  soft?: boolean;
  danger?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      className="m-bigbtn"
      data-soft={soft || undefined}
      data-danger={danger || undefined}
      disabled={disabled}
      onClick={onClick}
    >
      {Glyph && <Glyph aria-hidden strokeWidth={2} />}
      {children}
    </button>
  );
}

/** A link with copy and share buttons (a Send's, a file request's). */
export function LinkBox({
  url,
  onCopy,
  onShare,
}: {
  url: string;
  onCopy: () => void;
  onShare?: () => void;
}) {
  useLanguage();
  const { android } = useMobile();
  return (
    <div className="m-linkbox">
      <span className="m-linkbox-url selectable">{url}</span>
      <RowButton icon={ICONS.copy} label={t('Link kopieren')} onClick={onCopy} />
      {onShare && (
        <RowButton
          icon={android ? ICONS.share : ICONS.export}
          label={t('Teilen')}
          onClick={onShare}
        />
      )}
    </div>
  );
}

/** Nothing here (yet). */
export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="m-empty">
      <b>{title}</b>
      {children}
    </div>
  );
}

/** Two or three choices side by side ("Passwort · Maskiert", "Hell · Dunkel · System"). */
export function Segmented<T extends string>({
  value,
  onChange,
  options,
  label,
}: {
  value: T;
  onChange: (value: T) => void;
  options: readonly { value: T; label: string }[];
  label: string;
}) {
  return (
    <div className="m-segmented" role="group" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          aria-pressed={option.value === value}
          onClick={() => {
            if (option.value !== value) haptic('selection');
            onChange(option.value);
          }}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

/**
 * Where something is created or changed: a sheet on iOS (× left, a pink tick
 * right), Android's full-screen dialog, a form sheet on the iPad. While it
 * holds unsaved input it doesn't close by dragging or tapping beside it.
 */
export function EditSurface({
  open,
  onClose,
  title,
  action,
  dirty,
  closeIcon = 'close',
  children,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  /** The confirming action ("Sichern", "Anlegen"). */
  action?: { label: string; onClick: () => void; disabled?: boolean };
  dirty?: boolean;
  /** `back`: a chevron instead of the ×, for a sheet over a sheet (the icon picker). */
  closeIcon?: 'close' | 'back';
  children: ReactNode;
}) {
  useLanguage();
  const { android } = useMobile();
  // Unsaved input isn't dropped by a tap on ×, a swipe back or Android's back:
  // it asks first. (The layer before the question: back answers the question.)
  const close = () => {
    if (dirty)
      confirm.ask({ title: t('Änderungen verwerfen?'), confirm: t('Verwerfen'), run: onClose });
    else onClose();
  };
  useBackLayer(open, close);
  const confirm = useConfirm();
  if (android)
    return (
      <>
        <FullScreenDialog
          open={open}
          onClose={close}
          title={title}
          action={action && { ...action, label: action.label }}
          className="m-edit"
        >
          {children}
        </FullScreenDialog>
        {confirm.element}
      </>
    );
  return (
    <>
      <Sheet
        open={open}
        onClose={close}
        title={title}
        dismissible={!dirty}
        className="m-edit"
        leading={
          <NavButton
            label={closeIcon === 'back' ? t('Zurück') : t('Schließen')}
            icon={closeIcon === 'back' ? ICONS.previous : ICONS.close}
            onClick={close}
          />
        }
        trailing={
          action && (
            <NavButton
              label={action.label}
              icon={ICONS.done}
              tint
              disabled={action.disabled}
              onClick={action.onClick}
            />
          )
        }
      >
        {children}
      </Sheet>
      {confirm.element}
    </>
  );
}

/** A short sheet with a title (the account switcher, "Neuer Eintrag"). */
export function ShortSheet({
  open,
  onClose,
  title,
  children,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
}) {
  useLanguage();
  const { android, ipad } = useMobile();
  useBackLayer(open, onClose);
  return (
    <Sheet
      open={open}
      onClose={onClose}
      title={title}
      detents={['medium', 'large']}
      initialDetent="medium"
      ipadHeight={ipad ? 520 : undefined}
      className="m-short"
      leading={
        android ? undefined : (
          <NavButton label={t('Schließen')} icon={ICONS.close} onClick={onClose} />
        )
      }
    >
      {children}
    </Sheet>
  );
}

/** Pick one of a few values in a short sheet ("Löschen nach", "Automatisch sperren"). */
export function ChoiceSheet<T extends string | number>({
  open,
  onClose,
  title,
  options,
  value,
  onChange,
  footer,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  options: readonly { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
  footer?: ReactNode;
}) {
  return (
    <ShortSheet open={open} onClose={onClose} title={title}>
      <div className="m-choices uwu-list">
        <div className="uwu-list-body">
          {options.map((option) => (
            <button
              key={String(option.value)}
              type="button"
              className="uwu-row"
              aria-pressed={option.value === value}
              onClick={() => {
                haptic('selection');
                onChange(option.value);
                onClose();
              }}
            >
              <span className="uwu-row-text">
                <span className="uwu-row-title">{option.label}</span>
              </span>
              {option.value === value && <ICONS.done className="m-check" aria-hidden />}
            </button>
          ))}
        </div>
        {footer && <p className="uwu-list-footer">{footer}</p>}
      </div>
    </ShortSheet>
  );
}

/** A toolbar button that opens a menu ("…" on iOS, "⋮" on Android). */
export function MenuButton({
  items,
  label,
  icon,
}: {
  items: readonly ContextMenuEntry[];
  label?: string;
  icon?: LucideIcon;
}) {
  useLanguage();
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  useBackLayer(at !== null, () => setAt(null));
  return (
    <>
      <NavButton
        label={label ?? t('Mehr')}
        icon={icon ?? ICONS.more}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setAt({ x: rect.right - 8, y: rect.bottom + 4 });
        }}
      />
      <ContextMenu
        open={at !== null}
        onClose={() => setAt(null)}
        items={items}
        at={at ?? undefined}
      />
    </>
  );
}

/** Asks once before something goes for good. */
export function useConfirm() {
  const [asking, setAsking] = useState<null | {
    title: string;
    text?: string;
    confirm: string;
    run: () => void;
  }>(null);
  useBackLayer(asking !== null, () => setAsking(null));
  const element = (
    <ShortSheet open={asking !== null} onClose={() => setAsking(null)} title={asking?.title ?? ''}>
      {asking?.text && <p className="m-confirm-text">{asking.text}</p>}
      <div className="m-confirm-actions">
        <BigButton
          danger
          onClick={() => {
            const run = asking?.run;
            setAsking(null);
            run?.();
          }}
        >
          {asking?.confirm}
        </BigButton>
        <BigButton soft onClick={() => setAsking(null)}>
          {t('Abbrechen')}
        </BigButton>
      </div>
    </ShortSheet>
  );
  return { ask: setAsking, element };
}

/** Re-renders every second while `active` (countdowns). */
export function useTick(active: boolean) {
  const [, set] = useState(0);
  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => set((n) => n + 1), 1000);
    return () => window.clearInterval(timer);
  }, [active]);
}
