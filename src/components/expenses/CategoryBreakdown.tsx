import { formatMinor } from "../../utils/format";
import type { CategoryTotal } from "../../types";

interface CategoryBreakdownProps {
  totals: CategoryTotal[];
  currency: string;
  isLoading: boolean;
}

/** Category-wise spend for the selected range — a bar-list rather than a
 * full chart, since it's read a handful of times a week, not daily. This is
 * the number Phase 10's dashboard profit calc (sales − expenses − salary)
 * sums straight out of. */
export function CategoryBreakdown({ totals, currency, isLoading }: CategoryBreakdownProps) {
  if (isLoading) {
    return (
      <div className="space-y-2 rounded-2xl border border-line bg-surface p-5">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="h-6 animate-pulse rounded bg-surface-3" />
        ))}
      </div>
    );
  }

  if (totals.length === 0) {
    return (
      <p className="rounded-2xl border border-dashed border-line-strong px-4 py-8 text-center text-sm text-fg-muted">
        No expenses in this range.
      </p>
    );
  }

  const grandTotal = totals.reduce((sum, t) => sum + t.totalMinor, 0);
  const maxTotal = Math.max(...totals.map((t) => t.totalMinor));

  return (
    <div className="space-y-5 rounded-2xl border border-line bg-surface p-5">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h3 className="text-sm font-semibold text-fg">Spending by category</h3>
        <span className="text-sm font-semibold text-fg">{formatMinor(grandTotal, currency)}</span>
      </div>

      <p className="text-xs text-fg-muted">All categories in the selected date range</p>
      <div className="space-y-5">
        {totals.map((t) => (
          <div key={t.category}>
            <div className="flex items-center justify-between text-xs text-fg-2">
              <span className="font-medium">
                {t.category} <span className="text-fg-muted">· {t.count}</span>
              </span>
              <span>{formatMinor(t.totalMinor, currency)}</span>
            </div>
            <div className="mt-2 h-2 rounded-full bg-surface-3">
              <div
                className="h-2 rounded-full bg-linear-to-r from-accent/40 to-accent"
                style={{ width: `${maxTotal > 0 ? (t.totalMinor / maxTotal) * 100 : 0}%` }}
              />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
