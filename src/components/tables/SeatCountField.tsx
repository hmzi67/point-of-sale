import { useId } from "react";

export function SeatCountField({ value, onChange, disabled }: {
  value: number;
  onChange: (value: number) => void;
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <fieldset disabled={disabled} className="space-y-2">
      <legend className="mb-2 text-xs font-medium text-fg-muted">Seating capacity</legend>
      <div className="flex flex-wrap gap-1.5">
        {[2, 4, 6, 8, 10, 12].map((seats) => (
          <button key={seats} type="button" aria-pressed={value === seats}
            onClick={() => onChange(seats)}
            className={`rounded-md border px-3 py-1.5 text-sm font-medium transition-colors ${value === seats ? "border-accent bg-accent text-accent-ink" : "border-line-strong bg-surface text-fg-2 hover:bg-surface-3"}`}>
            {seats}
          </button>
        ))}
      </div>
      <div className="flex items-center gap-2">
        <label htmlFor={id} className="text-xs text-fg-muted">Or enter seats</label>
        <input id={id} type="number" min={1} step={1} required
          value={Number.isNaN(value) ? "" : value}
          onChange={(e) => onChange(e.target.valueAsNumber)}
          className="w-24 rounded-md border border-line-strong px-2.5 py-1.5 text-sm" />
      </div>
    </fieldset>
  );
}
