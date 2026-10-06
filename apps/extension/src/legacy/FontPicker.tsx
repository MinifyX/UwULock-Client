import { FONT_CHOICES, FONT_NAMES, FONT_STACKS, type FontChoice } from './fonts';

/**
 * Settings → Appearance → Font: every choice shown in itself, as in UwUMail.
 * Shared with the extension, so the words come in as props.
 */
export function FontPicker({
  value,
  onChange,
  label,
  systemName,
  sample,
}: {
  value: FontChoice;
  onChange: (font: FontChoice) => void;
  /** The group's name for screen readers. */
  label: string;
  /** What the system's font is called ("System"). */
  systemName: string;
  /** A line in each font. */
  sample: string;
}) {
  return (
    <div className="font-picker" role="radiogroup" aria-label={label}>
      {FONT_CHOICES.map((choice) => (
        <button
          key={choice}
          type="button"
          role="radio"
          aria-checked={value === choice}
          onClick={() => onChange(choice)}
          style={{ fontFamily: FONT_STACKS[choice] }}
        >
          <span className="font-picker-name">
            {choice === 'system' ? systemName : FONT_NAMES[choice]}
          </span>
          <span className="font-picker-sample">{sample}</span>
        </button>
      ))}
    </div>
  );
}
