/** A from–to pair of date inputs, each bounded by the other. */
export function DateRangeInputs({
  start,
  end,
  onStartChange,
  onEndChange,
  onBlur,
  label = "Date",
  inputClassName = "rounded-xl border border-line-strong px-3 py-2.5 text-sm",
}: {
  start: string;
  end: string;
  onStartChange: (value: string) => void;
  onEndChange: (value: string) => void;
  /** Fired when either input loses focus — for a caller that applies the
   * range only once editing is done. */
  onBlur?: () => void;
  /** Prefix for the inputs' accessible names ("Report" → "Report start date"). */
  label?: string;
  inputClassName?: string;
}) {
  return (
    <div className="flex items-center gap-1.5">
      <input
        type="date"
        aria-label={`${label} start date`}
        value={start}
        max={end || undefined}
        onChange={(e) => onStartChange(e.target.value)}
        onBlur={onBlur}
        className={inputClassName}
      />
      <span className="text-fg-muted">–</span>
      <input
        type="date"
        aria-label={`${label} end date`}
        value={end}
        min={start || undefined}
        onChange={(e) => onEndChange(e.target.value)}
        onBlur={onBlur}
        className={inputClassName}
      />
    </div>
  );
}
