import { useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { useReport } from "../../hooks/useReport";
import { getSalesOverTime } from "../../services/reportsService";
import { monthGrid } from "../../utils/monthCalendar";
import { receiptAmount } from "../../utils/receiptFormat";

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** A month at a glance: each day's sales in a calendar, the month's totals on top. */
export function MonthlySalesCalendar({ currency }: { currency: string }) {
  const [month, setMonth] = useState(() => {
    const now = new Date();
    return { year: now.getFullYear(), index: now.getMonth() };
  });
  const { blanks, dates } = monthGrid(month.year, month.index);
  const start = dates[0];
  const end = dates[dates.length - 1];
  const { data, isLoading, error } = useReport(() => getSalesOverTime(start, end), [start, end]);
  const byDate = new Map((data ?? []).map((day) => [day.date, day]));
  const sum = (pick: (day: NonNullable<typeof data>[number]) => number) => (data ?? []).reduce((total, day) => total + pick(day), 0);
  const step = (delta: number) => {
    const next = new Date(month.year, month.index + delta, 1);
    setMonth({ year: next.getFullYear(), index: next.getMonth() });
  };
  const title = new Date(month.year, month.index, 1).toLocaleDateString(undefined, { month: "long", year: "numeric" });

  return (
    <div className="space-y-4 rounded-2xl border border-line bg-surface p-4 sm:p-5">
      <div className="flex items-center justify-between gap-3">
        <button type="button" onClick={() => step(-1)} aria-label="Previous month" className="rounded-lg p-2 text-fg-muted hover:bg-surface-2">
          <ChevronLeft className="h-5 w-5" />
        </button>
        <h3 className="text-base font-semibold text-fg">{title}</h3>
        <button type="button" onClick={() => step(1)} aria-label="Next month" className="rounded-lg p-2 text-fg-muted hover:bg-surface-2">
          <ChevronRight className="h-5 w-5" />
        </button>
      </div>

      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      <dl className="grid grid-cols-3 gap-3 rounded-xl bg-surface-2 p-3 text-center">
        {[
          [`Sales (${currency})`, receiptAmount(sum((day) => day.totalMinor))],
          [`Refunds (${currency})`, receiptAmount(sum((day) => day.refundsMinor))],
          ["Orders", sum((day) => day.transactionCount).toLocaleString()],
        ].map(([label, value]) => (
          <div key={label}>
            <dt className="text-xs text-fg-muted">{label}</dt>
            <dd className="text-base font-bold tabular-nums text-fg">{isLoading ? "…" : value}</dd>
          </div>
        ))}
      </dl>

      <div className="grid grid-cols-7 gap-1 sm:gap-1.5">
        {WEEKDAYS.map((day) => (
          <div key={day} className="pb-1 text-center text-[11px] font-semibold text-fg-muted">
            {day}
          </div>
        ))}
        {Array.from({ length: blanks }, (_, i) => (
          <div key={`blank-${i}`} />
        ))}
        {dates.map((date) => {
          const day = byDate.get(date);
          return (
            <div key={date} className={`min-h-16 rounded-lg border p-1.5 sm:min-h-20 ${day ? "border-line bg-surface-2/50" : "border-line/50"}`}>
              <p className="text-[11px] font-medium text-fg-muted">{Number(date.slice(8))}</p>
              {day && (
                <>
                  <p className="mt-1 truncate text-xs font-bold tabular-nums text-accent-text sm:text-sm" title={receiptAmount(day.totalMinor)}>
                    {receiptAmount(day.totalMinor)}
                  </p>
                  <p className="truncate text-[10px] text-fg-muted">{day.transactionCount} orders</p>
                </>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
