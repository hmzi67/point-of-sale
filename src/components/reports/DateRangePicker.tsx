import { useState } from "react";
import { useReportsStore } from "../../store";
import { PRESET_LABELS } from "../../utils/dateRanges";
import type { DateRangePreset } from "../../types";

const PRESETS: Exclude<DateRangePreset, "custom">[] = ["today", "thisWeek", "thisMonth"];

export function DateRangePicker() {
  const preset = useReportsStore((state) => state.preset);
  const startDate = useReportsStore((state) => state.startDate);
  const endDate = useReportsStore((state) => state.endDate);
  const setPreset = useReportsStore((state) => state.setPreset);
  const setCustomRange = useReportsStore((state) => state.setCustomRange);

  // Local draft so typing a custom date doesn't refetch on every keystroke —
  // only once both ends of the range are valid and the user has moved on.
  const [draftStart, setDraftStart] = useState(startDate);
  const [draftEnd, setDraftEnd] = useState(endDate);

  const applyCustomRange = () => {
    if (!draftStart || !draftEnd || draftStart > draftEnd) return;
    setCustomRange(draftStart, draftEnd);
  };

  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="flex rounded-md border border-line-strong p-0.5">
        {PRESETS.map((p) => (
          <button
            key={p}
            type="button"
            onClick={() => {
              setPreset(p);
              setDraftStart(startDate);
              setDraftEnd(endDate);
            }}
            className={[
              "rounded px-3 py-1.5 text-sm font-medium transition-colors",
              preset === p ? "bg-accent text-accent-ink" : "text-fg-2 hover:bg-surface-3",
            ].join(" ")}
          >
            {PRESET_LABELS[p]}
          </button>
        ))}
      </div>

      <div className="flex items-center gap-1.5">
        <input
          type="date"
          value={draftStart}
          max={draftEnd || undefined}
          onChange={(e) => setDraftStart(e.target.value)}
          onBlur={applyCustomRange}
          className={[
            "rounded-md border px-2.5 py-1.5 text-sm",
            preset === "custom" ? "border-accent" : "border-line-strong",
          ].join(" ")}
        />
        <span className="text-fg-muted">–</span>
        <input
          type="date"
          value={draftEnd}
          min={draftStart || undefined}
          onChange={(e) => setDraftEnd(e.target.value)}
          onBlur={applyCustomRange}
          className={[
            "rounded-md border px-2.5 py-1.5 text-sm",
            preset === "custom" ? "border-accent" : "border-line-strong",
          ].join(" ")}
        />
      </div>
    </div>
  );
}
