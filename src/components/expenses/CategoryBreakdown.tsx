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
      <div className="space-y-2 rounded-lg border border-slate-200 bg-white p-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="h-6 animate-pulse rounded bg-slate-100" />
        ))}
      </div>
    );
  }

  if (totals.length === 0) {
    return (
      <p className="rounded-lg border border-dashed border-slate-300 px-4 py-8 text-center text-sm text-slate-400">
        No expenses in this range.
      </p>
    );
  }

  const grandTotal = totals.reduce((sum, t) => sum + t.totalMinor, 0);
  const maxTotal = Math.max(...totals.map((t) => t.totalMinor));

  return (
    <div className="space-y-3 rounded-lg border border-slate-200 bg-white p-4">
      <div className="flex items-baseline justify-between">
        <h3 className="text-sm font-semibold text-slate-900">By category</h3>
        <span className="text-sm font-semibold text-slate-900">{formatMinor(grandTotal, currency)}</span>
      </div>

      <div className="space-y-2.5">
        {totals.map((t) => (
          <div key={t.category}>
            <div className="flex items-center justify-between text-xs text-slate-600">
              <span className="font-medium">
                {t.category} <span className="text-slate-400">· {t.count}</span>
              </span>
              <span>{formatMinor(t.totalMinor, currency)}</span>
            </div>
            <div className="mt-1 h-1.5 rounded-full bg-slate-100">
              <div
                className="h-1.5 rounded-full bg-brand-500"
                style={{ width: `${maxTotal > 0 ? (t.totalMinor / maxTotal) * 100 : 0}%` }}
              />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
