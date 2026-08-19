import { lazy, Suspense, useEffect, useState } from "react";
import { QuickLinks } from "../components/dashboard/QuickLinks";
import { SnapshotCards } from "../components/dashboard/SnapshotCards";
import { useAppConfig } from "../hooks/useAppConfig";
import { useModules } from "../hooks/useModules";
import { getDashboardSummary } from "../services/dashboardService";
import { getSalesOverTime } from "../services/reportsService";
import { resolveDateRangePreset } from "../utils/dateRanges";
import type { DailySales, DashboardSummary } from "../types";

// Recharts is large and only used by the trend chart — deferred so it never
// loads before the rest of the landing page has painted.
const MonthlyTrendChart = lazy(() => import("../components/dashboard/MonthlyTrendChart"));

/** The default landing page for owner/admin. Aggregates Billing, Expenses
 * and Salary — the modules Phase 1's system may or may not have enabled for
 * this client — into one glance at "how's the shop doing". */
export function DashboardPage() {
  const { config } = useAppConfig();
  const { visibleModules } = useModules();

  const [summary, setSummary] = useState<DashboardSummary | null>(null);
  const [series, setSeries] = useState<DailySales[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const today = resolveDateRangePreset("today");
    const monthToDate = resolveDateRangePreset("thisMonth");

    setIsLoading(true);
    setError(null);
    Promise.all([
      getDashboardSummary(today.startDate, today.endDate),
      getSalesOverTime(monthToDate.startDate, monthToDate.endDate),
    ])
      .then(([todaySummary, monthSeries]) => {
        setSummary(todaySummary);
        setSeries(monthSeries);
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  }, []);

  return (
    <section className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold text-slate-900">Dashboard</h2>
        <p className="text-sm text-slate-500">Today's snapshot and this month's trend.</p>
      </div>

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      <SnapshotCards summary={summary} currency={config.currency} isLoading={isLoading} />

      <Suspense fallback={<div className="h-[21rem] animate-pulse rounded-lg border border-slate-200 bg-slate-50" />}>
        <MonthlyTrendChart series={series} currency={config.currency} isLoading={isLoading} />
      </Suspense>

      <QuickLinks modules={visibleModules} />
    </section>
  );
}
