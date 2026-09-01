/// A `<select>` that matches the inputs beside it.
///
/// The platform draws its own dropdown arrow at a size and colour we do not
/// control, and on macOS it also forces the control a couple of pixels taller
/// than an `<input>` with identical padding. Invisible on its own; obvious in
/// a row of five, which is exactly where these live. `index.css` turns the
/// native arrow off for every `select.field`, so **every one of them has to
/// come through here** or it renders with no arrow at all — which is how the
/// crawl toolbar lost its chevron the first time this was fixed in one place.
export function SelectField({
  label,
  value,
  onChange,
  options,
  disabled,
  title,
  className = "",
  width = "w-44",
  /// True when the value carries meaning the eye should catch — an applied
  /// filter, a pace that can get a site blocked.
  marked = false,
  tone,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  options: [string, string][];
  disabled?: boolean;
  title?: string;
  className?: string;
  width?: string;
  marked?: boolean;
  /// Overrides the chevron's colour when the control carries its own state.
  tone?: string;
}) {
  return (
    <label className="relative flex shrink-0 items-center">
      <span className="sr-only">{label}</span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        aria-label={label}
        title={title}
        disabled={disabled}
        className={`field ${width} text-sm ${className}`}
      >
        {options.map(([v, text]) => (
          <option key={v} value={v}>
            {text}
          </option>
        ))}
      </select>
      {/* Ours, in the theme's own ink, and `pointer-events-none` so the whole
          control still opens the menu. */}
      <svg
        aria-hidden
        viewBox="0 0 10 6"
        className={`pointer-events-none absolute right-2.5 h-[6px] w-[10px] ${
          tone ?? (marked ? "text-accent-fg" : "text-fg-faint")
        }`}
      >
        <path
          d="M1 1l4 4 4-4"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </label>
  );
}
